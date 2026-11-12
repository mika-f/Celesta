//! The page's words for what a resolution and the presets files tell it:
//! the facts of a [`ResolveWarning`] worded for the page, not the CLI.

use super::model::ExportModel;
use celesta_exporter::{PresetSource, ResolveWarning};
use celesta_media::{ComponentKind, ComponentRef};

/// A warning the page shows under the Preset or Container row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PageWarning {
    /// What a resolve found.
    Resolve(ResolveWarning),
    /// A presets file that could not be read; its presets are not listed.
    UnreadableFile {
        source: PresetSource,
        message: String,
    },
    /// The project's `default` names a preset found in no file.
    UnknownDefault { name: String },
}

impl PageWarning {
    /// What the warning calls the base it is about (``preset `web` ``,
    /// `your last export`), when it is about one.
    pub(crate) fn base(&self) -> Option<&str> {
        let PageWarning::Resolve(warning) = self else {
            return None;
        };
        match warning {
            ResolveWarning::Unavailable { base, .. }
            | ResolveWarning::NoAudio { base, .. }
            | ResolveWarning::AudioOptionsIgnored { base }
            | ResolveWarning::StaleValue { base, .. }
            | ResolveWarning::UnknownSetting { base, .. } => Some(base),
            ResolveWarning::Extension { .. } | ResolveWarning::Lookup(_) => None,
        }
    }
}

/// A resolve's failure or an exporter's text as the page shows it: the
/// exporter's `code` quoting (the CLI's) becomes typographic quotes.
pub(crate) fn plain(text: &str) -> String {
    let mut open = false;
    text.chars()
        .map(|character| match character {
            '`' => {
                open = !open;
                if open { '“' } else { '”' }
            }
            other => other,
        })
        .collect()
}

impl ExportModel {
    /// `warning` as the page shows it under the Preset or Container row.
    pub(crate) fn warning_text(&self, warning: &PageWarning) -> String {
        match warning {
            PageWarning::Resolve(warning) => self.resolve_text(warning),
            PageWarning::UnreadableFile { source, message } => {
                let which = match source {
                    PresetSource::Project => {
                        "This project’s presets file couldn’t be read, so its presets aren’t listed."
                    }
                    PresetSource::User => {
                        "Your presets file couldn’t be read, so your presets aren’t listed."
                    }
                    PresetSource::File => {
                        "The presets file couldn’t be read, so its presets aren’t listed."
                    }
                };
                format!("{which} {}", plain(message))
            }
            PageWarning::UnknownDefault { name } => {
                format!("The project’s default preset “{name}” isn’t in any presets file.")
            }
        }
    }

    fn resolve_text(&self, warning: &ResolveWarning) -> String {
        match warning {
            ResolveWarning::Unavailable {
                kind, text, reason, ..
            } => {
                let label = self.labelled(text, *kind);
                let fallback = if *kind == ComponentKind::Muxer {
                    "container"
                } else {
                    "encoder"
                };
                format!("{label} is unavailable: {reason}. The default {fallback} is used.")
            }
            ResolveWarning::NoAudio { text, muxer, .. } => {
                let audio = self.labelled(text, ComponentKind::AudioEncoder);
                let muxer = self.labelled_ref(muxer, ComponentKind::Muxer);
                format!("{audio} is not used: {muxer} takes no audio. The audio is left out.")
            }
            ResolveWarning::AudioOptionsIgnored { .. } => {
                "Audio settings are ignored: the preset leaves the audio out.".to_owned()
            }
            ResolveWarning::StaleValue { label, message, .. } => {
                format!("{label} {message}. The default is used.")
            }
            ResolveWarning::UnknownSetting { component, key, .. } => {
                let name = self.name_of(component);
                format!("{name} has no setting “{key}”. It is left out.")
            }
            ResolveWarning::Extension {
                muxer_name,
                extension,
                ..
            } => format!(
                "{muxer_name} doesn’t write .{extension} files. \
                 The file is written as {muxer_name} anyway."
            ),
            ResolveWarning::Lookup(text) => plain(text),
        }
    }

    /// The label of the component `text` names, else `text` as written.
    fn labelled(&self, text: &str, kind: ComponentKind) -> String {
        match text.parse::<ComponentRef>() {
            Ok(reference) => self.labelled_ref(&reference, kind),
            Err(_) => text.to_owned(),
        }
    }

    fn labelled_ref(&self, reference: &ComponentRef, kind: ComponentKind) -> String {
        self.component_label(reference, kind)
            .unwrap_or_else(|| reference.to_string())
    }

    /// The name of the component `reference`, of any kind, else its reference.
    fn name_of(&self, reference: &ComponentRef) -> String {
        self.component_name(reference)
            .unwrap_or_else(|| reference.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export_page::model::PresetFileState;
    use celesta_media::MediaRegistry;

    const VIDEO: &str = "celesta-test-codec/video";
    const AUDIO: &str = "celesta-test-codec/pcm";
    const MUXER: &str = "celesta-test-codec/file";

    fn model() -> ExportModel {
        let registry = MediaRegistry::new(vec![celesta_codec_test::registered()]);
        let empty = || PresetFileState {
            path: None,
            file: Ok(None),
        };
        ExportModel::new(&registry, empty(), empty())
    }

    fn reference(text: &str) -> ComponentRef {
        text.parse().unwrap()
    }

    fn text(warning: ResolveWarning) -> String {
        model().warning_text(&PageWarning::Resolve(warning))
    }

    #[test]
    fn a_stale_value_names_its_field() {
        assert_eq!(
            text(ResolveWarning::StaleValue {
                base: "preset `web`".into(),
                component: reference(VIDEO),
                key: "crf".into(),
                label: "CRF".into(),
                message: "must be at most 51".into(),
            }),
            "CRF must be at most 51. The default is used."
        );
    }

    #[test]
    fn an_unknown_setting_names_its_component() {
        assert_eq!(
            text(ResolveWarning::UnknownSetting {
                base: "preset `web`".into(),
                component: reference(VIDEO),
                key: "speed".into(),
            }),
            "Test video encoder has no setting “speed”. It is left out."
        );
    }

    #[test]
    fn an_unavailable_component_falls_back_to_the_default() {
        let unavailable = |kind, text: &str| ResolveWarning::Unavailable {
            base: "preset `web`".into(),
            kind,
            text: text.into(),
            reason: "no libx264".into(),
        };
        assert_eq!(
            text(unavailable(ComponentKind::VideoEncoder, VIDEO)),
            "Test video encoder · Test codec is unavailable: no libx264. \
             The default encoder is used."
        );
        assert_eq!(
            text(unavailable(ComponentKind::Muxer, MUXER)),
            "Test muxer · Test codec is unavailable: no libx264. \
             The default container is used."
        );
    }

    #[test]
    fn audio_a_muxer_does_not_take_is_left_out() {
        assert_eq!(
            text(ResolveWarning::NoAudio {
                base: "preset `web`".into(),
                text: AUDIO.into(),
                muxer: reference(MUXER),
            }),
            "Test audio encoder · Test codec is not used: \
             Test muxer · Test codec takes no audio. The audio is left out."
        );
    }

    #[test]
    fn audio_settings_with_no_audio_are_ignored() {
        assert_eq!(
            text(ResolveWarning::AudioOptionsIgnored {
                base: "preset `web`".into(),
            }),
            "Audio settings are ignored: the preset leaves the audio out."
        );
    }

    #[test]
    fn an_extension_the_muxer_does_not_write() {
        assert_eq!(
            text(ResolveWarning::Extension {
                muxer: reference(MUXER),
                muxer_name: "MP4".into(),
                extension: "mov".into(),
            }),
            "MP4 doesn’t write .mov files. The file is written as MP4 anyway."
        );
    }

    #[test]
    fn a_lookup_warning_is_quoted_plainly() {
        assert_eq!(
            text(ResolveWarning::Lookup(
                "the project's preset `web` hides the user's".into()
            )),
            "the project's preset “web” hides the user's"
        );
    }

    #[test]
    fn plain_quotes_code_marks() {
        assert_eq!(
            plain("preset `Web`: b/x264 has no setting `speed`; it is left out"),
            "preset “Web”: b/x264 has no setting “speed”; it is left out"
        );
    }

    #[test]
    fn an_unreadable_file_says_whose_it_is() {
        let warn = |source| {
            model().warning_text(&PageWarning::UnreadableFile {
                source,
                message: "m".into(),
            })
        };
        assert_eq!(
            warn(PresetSource::Project),
            "This project’s presets file couldn’t be read, so its presets aren’t listed. m"
        );
        assert_eq!(
            warn(PresetSource::User),
            "Your presets file couldn’t be read, so your presets aren’t listed. m"
        );
    }

    #[test]
    fn an_unreadable_file_quotes_its_message_plainly() {
        assert_eq!(
            model().warning_text(&PageWarning::UnreadableFile {
                source: PresetSource::User,
                message: "preset `Web` is listed twice".into(),
            }),
            "Your presets file couldn’t be read, so your presets aren’t listed. \
             preset “Web” is listed twice"
        );
    }

    #[test]
    fn an_unreadable_other_file_is_worded_neutrally() {
        assert_eq!(
            model().warning_text(&PageWarning::UnreadableFile {
                source: PresetSource::File,
                message: "m".into(),
            }),
            "The presets file couldn’t be read, so its presets aren’t listed. m"
        );
    }

    #[test]
    fn text_that_is_no_reference_is_worded_as_written() {
        assert_eq!(
            text(ResolveWarning::Unavailable {
                base: "preset `web`".into(),
                kind: ComponentKind::VideoEncoder,
                text: "x264".into(),
                reason: "not installed".into(),
            }),
            "x264 is unavailable: not installed. The default encoder is used."
        );
        assert_eq!(
            text(ResolveWarning::NoAudio {
                base: "preset `web`".into(),
                text: "aac".into(),
                muxer: reference(MUXER),
            }),
            "aac is not used: Test muxer · Test codec takes no audio. The audio is left out."
        );
    }

    #[test]
    fn an_unknown_default_names_the_preset() {
        assert_eq!(
            model().warning_text(&PageWarning::UnknownDefault { name: "Web".into() }),
            "The project’s default preset “Web” isn’t in any presets file."
        );
    }

    #[test]
    fn an_unknown_reference_is_worded_as_written() {
        assert_eq!(
            text(ResolveWarning::Unavailable {
                base: "preset `web`".into(),
                kind: ComponentKind::VideoEncoder,
                text: "gone/x264".into(),
                reason: "not installed".into(),
            }),
            "gone/x264 is unavailable: not installed. The default encoder is used."
        );
        assert_eq!(
            text(ResolveWarning::UnknownSetting {
                base: "preset `web`".into(),
                component: reference("gone/x264"),
                key: "speed".into(),
            }),
            "gone/x264 has no setting “speed”. It is left out."
        );
    }
}
