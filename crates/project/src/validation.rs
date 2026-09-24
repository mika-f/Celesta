use std::collections::HashSet;
use std::error::Error;
use std::fmt;

use celesta_composition::{Animatable, Paint, TextStyle, Time, Transform};

use crate::{AssetKind, Character, Project, SourceRange, TimelineContent, TimelineItem};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError {
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationErrors(Vec<ValidationError>);

impl ValidationErrors {
    pub fn as_slice(&self) -> &[ValidationError] {
        &self.0
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, error) in self.0.iter().enumerate() {
            if index != 0 {
                formatter.write_str("; ")?;
            }
            write!(formatter, "{}: {}", error.path, error.message)?;
        }
        Ok(())
    }
}

impl Error for ValidationErrors {}

struct Validator<'project> {
    project: &'project Project,
    errors: Vec<ValidationError>,
}

impl Project {
    pub fn validate(&self) -> Result<(), ValidationErrors> {
        let mut validator = Validator {
            project: self,
            errors: Vec::new(),
        };
        validator.validate();

        if validator.errors.is_empty() {
            Ok(())
        } else {
            Err(ValidationErrors(validator.errors))
        }
    }
}

impl Validator<'_> {
    fn validate(&mut self) {
        if self.project.settings.width == 0 {
            self.error("settings.width", "must be greater than zero");
        }
        if self.project.settings.height == 0 {
            self.error("settings.height", "must be greater than zero");
        }
        if !self.project.settings.frame_rate.is_valid() {
            self.error(
                "settings.frameRate",
                "numerator and denominator must be positive",
            );
        }
        if self.project.settings.sample_rate == 0 {
            self.error("settings.sampleRate", "must be greater than zero");
        }
        if let Some(volume) = self.project.settings.master_volume
            && (!volume.is_finite() || volume < 0.0)
        {
            self.error("settings.masterVolume", "must be finite and non-negative");
        }
        if let Some(duration) = self.project.settings.duration {
            self.time("settings.duration", duration, false);
        }

        let characters: Vec<_> = self
            .project
            .characters
            .iter()
            .map(|(id, character)| (id.clone(), character.clone()))
            .collect();
        for (id, character) in characters {
            self.character(&id, &character);
        }

        let mut track_ids = HashSet::new();
        let mut item_ids = HashSet::new();
        let tracks = self.project.tracks.clone();
        for (track_index, track) in tracks.iter().enumerate() {
            let path = format!("tracks[{track_index}]");
            if track.id.is_empty() {
                self.error(format!("{path}.id"), "must not be empty");
            } else if !track_ids.insert(&track.id) {
                self.error(format!("{path}.id"), "must be unique");
            }
            for (item_index, item) in track.items.iter().enumerate() {
                let item_path = format!("{path}.items[{item_index}]");
                if item.id.is_empty() {
                    self.error(format!("{item_path}.id"), "must not be empty");
                } else if !item_ids.insert(&item.id) {
                    self.error(
                        format!("{item_path}.id"),
                        "must be unique across the project",
                    );
                }
                self.item(&item_path, item);
            }
        }
    }

    fn character(&mut self, id: &str, character: &Character) {
        let path = format!("characters.{id}");
        if let Some(portrait) = &character.portrait {
            if !portrait
                .expressions
                .contains_key(&portrait.default_expression)
            {
                self.error(
                    format!("{path}.portrait.defaultExpression"),
                    "must name an expression in portrait.expressions",
                );
            }
            for (expression, asset) in &portrait.expressions {
                self.asset_ref(
                    &format!("{path}.portrait.expressions.{expression}"),
                    asset,
                    AssetKind::Image,
                );
            }
            if let Some(transform) = &portrait.transform {
                self.transform(&format!("{path}.portrait.transform"), transform);
            }
            if let Some(lip_sync) = &portrait.lip_sync {
                for (shape, asset) in [
                    ("a", &lip_sync.a),
                    ("i", &lip_sync.i),
                    ("u", &lip_sync.u),
                    ("e", &lip_sync.e),
                    ("o", &lip_sync.o),
                ] {
                    self.asset_ref(
                        &format!("{path}.portrait.lipSync.{shape}"),
                        asset,
                        AssetKind::Image,
                    );
                }
                if let Some(asset) = &lip_sync.closed {
                    self.asset_ref(
                        &format!("{path}.portrait.lipSync.closed"),
                        asset,
                        AssetKind::Image,
                    );
                }
                if let Some(transform) = &lip_sync.transform {
                    self.transform(&format!("{path}.portrait.lipSync.transform"), transform);
                }
            }
        }
        if let Some(subtitle) = &character.subtitle {
            if let Some(width) = subtitle.max_width
                && (!width.is_finite() || width <= 0.0)
            {
                self.error(
                    format!("{path}.subtitle.maxWidth"),
                    "must be finite and positive",
                );
            }
            if let Some(style) = &subtitle.style {
                self.text_style(&format!("{path}.subtitle.style"), style);
            }
            if let Some(transform) = &subtitle.transform {
                self.transform(&format!("{path}.subtitle.transform"), transform);
            }
        }
    }

    fn item(&mut self, path: &str, item: &TimelineItem) {
        self.time(&format!("{path}.range.start"), item.range.start, true);
        self.time(
            &format!("{path}.range.duration"),
            item.range.duration,
            false,
        );
        if let Some(transform) = &item.transform {
            self.transform(&format!("{path}.transform"), transform);
        }
        if let Some(opacity) = &item.opacity {
            self.animatable(
                &format!("{path}.opacity"),
                opacity,
                |value| value.is_finite() && (0.0..=1.0).contains(value),
                "must be finite and between 0 and 1",
            );
        }

        match &item.content {
            TimelineContent::Video {
                asset,
                source_range,
                playback_rate,
                volume,
                ..
            } => {
                self.asset_ref(&format!("{path}.content.asset"), asset, AssetKind::Video);
                self.media_fields(path, source_range, playback_rate, volume);
            }
            TimelineContent::Audio {
                asset,
                source_range,
                playback_rate,
                volume,
                ..
            } => {
                self.asset_ref(&format!("{path}.content.asset"), asset, AssetKind::Audio);
                self.media_fields(path, source_range, playback_rate, volume);
            }
            TimelineContent::Image { asset } => {
                self.asset_ref(&format!("{path}.content.asset"), asset, AssetKind::Image);
            }
            TimelineContent::Text { style, .. } => {
                if let Some(style) = style {
                    self.text_style(&format!("{path}.content.style"), style);
                }
            }
            TimelineContent::Dialogue {
                character,
                audio,
                volume,
                expression,
                lip_sync,
                ..
            } => {
                let Some(definition) = self.project.characters.get(character) else {
                    self.error(
                        format!("{path}.content.character"),
                        format!("references missing character `{character}`"),
                    );
                    return;
                };
                if let Some(audio) = audio {
                    self.asset_ref(&format!("{path}.content.audio"), audio, AssetKind::Audio);
                }
                if let Some(volume) = volume {
                    self.animatable(
                        &format!("{path}.content.volume"),
                        volume,
                        |value| value.is_finite() && *value >= 0.0,
                        "must be finite and non-negative",
                    );
                }
                if let Some(expression) = expression {
                    match &definition.portrait {
                        Some(portrait) if portrait.expressions.contains_key(expression) => {}
                        _ => self.error(
                            format!("{path}.content.expression"),
                            format!("character `{character}` has no expression `{expression}`"),
                        ),
                    }
                }
                if !lip_sync.is_empty() && audio.is_none() {
                    self.error(format!("{path}.content.lipSync"), "requires an audio asset");
                }
                if !lip_sync.is_empty()
                    && definition
                        .portrait
                        .as_ref()
                        .and_then(|portrait| portrait.lip_sync.as_ref())
                        .is_none()
                {
                    self.error(
                        format!("{path}.content.lipSync"),
                        format!("character `{character}` has no lip sync mouth assets"),
                    );
                }
                let mut previous = None;
                for (index, cue) in lip_sync.iter().enumerate() {
                    let cue_path = format!("{path}.content.lipSync[{index}].time");
                    self.time(&cue_path, cue.time, true);
                    if cue
                        .time
                        .cmp_exact(item.range.duration)
                        .is_ok_and(|ordering| ordering.is_gt())
                    {
                        self.error(cue_path.clone(), "must be inside the dialogue clip");
                    }
                    if let Some(previous) = previous
                        && cue
                            .time
                            .cmp_exact(previous)
                            .is_ok_and(|ordering| !ordering.is_gt())
                    {
                        self.error(cue_path, "must be in strictly ascending order");
                    }
                    previous = Some(cue.time);
                }
            }
            TimelineContent::Component { component, .. } => {
                if component.is_empty() {
                    self.error(format!("{path}.content.component"), "must not be empty");
                }
            }
        }
    }

    fn media_fields(
        &mut self,
        path: &str,
        source_range: &Option<SourceRange>,
        playback_rate: &Option<Animatable<f64>>,
        volume: &Option<Animatable<f64>>,
    ) {
        if let Some(range) = source_range {
            self.time(
                &format!("{path}.content.sourceRange.start"),
                range.start,
                true,
            );
            if let Some(duration) = range.duration {
                self.time(
                    &format!("{path}.content.sourceRange.duration"),
                    duration,
                    false,
                );
            }
        }
        if let Some(rate) = playback_rate {
            self.animatable(
                &format!("{path}.content.playbackRate"),
                rate,
                |value| value.is_finite() && *value > 0.0,
                "must be finite and positive",
            );
        }
        if let Some(volume) = volume {
            self.animatable(
                &format!("{path}.content.volume"),
                volume,
                |value| value.is_finite() && *value >= 0.0,
                "must be finite and non-negative",
            );
        }
    }

    fn asset_ref(&mut self, path: &str, id: &str, expected: AssetKind) {
        match self.project.assets.get(id) {
            None => self.error(path, format!("references missing asset `{id}`")),
            Some(asset) if asset.kind() != expected => self.error(
                path,
                format!("asset `{id}` is {}, expected {expected}", asset.kind()),
            ),
            Some(_) => {}
        }
    }

    fn time(&mut self, path: &str, time: Time, allow_zero: bool) {
        if !time.is_valid() {
            self.error(path, "timescale must be greater than zero");
        } else if time.value < 0 || (!allow_zero && time.value == 0) {
            self.error(
                path,
                if allow_zero {
                    "must be non-negative"
                } else {
                    "must be positive"
                },
            );
        }
    }

    fn transform(&mut self, path: &str, transform: &Transform) {
        for (name, point) in [
            ("position", &transform.position),
            ("scale", &transform.scale),
            ("anchor", &transform.anchor),
        ] {
            if let Some(point) = point {
                if let Some(x) = &point.x {
                    self.animatable(
                        &format!("{path}.{name}.x"),
                        x,
                        |v| v.is_finite(),
                        "must be finite",
                    );
                }
                if let Some(y) = &point.y {
                    self.animatable(
                        &format!("{path}.{name}.y"),
                        y,
                        |v| v.is_finite(),
                        "must be finite",
                    );
                }
            }
        }
        if let Some(rotation) = &transform.rotation {
            self.animatable(
                &format!("{path}.rotation"),
                rotation,
                |v| v.is_finite(),
                "must be finite",
            );
        }
    }

    fn animatable(
        &mut self,
        path: &str,
        value: &Animatable<f64>,
        valid: impl Fn(&f64) -> bool,
        message: &str,
    ) {
        match value {
            Animatable::Static(value) => {
                if !valid(value) {
                    self.error(path, message);
                }
            }
            Animatable::Keyframes(animation) => {
                if animation.keyframes.is_empty() {
                    self.error(format!("{path}.keyframes"), "must not be empty");
                }
                let mut previous = None;
                for (index, keyframe) in animation.keyframes.iter().enumerate() {
                    let keyframe_path = format!("{path}.keyframes[{index}]");
                    self.time(&format!("{keyframe_path}.time"), keyframe.time, true);
                    if let Some(previous) = previous
                        && keyframe
                            .time
                            .cmp_exact(previous)
                            .is_ok_and(|ordering| ordering.is_lt())
                    {
                        self.error(
                            format!("{keyframe_path}.time"),
                            "must be in ascending order",
                        );
                    }
                    if !valid(&keyframe.value) {
                        self.error(format!("{keyframe_path}.value"), message);
                    }
                    previous = Some(keyframe.time);
                }
            }
        }
    }

    fn text_style(&mut self, path: &str, style: &TextStyle) {
        if let Some(size) = style.font_size
            && (!size.is_finite() || size <= 0.0)
        {
            self.error(format!("{path}.fontSize"), "must be finite and positive");
        }
        if let Some(line_height) = style.line_height
            && (!line_height.is_finite() || line_height <= 0.0)
        {
            self.error(format!("{path}.lineHeight"), "must be finite and positive");
        }
        if let Some(fill) = &style.fill {
            self.paint(&format!("{path}.fill"), fill);
        }
        if let Some(stroke) = &style.stroke {
            if !stroke.width.is_finite() || stroke.width < 0.0 {
                self.error(
                    format!("{path}.stroke.width"),
                    "must be finite and non-negative",
                );
            }
            self.paint(&format!("{path}.stroke.paint"), &stroke.paint);
        }
    }

    fn paint(&mut self, path: &str, paint: &Paint) {
        let Paint::Solid { color } = paint;
        let hex = color.strip_prefix('#');
        if !hex.is_some_and(|hex| {
            matches!(hex.len(), 6 | 8) && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
        }) {
            self.error(format!("{path}.color"), "must use #RRGGBB or #RRGGBBAA");
        }
    }

    fn error(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.errors.push(ValidationError {
            path: path.into(),
            message: message.into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::{LoadError, Project};

    #[test]
    fn reports_semantic_reference_errors_with_paths() {
        let input = r#"{
          "version": 0,
          "settings": {
            "width": 1920,
            "height": 1080,
            "frameRate": { "numerator": 60, "denominator": 1 },
            "sampleRate": 48000
          },
          "assets": {},
          "characters": {},
          "tracks": [{
            "id": "video",
            "name": "Video",
            "kind": "video",
            "items": [{
              "id": "gameplay",
              "range": {
                "start": { "value": 0, "timescale": 1 },
                "duration": { "value": 10, "timescale": 1 }
              },
              "content": { "type": "video", "asset": "missing" }
            }]
          }],
          "properties": {}
        }"#;

        let LoadError::Validation(errors) = Project::from_json(input).unwrap_err() else {
            panic!("expected validation error");
        };
        assert_eq!(
            errors.as_slice()[0].path,
            "tracks[0].items[0].content.asset"
        );
        assert!(errors.as_slice()[0].message.contains("missing"));
    }
}
