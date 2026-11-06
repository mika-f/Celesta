//! The `celesta-codec-test` program serves the test codec over the codec
//! protocol, answering as [`TestCodec`] does in this process.

use celesta_codec_test::{StoredAudio, StoredVideo, TestCodec, TestMedia, read, write_rgba_video};
use celesta_composition::Rational;
use celesta_media::{
    CodecProcess, CodecProvider, Container, EncoderSettings, MediaError, MuxRequest, PixelFormat,
    VideoRequest,
};
use std::path::{Path, PathBuf};

fn program() -> Box<dyn CodecProvider> {
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_celesta-codec-test"));
    let folder = executable.parent().unwrap().to_path_buf();
    CodecProcess::factory(executable, folder)(&Default::default()).unwrap()
}

#[test]
fn the_program_answers_as_the_codec_in_process_does() {
    let directory = tempfile::tempdir().unwrap();
    let mut process = program();
    let mut local = TestCodec::new();

    let video = directory.path().join("video.ctc");
    let frames = vec![
        vec![1, 2, 3, 4, 5, 6, 7, 8],
        vec![9, 10, 11, 12, 13, 14, 15, 16],
    ];
    write_rgba_video(&video, 2, 1, Rational::new(2, 1), &frames).unwrap();
    assert_eq!(process.probe(&video).unwrap(), local.probe(&video).unwrap());
    for time in [0.0, 0.5] {
        let request = VideoRequest {
            path: &video,
            time,
            sequential: None,
        };
        assert_eq!(
            process.decode_video(&request).unwrap(),
            local.decode_video(&request).unwrap(),
            "at {time}"
        );
    }
    let voice = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/assets/voices/001.wav");
    assert_eq!(
        process.decode_audio(&voice, 44_100, 1).unwrap(),
        local.decode_audio(&voice, 44_100, 1).unwrap()
    );

    let encoded = directory.path().join("encoded.ctc");
    let encoder = process
        .open_encoder(&EncoderSettings {
            output: encoded.clone(),
            width: 2,
            height: 1,
            frame_rate: Rational::new(30, 1),
            input: PixelFormat::Rgba8,
            container: Container::Mp4,
            preset: "medium".into(),
            crf: 18,
            bit_depth: 8,
        })
        .unwrap();
    for frame in &frames {
        process.encode_frame(encoder, frame.clone()).unwrap();
    }
    process.finish_encoder(encoder).unwrap();
    let pcm = directory.path().join("audio.pcm");
    let samples = [0.25f32, -0.5, 0.75, -1.0];
    std::fs::write(
        &pcm,
        samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let output = directory.path().join("muxed.ctc");
    process
        .mux_audio(&MuxRequest {
            video: encoded,
            audio: pcm,
            sample_rate: 48_000,
            channels: 2,
            output: output.clone(),
            container: Container::Mp4,
            lead_in_frames: 1,
        })
        .unwrap();
    assert_eq!(
        read(&output).unwrap(),
        TestMedia {
            container: Some(Container::Mp4),
            video: Some(StoredVideo {
                width: 2,
                height: 1,
                frame_rate: Rational::new(30, 1),
                format: PixelFormat::Rgba8,
                frames,
            }),
            audio: Some(StoredAudio {
                sample_rate: 48_000,
                channels: 2,
                lead_in_frames: 1,
                samples: samples.to_vec(),
            }),
        }
    );

    let missing = directory.path().join("missing.mp4");
    assert!(matches!(
        process.probe(&missing),
        Err(MediaError::NotFound(path)) if path == missing
    ));
}
