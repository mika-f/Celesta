//! What an export resolution tells the caller before it starts.

use celesta_media::{ComponentKind, ComponentRef};
use std::fmt;

/// A resolution's warning: the facts, so a caller can word them for its own
/// surface; `Display` is the CLI's wording.
#[derive(Clone, Debug, PartialEq)]
pub enum ResolveWarning {
    /// A base's component that cannot be used; the default is used.
    Unavailable {
        base: String,
        kind: ComponentKind,
        text: String,
        reason: String,
    },
    /// A base's audio encoder with a muxer that takes no audio.
    NoAudio {
        base: String,
        text: String,
        muxer: ComponentRef,
    },
    /// Audio options given with a base that leaves the audio out.
    AudioOptionsIgnored { base: String },
    /// A base's value the schema does not take; the default is used.
    StaleValue {
        base: String,
        component: ComponentRef,
        key: String,
        label: String,
        message: String,
    },
    /// A base's value for a key the schema does not have.
    UnknownSetting {
        base: String,
        component: ComponentRef,
        key: String,
    },
    /// A named muxer that does not list the output's extension.
    Extension {
        muxer: ComponentRef,
        muxer_name: String,
        extension: String,
    },
    /// The preset lookup's and the project default's warnings, in the CLI's words.
    Lookup(String),
}

impl fmt::Display for ResolveWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable {
                base, text, reason, ..
            } => write!(
                f,
                "{base}: {text} is unavailable ({reason}); using the default instead"
            ),
            Self::NoAudio { base, text, muxer } => write!(
                f,
                "{base}: {text} is unavailable ({muxer} takes no audio); the audio is left out"
            ),
            Self::AudioOptionsIgnored { base } => {
                write!(f, "audio options are ignored: {base} leaves the audio out")
            }
            Self::StaleValue {
                base,
                component,
                key,
                message,
                ..
            } => write!(f, "{base}: {component} {key} {message}; using its default"),
            Self::UnknownSetting {
                base,
                component,
                key,
            } => write!(
                f,
                "{base}: {component} has no setting `{key}`; it is left out"
            ),
            Self::Extension {
                muxer,
                muxer_name,
                extension,
            } => write!(
                f,
                "{muxer} does not write .{extension} files; the file is written as {muxer_name} anyway"
            ),
            Self::Lookup(text) => f.write_str(text),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(text: &str) -> ComponentRef {
        text.parse().unwrap()
    }

    #[test]
    fn resolve_warnings_read_as_before() {
        let base = || "preset `Web`".to_owned();
        let cases = [
            (
                ResolveWarning::Unavailable {
                    base: base(),
                    kind: ComponentKind::VideoEncoder,
                    text: "ffmpeg/libx264".into(),
                    reason: "no libx264".into(),
                },
                "preset `Web`: ffmpeg/libx264 is unavailable (no libx264); using the default instead",
            ),
            (
                ResolveWarning::NoAudio {
                    base: base(),
                    text: "ffmpeg/aac".into(),
                    muxer: reference("ffmpeg/ivf"),
                },
                "preset `Web`: ffmpeg/aac is unavailable (ffmpeg/ivf takes no audio); the audio is left out",
            ),
            (
                ResolveWarning::AudioOptionsIgnored { base: base() },
                "audio options are ignored: preset `Web` leaves the audio out",
            ),
            (
                ResolveWarning::StaleValue {
                    base: base(),
                    component: reference("ffmpeg/libx264"),
                    key: "crf".into(),
                    label: "CRF".into(),
                    message: "must be at most 51".into(),
                },
                "preset `Web`: ffmpeg/libx264 crf must be at most 51; using its default",
            ),
            (
                ResolveWarning::UnknownSetting {
                    base: base(),
                    component: reference("ffmpeg/libx264"),
                    key: "speed".into(),
                },
                "preset `Web`: ffmpeg/libx264 has no setting `speed`; it is left out",
            ),
            (
                ResolveWarning::Extension {
                    muxer: reference("ffmpeg/mp4"),
                    muxer_name: "MP4".into(),
                    extension: "mov".into(),
                },
                "ffmpeg/mp4 does not write .mov files; the file is written as MP4 anyway",
            ),
            (ResolveWarning::Lookup("x".into()), "x"),
        ];
        for (warning, text) in cases {
            assert_eq!(warning.to_string(), text);
        }
    }
}
