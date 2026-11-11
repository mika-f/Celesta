//! Opus packets survive encoding, WebM muxing, and audio decoding.
use celesta_codec_ffmpeg::{FfmpegBackend, FfmpegCodec};
use celesta_media::{
    AudioDecoder, AudioEncoderRequest, MediaProvider, MuxerRequest, Settings, Track, TrackPacket,
};

#[test]
fn opus_audio_round_trips_through_the_standard_webm_muxer() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("tone.webm");
    let settings = Settings::new();
    let samples: Vec<f32> = (0..4800)
        .flat_map(|frame| {
            let value = (frame as f32 * std::f32::consts::TAU * 440.0 / 48000.0).sin() * 0.25;
            [value, value]
        })
        .collect();
    let mut codec = FfmpegCodec::opus();
    let encoder = codec.audio_encoder().unwrap();
    let opened = encoder
        .open_audio(&AudioEncoderRequest {
            component: "opus",
            settings: &settings,
            sample_rate: 48000,
            channels: 2,
        })
        .unwrap();
    let mut encoded = encoder.encode_audio(opened.session, 0, &samples).unwrap();
    let finished = encoder.finish_audio(opened.session).unwrap();
    encoded.config = encoded.config.or(finished.config);
    encoded.packets.extend(finished.packets);
    assert!(!encoded.packets.is_empty());
    let tracks = [Track::Audio {
        config: encoded.config.unwrap(),
        timescale: opened.sample_rate,
        sample_rate: opened.sample_rate,
        channels: opened.channels,
        delay: opened.delay,
    }];
    let mut standard = FfmpegCodec::new();
    let muxer = standard.muxer().unwrap();
    let session = muxer
        .open_muxer(&MuxerRequest {
            component: "webm",
            settings: &settings,
            output: &output,
            tracks: &tracks,
            leading_audio: 0,
        })
        .unwrap();
    let packets: Vec<_> = encoded
        .packets
        .into_iter()
        .map(|packet| TrackPacket { track: 0, packet })
        .collect();
    muxer.write_packets(session, &packets).unwrap();
    muxer.finish_muxer(session).unwrap();
    let decoded = FfmpegBackend::new()
        .decode_audio(&output, 48000, 2)
        .unwrap();
    assert_eq!(decoded.frame_count(), 4800);
    let error: f32 = decoded
        .samples
        .iter()
        .zip(&samples)
        .map(|(actual, expected)| (actual - expected).powi(2))
        .sum::<f32>()
        / samples.len() as f32;
    assert!(error.sqrt() < 0.02, "Opus RMS error: {}", error.sqrt());
}
