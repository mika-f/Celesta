//! Writing exports: the video encoders, and muxing the mixed audio in.

use crate::sequential::ffmpeg_path;
use celesta_media::{Container, EncoderSettings, MediaError, MuxRequest, PixelFormat};
use ez_ffmpeg::{FfmpegContext, Input, Output, VideoWriter, capabilities};

/// The largest H.264 constant rate factor libx264 takes for 8-bit output.
const MAX_CRF: u8 = 51;

/// The video and audio encoders `container` needs, which a stripped
/// FFmpeg build may lack.
const fn encoders(container: Container) -> [&'static str; 2] {
    match container {
        Container::Mp4 => ["libx264", "aac"],
        Container::Mov => ["prores_ks", "pcm_s24le"],
        Container::Webm => ["libvpx-vp9", "libopus"],
    }
}

/// Fails when the linked FFmpeg lacks an encoder `container` needs.
pub(crate) fn check_encoder(container: Container) -> Result<(), MediaError> {
    match encoders(container)
        .into_iter()
        .find(|encoder| !capabilities::is_encoder_available(encoder))
    {
        Some(encoder) => Err(MediaError::EncoderUnavailable {
            encoder: encoder.to_owned(),
        }),
        None => Ok(()),
    }
}

/// Opens a constant-frame-rate `VideoWriter` for pushed frames in
/// `settings.input` layout — the library-linked equivalent of piping
/// `rawvideo` into
///
/// - `Mp4`: `ffmpeg -c:v libx264 -preset <preset> -crf <crf> -pix_fmt yuv420p -movflags +faststart`,
///   with `-pix_fmt yuv420p10le` (High 10) for 10-bit output. Input already
///   in the output's pixel format reaches the encoder without a conversion.
/// - `Mov`: `ffmpeg -vf <BT709_FILTER> -c:v prores_ks -profile:v 4444 -alpha_bits 16 -vendor apl0 -pix_fmt yuva444p10le`
/// - `Webm`: `ffmpeg -vf <BT709_FILTER> -c:v libvpx-vp9 -crf 20 -b:v 0 -row-mt 1 -pix_fmt yuva420p`
///
/// The alpha formats take straight-alpha `rgba64le`, which libswscale
/// converts with the BT.709 matrix in limited range (see [`BT709_FILTER`]);
/// the preset, CRF, and bit depth apply to `Mp4` only.
pub(crate) fn open_video_writer(settings: &EncoderSettings) -> Result<VideoWriter, MediaError> {
    if settings.container == Container::Mp4 && settings.crf > MAX_CRF {
        return Err(MediaError::Codec(format!(
            "H.264 CRF must be between 0 and {MAX_CRF}, got {}",
            settings.crf
        )));
    }
    let rate = settings.frame_rate;
    let (Ok(fps_num), Ok(fps_den)) = (
        i32::try_from(rate.numerator),
        i32::try_from(rate.denominator),
    ) else {
        return Err(MediaError::Codec(format!(
            "frame rate {}/{} is too large",
            rate.numerator, rate.denominator
        )));
    };
    let output = Output::from(ffmpeg_path(&settings.output));
    let output = match settings.container {
        Container::Mp4 => output
            .set_video_codec("libx264")
            .set_video_codec_opt("preset", settings.preset.as_str())
            .set_video_codec_opt("crf", settings.crf.to_string())
            .set_pix_fmt(if settings.bit_depth == 10 {
                "yuv420p10le"
            } else {
                "yuv420p"
            })
            .set_format_opt("movflags", "+faststart"),
        // No faststart here: this is the video-only intermediate, and
        // `mux_audio` sets it on the final file.
        Container::Mov => output
            .set_video_codec("prores_ks")
            .set_video_codec_opt("profile", "4444")
            .set_video_codec_opt("alpha_bits", "16")
            .set_video_codec_opt("vendor", "apl0")
            .set_pix_fmt("yuva444p10le")
            .set_video_filter(BT709_FILTER),
        Container::Webm => output
            .set_video_codec("libvpx-vp9")
            .set_video_codec_opt("crf", "20")
            .set_video_codec_opt("b", "0")
            .set_video_codec_opt("row-mt", "1")
            .set_pix_fmt("yuva420p")
            .set_video_filter(BT709_FILTER),
    };
    VideoWriter::builder(settings.width, settings.height)
        .pixel_format(match settings.input {
            PixelFormat::Rgba8 => "rgba",
            PixelFormat::Rgba16 => "rgba64le",
            PixelFormat::Yuv420p => "yuv420p",
            PixelFormat::Yuv420p10 => "yuv420p10le",
        })
        .fps(fps_num, fps_den)
        .open(output)
        .map_err(|error| stage_error("video encoding", error))
}

/// Converts RGB to YUV with the BT.709 matrix in limited range and tags the
/// frames BT.709 (matrix, range, primaries and transfer), since editing
/// software assumes BT.709 for untagged HD material. The tags go on the
/// frames because the encoder copies its color properties from the first
/// frame after applying its options, so `colorspace`, `color_primaries` and
/// `color_trc` encoder options would not take effect.
const BT709_FILTER: &str =
    "scale=out_color_matrix=bt709:out_range=tv,setparams=color_primaries=bt709:color_trc=bt709";

/// Hands the frame's pixel buffer to the encoder as is (`write_owned`),
/// rather than having `write` copy all of it first.
pub(crate) fn write_frame(writer: &mut VideoWriter, pixels: Vec<u8>) -> Result<(), MediaError> {
    writer
        .write_owned(pixels)
        .map_err(|error| stage_error("video encoding", error.into_parts().1.into()))
}

/// Finalizes an encode: writes the container trailer.
pub(crate) fn finish(writer: VideoWriter) -> Result<(), MediaError> {
    writer
        .finish()
        .map_err(|error| stage_error("video encoding", error))
}

/// Muxes the encoded, audio-less video with the staged `f32le` PCM,
/// stream-copying the video and encoding the container's audio codec — the
/// library-linked equivalent of a second
/// `ffmpeg -i video -f f32le -i pcm -c:v copy -c:a <codec>` pass: AAC 192k
/// for MP4, 24-bit PCM for MOV, and Opus 192k at 48 kHz for WebM. Lead-in
/// frames are timed before 0, which the MP4 muxer turns into an edit list.
pub(crate) fn mux_audio(request: &MuxRequest) -> Result<(), MediaError> {
    // No `-shortest`: the mix already spans the video. With a copied WebM
    // video it cut the audio a frame short, and it lets the muxer finish
    // while the audio encoder still sends, which ez-ffmpeg can report as
    // "Muxer already finished".
    let muxed = Output::from(ffmpeg_path(&request.output))
        .add_stream_map_with_copy("0:v:0")
        .add_stream_map("1:a:0");
    let muxed = match request.lead_in_frames {
        0 => muxed,
        frames => muxed.set_audio_filter(format!("asetpts=PTS-{frames}/SR/TB")),
    };
    let context = FfmpegContext::builder()
        .input(Input::from(ffmpeg_path(&request.video)))
        .input(
            Input::from(ffmpeg_path(&request.audio))
                .set_format("f32le")
                .set_format_opt("sample_rate", request.sample_rate.to_string())
                .set_format_opt("ch_layout", format!("{}c", request.channels)),
        )
        .output(match request.container {
            Container::Mp4 => muxed
                .set_audio_codec("aac")
                .set_audio_codec_opt("b", "192k")
                .set_format_opt("movflags", "+faststart"),
            Container::Mov => muxed
                .set_audio_codec("pcm_s24le")
                .set_format_opt("movflags", "+faststart"),
            // libopus takes 48, 24, 16, 12 or 8 kHz only, so a 44.1 kHz
            // mix is resampled.
            Container::Webm => muxed
                .set_audio_codec("libopus")
                .set_audio_codec_opt("b", "192k")
                .set_audio_sample_rate(48_000),
        })
        .build()
        .map_err(|error| stage_error("audio muxing", error))?;
    context
        .start()
        .and_then(|running| running.wait())
        .map_err(|error| stage_error("audio muxing", error))
}

fn stage_error(stage: &str, error: ez_ffmpeg::error::Error) -> MediaError {
    MediaError::Codec(format!("FFmpeg {stage} failed: {error}"))
}
