use std::collections::HashSet;
use std::error::Error;
use std::fmt;

use celesta_composition::{
    Animatable, Color, DuckError, DuckLevel, GradientStop, LetterSpacing, Paint, TextStroke,
    TextStyle, Time, Transform, check_duck_group, duck_duration,
};

use crate::{
    AssetKind, Character, Project, ProjectDuck, SourceRange, TimelineContent, TimelineEffects,
    TimelineItem,
};

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

        let project = self.project;
        for (id, character) in &project.characters {
            self.character(id, character);
        }

        let mut track_ids = HashSet::new();
        let mut item_ids = HashSet::new();
        for (track_index, track) in project.tracks.iter().enumerate() {
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
        if let Some(effects) = &item.effects {
            let radius = |value: &f64| value.is_finite() && (0.0..=64.0).contains(value);
            if let Some(blur) = &effects.blur {
                self.animatable(
                    &format!("{path}.effects.blur"),
                    blur,
                    radius,
                    "must be finite and between 0 and 64",
                );
            }
            if let Some(shadow) = &effects.shadow {
                self.animatable(
                    &format!("{path}.effects.shadow.color"),
                    &shadow.color,
                    |color| Color::from_hex(color).is_some(),
                    "must be a #RRGGBB or #RRGGBBAA color",
                );
                self.animatable(
                    &format!("{path}.effects.shadow.blur"),
                    &shadow.blur,
                    radius,
                    "must be finite and between 0 and 64",
                );
                self.animatable(
                    &format!("{path}.effects.shadow.offsetX"),
                    &shadow.offset_x,
                    |value| value.is_finite(),
                    "must be finite",
                );
                self.animatable(
                    &format!("{path}.effects.shadow.offsetY"),
                    &shadow.offset_y,
                    |value| value.is_finite(),
                    "must be finite",
                );
            }
            if let Some(glow) = &effects.glow {
                self.animatable(
                    &format!("{path}.effects.glow.color"),
                    &glow.color,
                    |color| Color::from_hex(color).is_some(),
                    "must be a #RRGGBB or #RRGGBBAA color",
                );
                self.animatable(
                    &format!("{path}.effects.glow.blur"),
                    &glow.blur,
                    radius,
                    "must be finite and between 0 and 64",
                );
            }
            self.color_filters(&format!("{path}.effects"), effects);
        }

        match &item.content {
            TimelineContent::Video {
                asset,
                source_range,
                playback_rate,
                volume,
                duck_group,
                duck,
                ..
            } => {
                self.asset_ref(&format!("{path}.content.asset"), asset, AssetKind::Video);
                self.media_fields(path, source_range, playback_rate, volume);
                self.ducking(path, duck_group.as_deref(), duck.as_ref());
            }
            TimelineContent::Audio {
                asset,
                source_range,
                playback_rate,
                volume,
                pitch,
                fade_in,
                fade_out,
                duck_group,
                duck,
                ..
            } => {
                self.asset_ref(&format!("{path}.content.asset"), asset, AssetKind::Audio);
                self.media_fields(path, source_range, playback_rate, volume);
                self.ducking(path, duck_group.as_deref(), duck.as_ref());
                if let Some(pitch) = pitch {
                    self.animatable(
                        &format!("{path}.content.pitch"),
                        pitch,
                        |v| v.is_finite() && (-24.0..=24.0).contains(v),
                        "must be finite and between -24 and 24",
                    );
                }
                for (name, fade) in [("fadeIn", fade_in), ("fadeOut", fade_out)] {
                    if let Some(fade) = fade {
                        self.time(&format!("{path}.content.{name}"), *fade, true);
                    }
                }
            }
            TimelineContent::Image {
                asset,
                width,
                height,
                ..
            } => {
                for (name, value) in [("width", width), ("height", height)] {
                    if let Some(value) = value
                        && (!value.is_finite() || *value <= 0.0)
                    {
                        self.error(
                            format!("{path}.content.{name}"),
                            "must be finite and positive",
                        );
                    }
                }
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
                duck_group,
                ..
            } => {
                if let Some(Some(group)) = duck_group {
                    self.ducking(path, Some(group), None);
                }
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

    /// Checks an item's `duckGroup` and `duck` against the rules
    /// `AudioDuck::resolve` applies, one error per field.
    fn ducking(&mut self, path: &str, duck_group: Option<&str>, duck: Option<&ProjectDuck>) {
        if let Some(Err(error)) = duck_group.map(check_duck_group) {
            self.error(format!("{path}.content.duckGroup"), error.to_string());
        }
        let Some(duck) = duck else {
            return;
        };
        let checks = [
            ("under", duck.under.names().err()),
            ("to", duck.to.map(DuckLevel::gain).and_then(Result::err)),
            (
                "attack",
                duck_duration(duck.attack, Time::ZERO, DuckError::Attack).err(),
            ),
            (
                "release",
                duck_duration(duck.release, Time::ZERO, DuckError::Release).err(),
            ),
            (
                "holdGap",
                duck_duration(duck.hold_gap, Time::ZERO, DuckError::HoldGap).err(),
            ),
        ];
        for (field, error) in checks {
            if let Some(error) = error {
                self.error(format!("{path}.content.duck.{field}"), error.to_string());
            }
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
            ("origin", &transform.origin),
            ("skew", &transform.skew),
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
        for (name, angle) in [
            ("rotation", &transform.rotation),
            ("rotationX", &transform.rotation_x),
            ("rotationY", &transform.rotation_y),
        ] {
            if let Some(angle) = angle {
                self.animatable(
                    &format!("{path}.{name}"),
                    angle,
                    |v| v.is_finite(),
                    "must be finite",
                );
            }
        }
        if let Some(perspective) = &transform.perspective {
            self.animatable(
                &format!("{path}.perspective"),
                perspective,
                |v| v.is_finite() && *v > 0.0,
                "must be finite and positive",
            );
        }
    }

    /// The color filters of `effects` at `path`, with the ranges the React
    /// props of the same names take.
    fn color_filters(&mut self, path: &str, effects: &TimelineEffects) {
        let at_least_zero = |value: &f64| value.is_finite() && *value >= 0.0;
        let unit = |value: &f64| value.is_finite() && (0.0..=1.0).contains(value);
        const AT_LEAST_ZERO: &str = "must be finite and at least 0";
        const UNIT: &str = "must be finite and between 0 and 1";
        for (name, value) in [
            ("brightness", &effects.brightness),
            ("contrast", &effects.contrast),
            ("saturation", &effects.saturation),
        ] {
            if let Some(value) = value {
                self.animatable(
                    &format!("{path}.{name}"),
                    value,
                    at_least_zero,
                    AT_LEAST_ZERO,
                );
            }
        }
        if let Some(hue) = &effects.hue {
            self.animatable(
                &format!("{path}.hue"),
                hue,
                |value| value.is_finite(),
                "must be finite",
            );
        }
        for (name, value) in [
            ("grayscale", &effects.grayscale),
            ("invert", &effects.invert),
        ] {
            if let Some(value) = value {
                self.animatable(&format!("{path}.{name}"), value, unit, UNIT);
            }
        }
        if let Some(tint) = &effects.tint {
            self.animatable(
                &format!("{path}.tint.color"),
                &tint.color,
                |color| Color::from_hex(color).is_some(),
                "must be a #RRGGBB or #RRGGBBAA color",
            );
            if let Some(amount) = &tint.amount {
                self.animatable(&format!("{path}.tint.amount"), amount, unit, UNIT);
            }
        }
        if let Some(vignette) = &effects.vignette {
            self.animatable(
                &format!("{path}.vignette.amount"),
                &vignette.amount,
                unit,
                UNIT,
            );
            for (name, value) in [
                ("radius", &vignette.radius),
                ("softness", &vignette.softness),
            ] {
                if let Some(value) = value {
                    self.animatable(
                        &format!("{path}.vignette.{name}"),
                        value,
                        at_least_zero,
                        AT_LEAST_ZERO,
                    );
                }
            }
        }
    }

    fn animatable<T>(
        &mut self,
        path: &str,
        value: &Animatable<T>,
        valid: impl Fn(&T) -> bool,
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
        if let Some(letter_spacing) = style.letter_spacing
            && !letter_spacing.is_finite()
        {
            let field = match letter_spacing {
                LetterSpacing::Px(_) => "letterSpacing",
                LetterSpacing::Em { .. } => "letterSpacing.em",
            };
            self.error(format!("{path}.{field}"), "must be finite");
        }
        if let Some(fill) = &style.fill {
            self.paint(&format!("{path}.fill"), fill);
        }
        if let Some(stroke) = &style.stroke {
            for (index, stroke) in stroke.strokes().iter().enumerate() {
                let path = match style.stroke {
                    Some(TextStroke::Many(_)) => format!("{path}.stroke[{index}]"),
                    _ => format!("{path}.stroke"),
                };
                if !stroke.width.is_finite() || stroke.width < 0.0 {
                    self.error(format!("{path}.width"), "must be finite and non-negative");
                }
                self.paint(&format!("{path}.paint"), &stroke.paint);
            }
        }
    }

    fn paint(&mut self, path: &str, paint: &Paint) {
        match paint {
            Paint::Solid { color } => self.color(&format!("{path}.color"), color),
            Paint::Linear { start, end, stops } => {
                for (name, point) in [("start", start), ("end", end)] {
                    if !point.x.is_finite() || !point.y.is_finite() {
                        self.error(format!("{path}.{name}"), "must be finite");
                    }
                }
                if start == end {
                    self.error(path, "start and end must differ");
                }
                self.gradient_stops(path, stops);
            }
            Paint::Radial {
                center,
                radius,
                stops,
            } => {
                if !center.x.is_finite() || !center.y.is_finite() {
                    self.error(format!("{path}.center"), "must be finite");
                }
                if !radius.is_finite() || *radius <= 0.0 {
                    self.error(format!("{path}.radius"), "must be finite and positive");
                }
                self.gradient_stops(path, stops);
            }
        }
    }

    fn gradient_stops(&mut self, path: &str, stops: &[GradientStop]) {
        if stops.len() < 2 {
            self.error(format!("{path}.stops"), "needs at least 2 stops");
        }
        for (index, stop) in stops.iter().enumerate() {
            if !stop.offset.is_finite() || !(0.0..=1.0).contains(&stop.offset) {
                self.error(
                    format!("{path}.stops[{index}].offset"),
                    "must be between 0 and 1",
                );
            }
            self.color(&format!("{path}.stops[{index}].color"), &stop.color);
        }
    }

    fn color(&mut self, path: &str, color: &str) {
        if Color::from_hex(color).is_none() {
            self.error(path, "must use #RRGGBB or #RRGGBBAA");
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

    #[test]
    fn reports_character_and_track_errors_in_document_order() {
        let input = r#"{
          "version": 0,
          "settings": {
            "width": 0,
            "height": 1080,
            "frameRate": { "numerator": 60, "denominator": 1 },
            "sampleRate": 48000
          },
          "assets": {},
          "characters": {
            "zeta": { "name": "Zeta", "subtitle": { "maxWidth": 0 } },
            "alpha": { "name": "Alpha", "subtitle": { "maxWidth": -1 } }
          },
          "tracks": [
            {
              "id": "overlay",
              "name": "Overlay",
              "kind": "overlay",
              "items": [{
                "id": "",
                "range": {
                  "start": { "value": 0, "timescale": 1 },
                  "duration": { "value": 1, "timescale": 1 }
                },
                "content": { "type": "text", "text": "Celesta", "style": { "fontSize": 64 } }
              }]
            },
            { "id": "overlay", "name": "Overlay", "kind": "overlay", "items": [] }
          ],
          "properties": {}
        }"#;

        let LoadError::Validation(errors) = Project::from_json(input).unwrap_err() else {
            panic!("expected validation error");
        };
        let errors: Vec<_> = errors
            .as_slice()
            .iter()
            .map(|error| (error.path.as_str(), error.message.as_str()))
            .collect();
        assert_eq!(
            errors,
            [
                ("settings.width", "must be greater than zero"),
                (
                    "characters.alpha.subtitle.maxWidth",
                    "must be finite and positive"
                ),
                (
                    "characters.zeta.subtitle.maxWidth",
                    "must be finite and positive"
                ),
                ("tracks[0].items[0].id", "must not be empty"),
                ("tracks[1].id", "must be unique"),
            ]
        );
    }
}
