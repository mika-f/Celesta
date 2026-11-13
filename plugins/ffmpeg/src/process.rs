//! Shared command handling for the two independently linked programs.

use crate::{FfmpegCodec, check_license};
use celesta_composition::Rational;
use celesta_media::{
    AudioEncoder, AudioEncoderRequest, Color, ComponentKind, MediaError, MediaProvider,
    PixelFormat, Settings, VideoEncoder, VideoEncoderRequest,
};
use celesta_plugin_host::MANIFEST_FLAG;
use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;

/// The flag that has a program refuse a build that links a GPL library it
/// may not ship with (the standard program) or misses one (x264).
pub const CHECK_LICENSE_FLAG: &str = "--check-license";

/// The flag that has a program open every encoder it describes.
pub const CHECK_BUILD_FLAG: &str = "--check-build";

/// What a program run with an argument it does not know exits with, as
/// command-line tools do for a usage error.
const USAGE_EXIT: u8 = 2;

/// Serves one build's components, prints its manifest, or validates a build,
/// and says what the program exits with. `--check-license` is an explicit
/// distribution gate; local use may link a system FFmpeg of either license.
/// `--check-build` actually opens encoders. Protocol mode takes no argument:
/// any other argument, or a second one, is a usage error (the usage on
/// stderr, exit 2), so a misspelled gate, or one an older program does not
/// have, fails rather than serving an empty stdin and exiting 0.
pub fn run_plugin(codec: FfmpegCodec, id: &str, version: &str, manifest: &str) -> ExitCode {
    let mut arguments = std::env::args_os();
    let program = arguments
        .next()
        .and_then(|program| {
            Path::new(&program)
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| id.to_owned());
    let arguments: Vec<OsString> = arguments.collect();
    let Some(command) = Command::parse(&arguments) else {
        let given = arguments
            .iter()
            .map(|argument| argument.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        eprintln!("{program}: unknown arguments: {given}");
        eprintln!("{}", usage(&program));
        return ExitCode::from(USAGE_EXIT);
    };
    match run(codec, command, id, version, manifest) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// What a program was asked to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    /// Serve the protocol on stdin and stdout: no argument.
    Serve,
    Manifest,
    CheckBuild,
    CheckLicense,
}

impl Command {
    /// The command `arguments` (without the program) name, or `None` for
    /// any that is not one alone.
    fn parse(arguments: &[OsString]) -> Option<Self> {
        match arguments {
            [] => Some(Self::Serve),
            [flag] => match flag.to_str()? {
                MANIFEST_FLAG => Some(Self::Manifest),
                CHECK_BUILD_FLAG => Some(Self::CheckBuild),
                CHECK_LICENSE_FLAG => Some(Self::CheckLicense),
                _ => None,
            },
            _ => None,
        }
    }
}

/// How `program` is run.
fn usage(program: &str) -> String {
    format!(
        "usage: {program} [{MANIFEST_FLAG} | {CHECK_BUILD_FLAG} | {CHECK_LICENSE_FLAG}]\n\
         With no argument, it serves Celesta's media plugin protocol on stdin and stdout."
    )
}

fn run(
    mut codec: FfmpegCodec,
    command: Command,
    id: &str,
    version: &str,
    manifest: &str,
) -> Result<(), String> {
    if command == Command::CheckLicense {
        return check_license(id, codec.profile.gpl());
    }
    // A static manifest must never advertise an optional encoder absent from
    // the linked build, including when discovery runs `--manifest`.
    if codec.profile.optional() {
        codec
            .check_components()
            .map_err(|error| format!("{id}: {error}"))?;
    }
    match command {
        Command::Manifest => {
            println!("{manifest}");
            Ok(())
        }
        Command::CheckBuild => codec
            .check_build()
            .map_err(|error| format!("{id}: {error}")),
        Command::CheckLicense => unreachable!("checked first"),
        Command::Serve => celesta_media::protocol::serve(&mut codec, id, version)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(arguments: &[&str]) -> Option<Command> {
        let arguments: Vec<OsString> = arguments.iter().map(OsString::from).collect();
        Command::parse(&arguments)
    }

    #[test]
    fn only_known_flags_alone_are_commands() {
        assert_eq!(parsed(&[]), Some(Command::Serve));
        assert_eq!(parsed(&["--manifest"]), Some(Command::Manifest));
        assert_eq!(parsed(&["--check-build"]), Some(Command::CheckBuild));
        assert_eq!(parsed(&["--check-license"]), Some(Command::CheckLicense));
        for unknown in [
            &["--check-licence"][..],
            &["--help"],
            &[""],
            &["--manifest", "--check-build"],
            &["--manifest", "extra"],
        ] {
            assert_eq!(parsed(unknown), None, "{unknown:?}");
        }
    }
}
