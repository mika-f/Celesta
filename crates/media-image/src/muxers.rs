//! The `image` and `imageSequence` muxers: each packet of the one video
//! track is a whole image file, which they write as it comes. Celesta stages
//! and publishes what they write, so they create files where they are told.

use crate::{CODECS, IMAGE, IMAGE_SEQUENCE, ImageComponents, codec_for_extension, sequence_path};
use celesta_media::settings::validate;
use celesta_media::{
    InvalidSetting, MediaError, Muxer, MuxerRequest, SessionId, Track, TrackPacket,
};
use celesta_plugin_host::codec_family;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

/// An open muxer.
pub(crate) struct Session {
    kind: Kind,
    /// The files created so far, in the order written.
    files: Vec<PathBuf>,
}

enum Kind {
    /// `builtin/image`: one frame to `output`.
    Image { output: PathBuf },
    /// `builtin/imageSequence`: each frame to its file of `pattern`.
    Sequence {
        pattern: PathBuf,
        /// A frame's duration in the video track's timescale.
        frame_duration: i64,
        frames: HashSet<u64>,
    },
}

fn tracks_refused(message: impl Into<String>) -> MediaError {
    MediaError::InvalidSettings {
        key: "tracks".to_owned(),
        message: message.into(),
    }
}

/// The one video track's frame duration, once the track is checked against
/// what `component` writes to `output`.
fn check_tracks(component: &str, request: &MuxerRequest<'_>) -> Result<i64, MediaError> {
    let [
        Track::Video {
            config, frame_rate, ..
        },
    ] = request.tracks
    else {
        return Err(tracks_refused(format!(
            "builtin/{component} takes one video track and no audio"
        )));
    };
    let family = codec_family(&config.codec);
    if !CODECS.contains(&family.as_str()) {
        return Err(tracks_refused(format!(
            "builtin/{component} takes png, jpeg or webp video, not {}",
            config.codec
        )));
    }
    let extension = request.output.extension().and_then(OsStr::to_str);
    match extension.and_then(codec_for_extension) {
        Some(codec) if codec == family => {}
        Some(codec) => {
            return Err(tracks_refused(format!(
                "builtin/{component} writes .{} files as {codec}, not {family}",
                extension.unwrap_or_default().to_ascii_lowercase()
            )));
        }
        None => {
            return Err(tracks_refused(format!(
                "builtin/{component} writes .png, .jpg, .jpeg or .webp files, not {}",
                request.output.display()
            )));
        }
    }
    if frame_rate.denominator == 0 {
        return Err(tracks_refused(format!(
            "the video track's frame rate {}/{} is not valid",
            frame_rate.numerator, frame_rate.denominator
        )));
    }
    Ok(i64::from(frame_rate.denominator))
}

/// Creates `path` with `bytes`, whole on disk when it returns; `files`
/// gains it once it exists.
fn write_whole(path: &Path, bytes: &[u8], files: &mut Vec<PathBuf>) -> Result<(), MediaError> {
    let failed = |operation: &str, error: std::io::Error| {
        MediaError::Codec(format!("could not {operation} {}: {error}", path.display()))
    };
    let mut file = File::create(path).map_err(|error| failed("create", error))?;
    files.push(path.to_owned());
    file.write_all(bytes)
        .map_err(|error| failed("write", error))
}

fn no_session(session: SessionId) -> MediaError {
    MediaError::Codec(format!("no open muxer session {}", session.0))
}

impl Muxer for ImageComponents {
    fn open_muxer(&mut self, request: &MuxerRequest<'_>) -> Result<SessionId, MediaError> {
        let component = request.component;
        if component != IMAGE && component != IMAGE_SEQUENCE {
            return Err(MediaError::Codec(format!(
                "no muxer {component:?} in builtin"
            )));
        }
        validate(&[], request.settings).map_err(|InvalidSetting { key, message }| {
            MediaError::InvalidSettings { key, message }
        })?;
        let frame_duration = check_tracks(component, request)?;
        let kind = if component == IMAGE {
            Kind::Image {
                output: request.output.to_owned(),
            }
        } else {
            // A pattern without its one `%06d` fails here, not at a frame.
            sequence_path(request.output, 0)?;
            Kind::Sequence {
                pattern: request.output.to_owned(),
                frame_duration,
                frames: HashSet::new(),
            }
        };
        let session = self.next_session();
        self.muxers.insert(
            session,
            Session {
                kind,
                files: Vec::new(),
            },
        );
        Ok(session)
    }

    fn write_packets(
        &mut self,
        session: SessionId,
        packets: &[TrackPacket],
    ) -> Result<(), MediaError> {
        let open = self
            .muxers
            .get_mut(&session)
            .ok_or_else(|| no_session(session))?;
        for TrackPacket { track, packet } in packets {
            if *track != 0 {
                return Err(MediaError::Codec(format!(
                    "a packet for track {track}, which the muxer does not have"
                )));
            }
            match &mut open.kind {
                Kind::Image { output } => {
                    if !open.files.is_empty() {
                        return Err(MediaError::Codec("builtin/image writes one frame".into()));
                    }
                    write_whole(output, &packet.bytes, &mut open.files)?;
                }
                Kind::Sequence {
                    pattern,
                    frame_duration,
                    frames,
                } => {
                    if packet.pts < 0 {
                        return Err(MediaError::Codec(format!(
                            "builtin/imageSequence was given a frame at {}, before frame 0",
                            packet.pts
                        )));
                    }
                    let frame = (packet.pts / *frame_duration) as u64;
                    if !frames.insert(frame) {
                        return Err(MediaError::Codec(format!(
                            "builtin/imageSequence was given frame {frame} twice"
                        )));
                    }
                    let path = sequence_path(pattern, frame)?;
                    write_whole(&path, &packet.bytes, &mut open.files)?;
                }
            }
        }
        Ok(())
    }

    fn finish_muxer(&mut self, session: SessionId) -> Result<Vec<PathBuf>, MediaError> {
        let open = self
            .muxers
            .remove(&session)
            .ok_or_else(|| no_session(session))?;
        if matches!(open.kind, Kind::Image { .. }) && open.files.is_empty() {
            return Err(MediaError::Codec("builtin/image was given no frame".into()));
        }
        Ok(open.files)
    }

    fn abort_muxer(&mut self, session: SessionId) -> Vec<PathBuf> {
        self.muxers
            .remove(&session)
            .map(|open| open.files)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_composition::Rational;
    use celesta_media::{Color, Packet, Settings, TrackConfig};
    use std::fs;

    fn video(codec: &str) -> Track {
        Track::Video {
            config: TrackConfig {
                codec: codec.to_owned(),
                description: None,
            },
            timescale: 30_000,
            width: 7,
            height: 5,
            frame_rate: Rational::new(30_000, 1_001),
            color: Color::bt709_limited(),
            alpha: false,
        }
    }

    fn open(
        provider: &mut ImageComponents,
        component: &str,
        output: &Path,
        tracks: &[Track],
    ) -> Result<SessionId, MediaError> {
        provider.open_muxer(&MuxerRequest {
            component,
            settings: &Settings::new(),
            output,
            tracks,
            leading_audio: 0,
        })
    }

    fn packet(pts: i64, bytes: &[u8]) -> TrackPacket {
        TrackPacket {
            track: 0,
            packet: Packet {
                pts,
                dts: pts,
                duration: 1_001,
                key: true,
                bytes: bytes.to_vec(),
                additional: Vec::new(),
            },
        }
    }

    fn refused_tracks(result: Result<SessionId, MediaError>) -> bool {
        matches!(result, Err(MediaError::InvalidSettings { key, .. }) if key == "tracks")
    }

    #[test]
    fn image_writes_one_frame() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("a.png");
        let mut provider = ImageComponents::default();
        let session = open(&mut provider, "image", &output, &[video("png")]).unwrap();
        provider
            .write_packets(session, &[packet(0, b"first")])
            .unwrap();
        assert_eq!(fs::read(&output).unwrap(), b"first");
        let error = provider
            .write_packets(session, &[packet(1_001, b"second")])
            .unwrap_err();
        assert_eq!(error.to_string(), "builtin/image writes one frame");
        assert_eq!(provider.finish_muxer(session).unwrap(), [output.as_path()]);
        assert_eq!(fs::read(&output).unwrap(), b"first");

        let empty = open(&mut provider, "image", &output, &[video("png")]).unwrap();
        let error = provider.finish_muxer(empty).unwrap_err();
        assert_eq!(error.to_string(), "builtin/image was given no frame");

        assert!(refused_tracks(open(
            &mut provider,
            "image",
            &output,
            &[video("webp")]
        )));
        let jpeg = dir.path().join("b.JPG");
        assert!(open(&mut provider, "image", &jpeg, &[video("jpeg")]).is_ok());
        assert!(refused_tracks(open(
            &mut provider,
            "image",
            &dir.path().join("b.gif"),
            &[video("png")]
        )));
        assert!(refused_tracks(open(
            &mut provider,
            "image",
            &output,
            &[video("avc1.640028")]
        )));
        assert!(refused_tracks(open(&mut provider, "image", &output, &[])));
        let audio = Track::Audio {
            config: TrackConfig {
                codec: "opus".to_owned(),
                description: None,
            },
            timescale: 48_000,
            sample_rate: 48_000,
            channels: 2,
            delay: 0,
        };
        assert!(refused_tracks(open(
            &mut provider,
            "image",
            &output,
            &[video("png"), audio]
        )));
    }

    #[test]
    fn the_sequence_writes_each_frame_as_it_comes() {
        let dir = tempfile::tempdir().unwrap();
        let pattern = dir.path().join("frame-%06d.png");
        let mut provider = ImageComponents::default();
        let session = open(&mut provider, "imageSequence", &pattern, &[video("png")]).unwrap();
        let third = dir.path().join("frame-000003.png");
        let first = dir.path().join("frame-000000.png");
        provider
            .write_packets(session, &[packet(3 * 1_001, b"three")])
            .unwrap();
        assert_eq!(fs::read(&third).unwrap(), b"three");
        assert!(!first.exists());
        provider
            .write_packets(session, &[packet(0, b"zero")])
            .unwrap();
        assert_eq!(fs::read(&first).unwrap(), b"zero");
        assert!(matches!(
            provider.write_packets(session, &[packet(3 * 1_001, b"again")]),
            Err(MediaError::Codec(_))
        ));
        assert_eq!(fs::read(&third).unwrap(), b"three");
        assert!(matches!(
            provider.write_packets(session, &[packet(-1, b"early")]),
            Err(MediaError::Codec(_))
        ));
        assert_eq!(
            provider.finish_muxer(session).unwrap(),
            [third.clone(), first.clone()]
        );

        // Abort lists what was written and removes none of it.
        let other = dir.path().join("other-%06d.png");
        let session = open(&mut provider, "imageSequence", &other, &[video("png")]).unwrap();
        provider
            .write_packets(session, &[packet(1_001, b"one")])
            .unwrap();
        let written = dir.path().join("other-000001.png");
        assert_eq!(provider.abort_muxer(session), [written.as_path()]);
        assert!(written.exists());
        assert_eq!(provider.abort_muxer(session), Vec::<PathBuf>::new());

        assert!(matches!(
            open(
                &mut provider,
                "imageSequence",
                &dir.path().join("frame.png"),
                &[video("png")]
            ),
            Err(MediaError::Codec(_))
        ));
        assert!(refused_tracks(open(
            &mut provider,
            "imageSequence",
            &pattern,
            &[video("jpeg")]
        )));
    }

    #[test]
    fn muxers_share_the_encoders_counter() {
        let dir = tempfile::tempdir().unwrap();
        let mut provider = ImageComponents::default();
        let output = dir.path().join("a.png");
        let encoder = celesta_media::VideoEncoder::open_video(
            &mut provider,
            &celesta_media::VideoEncoderRequest {
                component: "png",
                settings: &Settings::new(),
                width: 7,
                height: 5,
                frame_rate: Rational::new(30_000, 1_001),
                timescale: 30_000,
                offered: &[celesta_media::PixelFormat::Rgba8],
                color: &Color::bt709_limited(),
            },
        )
        .unwrap();
        let a = open(&mut provider, "image", &output, &[video("png")]).unwrap();
        let b = open(&mut provider, "image", &output, &[video("png")]).unwrap();
        assert_eq!(
            (encoder.session, a, b),
            (SessionId(1), SessionId(2), SessionId(3))
        );
        // An encoder's session is not a muxer's.
        assert!(provider.finish_muxer(encoder.session).is_err());
        assert!(matches!(
            open(&mut provider, "mp4", &output, &[video("png")]),
            Err(MediaError::Codec(_))
        ));
        let settings: Settings =
            [("speed".to_owned(), celesta_media::SettingValue::Number(1.0))].into();
        let error = provider
            .open_muxer(&MuxerRequest {
                component: "image",
                settings: &settings,
                output: &output,
                tracks: &[video("png")],
                leading_audio: 0,
            })
            .unwrap_err();
        assert!(
            matches!(&error, MediaError::InvalidSettings { key, .. } if key == "speed"),
            "{error:?}"
        );
    }
}
