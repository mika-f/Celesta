//! Shared command handling for the four independently linked programs.

use crate::{FfmpegCodec, Profile, check_license};
use celesta_composition::Rational;
use celesta_media::{
    AudioEncoder, AudioEncoderRequest, Color, ComponentKind, MediaError, MediaProvider,
    PixelFormat, Settings, VideoEncoder, VideoEncoderRequest,
};
use celesta_plugin_host::MANIFEST_FLAG;

/// Serves one build's components, prints its manifest, or validates a build.
/// `--check-license` is an explicit distribution gate; local use may link a
/// system FFmpeg of either license. `--check-build` actually opens encoders.
pub fn run_plugin(
    mut codec: FfmpegCodec,
    id: &str,
    version: &str,
    manifest: &str,
) -> Result<(), String> {
    let argument = std::env::args().nth(1);
    if argument.as_deref() == Some("--check-license") {
        return check_license(id, codec.profile == Profile::X264);
    }
    // A static manifest must never advertise an optional encoder absent from
    // the linked build, including when discovery runs `--manifest`.
    if codec.profile != Profile::Standard {
        codec
            .check_components()
            .map_err(|error| format!("{id}: {error}"))?;
    }
    match argument.as_deref() {
        Some(MANIFEST_FLAG) => {
            println!("{manifest}");
            Ok(())
        }
        Some("--check-build") => codec
            .check_build()
            .map_err(|error| format!("{id}: {error}")),
        _ => celesta_media::protocol::serve(&mut codec, id, version)
            .map_err(|error| format!("{id}: {error}")),
    }
}

impl FfmpegCodec {
    fn check_components(&mut self) -> Result<(), MediaError> {
        for component in self.describe()? {
            if !component.available {
                return Err(MediaError::ComponentUnavailable {
                    component: component.id,
                    reason: component
                        .reason
                        .unwrap_or_else(|| "not available in this FFmpeg build".into()),
                });
            }
        }
        Ok(())
    }

    fn check_build(&mut self) -> Result<(), MediaError> {
        self.check_components()?;
        let settings = Settings::new();
        let color = Color::bt709_limited();
        for component in self.describe()? {
            match component.kind {
                ComponentKind::VideoEncoder => {
                    let opened = self.open_video(&VideoEncoderRequest {
                        component: &component.id,
                        settings: &settings,
                        width: 64,
                        height: 32,
                        frame_rate: Rational::new(25, 1),
                        timescale: 25,
                        offered: &[PixelFormat::Rgba16, PixelFormat::Rgba8],
                        color: &color,
                    })?;
                    self.abort_video(opened.session);
                }
                ComponentKind::AudioEncoder => {
                    let opened = self.open_audio(&AudioEncoderRequest {
                        component: &component.id,
                        settings: &settings,
                        sample_rate: 48_000,
                        channels: 2,
                    })?;
                    self.abort_audio(opened.session);
                }
                _ => {}
            }
        }
        Ok(())
    }
}
