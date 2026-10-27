//! A port of `skills/celesta/scripts/inspect.mjs`'s scene summary and loop
//! comparison, over the JSON a `Scene` serializes to.

use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value};

const MISSING: &str = "  ← MISSING FILE";

/// A number as `inspect.mjs` prints it: integers as is, others to at most
/// three decimals.
pub fn fmt(value: f64) -> String {
    if value.fract() == 0.0 && value.is_finite() {
        return format!("{}", value as i64);
    }
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

/// Local files a scene refers to that do not exist.
pub struct FileCheck {
    entry_dir: PathBuf,
    pub missing: BTreeSet<String>,
}

impl FileCheck {
    pub fn new(entry_dir: impl Into<PathBuf>) -> Self {
        Self {
            entry_dir: entry_dir.into(),
            missing: BTreeSet::new(),
        }
    }

    /// `""`, or a marker to append when `src` is a local file that is missing.
    pub fn check(&mut self, src: Option<&str>) -> &'static str {
        let Some(src) = src.filter(|src| !src.is_empty()) else {
            return "";
        };
        let lower = src.to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            return "";
        }
        let path = Path::new(src);
        let resolved = if path.is_absolute() {
            path.to_owned()
        } else {
            self.entry_dir.join(path)
        };
        if resolved.exists() {
            return "";
        }
        self.missing.insert(src.to_owned());
        MISSING
    }
}

fn num(value: &Value) -> f64 {
    value.as_f64().unwrap_or(0.0)
}

/// A number as JavaScript's `String()` prints it.
fn js(value: &Value) -> String {
    match value {
        Value::Number(number) => number
            .as_f64()
            .map_or_else(|| number.to_string(), |n| format!("{n}")),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Some(Value::String(text)) => !text.is_empty(),
        Some(_) => true,
    }
}

fn point(value: Option<&Value>, default: f64) -> (f64, f64) {
    let coordinate = |axis| {
        value
            .and_then(|v| v.get(axis))
            .and_then(Value::as_f64)
            .unwrap_or(default)
    };
    (coordinate("x"), coordinate("y"))
}

fn describe_transform(layer: &Value) -> String {
    let transform = layer.get("transform");
    let field = |name| transform.and_then(|t: &Value| t.get(name));
    let (x, y) = point(field("position"), 0.0);
    let (anchor_x, anchor_y) = point(field("anchor"), 0.0);
    let (scale_x, scale_y) = point(field("scale"), 1.0);
    let rotation = field("rotation").map_or(0.0, num);
    let mut parts = vec![format!("at ({}, {})", fmt(x), fmt(y))];
    if anchor_x != 0.0 || anchor_y != 0.0 {
        parts.push(format!("anchor ({}, {})", fmt(anchor_x), fmt(anchor_y)));
    }
    let (origin_x, origin_y) = point(field("origin"), 0.0);
    if origin_x != 0.0 || origin_y != 0.0 {
        parts.push(format!("origin ({}, {})", fmt(origin_x), fmt(origin_y)));
    }
    if scale_x != 1.0 || scale_y != 1.0 {
        parts.push(if scale_x == scale_y {
            format!("scale {}", fmt(scale_x))
        } else {
            format!("scale ({}, {})", fmt(scale_x), fmt(scale_y))
        });
    }
    if rotation != 0.0 {
        parts.push(format!("rotate {}°", fmt(rotation)));
    }
    let (skew_x, skew_y) = point(field("skew"), 0.0);
    if skew_x != 0.0 || skew_y != 0.0 {
        parts.push(format!("skew ({}°, {}°)", fmt(skew_x), fmt(skew_y)));
    }
    let rotation_x = field("rotationX").map_or(0.0, num);
    if rotation_x != 0.0 {
        parts.push(format!("rotate X {}°", fmt(rotation_x)));
    }
    let rotation_y = field("rotationY").map_or(0.0, num);
    if rotation_y != 0.0 {
        parts.push(format!("rotate Y {}°", fmt(rotation_y)));
    }
    if let Some(perspective) = field("perspective")
        .and_then(Value::as_f64)
        .filter(|perspective| perspective.is_finite() && *perspective > 0.0)
    {
        parts.push(format!("perspective {}", fmt(perspective)));
    }
    if field("backfaceVisibility").and_then(Value::as_str) == Some("hidden") {
        parts.push("backface hidden".to_owned());
    }
    let opacity = layer.get("opacity").map_or(1.0, num);
    if opacity != 1.0 {
        parts.push(format!("opacity {}", fmt(opacity)));
    }
    if let Some(mode) = layer
        .get("blendMode")
        .and_then(Value::as_str)
        .filter(|mode| *mode != "normal")
    {
        parts.push(format!("blend {mode}"));
    }
    let effects = layer.get("effects");
    let effect = |name| {
        effects
            .and_then(|e: &Value| e.get(name))
            .filter(|e| truthy(Some(e)))
    };
    if let Some(blur) = effect("blur") {
        parts.push(format!("blur {}", fmt(num(blur))));
    }
    if let Some(shadow) = effect("shadow") {
        parts.push(format!(
            "shadow {}/{} ({}, {})",
            js(&shadow["color"]),
            fmt(num(&shadow["blur"])),
            fmt(num(&shadow["offsetX"])),
            fmt(num(&shadow["offsetY"]))
        ));
    }
    if let Some(glow) = effect("glow") {
        parts.push(format!(
            "glow {}/{}",
            js(&glow["color"]),
            fmt(num(&glow["blur"]))
        ));
    }
    let filters = effects
        .and_then(|effects| effects.get("filters"))
        .and_then(Value::as_array)
        .filter(|filters| !filters.is_empty());
    if let Some(filters) = filters {
        let described: Vec<_> = filters.iter().map(describe_filter).collect();
        parts.push(format!("filters {}", described.join(" → ")));
    }
    parts.join(" ")
}

/// A custom shader by its id, or a built-in filter with its values, as
/// `inspect.mjs` prints them.
fn describe_filter(filter: &Value) -> String {
    let value = |name: &str| fmt(num(&filter[name]));
    match filter["type"].as_str().unwrap_or("shader") {
        "shader" => format!("shader {}", js(&filter["id"])),
        "hue" => format!("hue({}°)", value("degrees")),
        "tint" => format!("tint({}, {})", js(&filter["color"]), value("amount")),
        "vignette" => format!(
            "vignette({}, radius {}, softness {})",
            value("amount"),
            value("radius"),
            value("softness")
        ),
        "pixelate" => format!("pixelate({} px)", value("size")),
        "halftone" => format!("halftone({} px, {}°)", value("size"), value("angle")),
        "chromaticAberration" => format!(
            "chromaticAberration({} px, {}°)",
            value("offset"),
            value("angle")
        ),
        "wave" => format!(
            "wave({} px, wavelength {} px, phase {}°, {}°)",
            value("amplitude"),
            value("wavelength"),
            value("phase"),
            value("angle")
        ),
        "noise" => format!(
            "noise({}, seed {}{})",
            value("amount"),
            value("seed"),
            if filter["monochrome"] == Value::Bool(false) {
                ", color"
            } else {
                ""
            }
        ),
        "scanlines" => format!(
            "scanlines({}, spacing {} px, thickness {}, offset {} px)",
            value("amount"),
            value("spacing"),
            value("thickness"),
            value("offset")
        ),
        "zoomBlur" => format!(
            "zoomBlur({}, centre ({}, {}))",
            value("amount"),
            value("centerX"),
            value("centerY")
        ),
        "chromaKey" => format!(
            "chromaKey({}, tolerance {}, softness {}, spill {})",
            js(&filter["color"]),
            value("tolerance"),
            value("softness"),
            value("spill")
        ),
        kind => format!("{kind}({})", value("amount")),
    }
}

fn describe_paint(paint: Option<&Value>) -> String {
    let Some(paint) = paint.filter(|paint| truthy(Some(paint))) else {
        return String::new();
    };
    if paint.get("type").and_then(Value::as_str) == Some("solid") {
        return js(&paint["color"]);
    }
    let stops: Vec<String> = paint
        .get("stops")
        .and_then(Value::as_array)
        .map(|stops| stops.iter().map(|stop| js(&stop["color"])).collect())
        .unwrap_or_default();
    format!("{}({})", js(&paint["type"]), stops.join("→"))
}

fn fill_and_stroke(content: &Value) -> String {
    let mut text = String::new();
    if truthy(content.get("fill")) {
        let _ = write!(text, " fill {}", describe_paint(content.get("fill")));
    }
    if let Some(stroke) = content.get("stroke").filter(|stroke| truthy(Some(stroke))) {
        let _ = write!(
            text,
            " stroke {}/{}",
            describe_paint(stroke.get("paint")),
            fmt(num(&stroke["width"]))
        );
    }
    text
}

fn location_src(asset: &Value) -> Option<&str> {
    let location = asset.get("location")?;
    location
        .get("path")
        .or_else(|| location.get("url"))
        .and_then(Value::as_str)
}

fn location_path(asset: &Value) -> Option<&str> {
    asset.get("location")?.get("path").and_then(Value::as_str)
}

fn print_layers(
    layers: &[Value],
    depth: usize,
    files: &mut FileCheck,
    warnings: &mut Vec<String>,
    out: &mut String,
) {
    let indent = "  ".repeat(depth);
    for layer in layers {
        let content = &layer["content"];
        let kind = content.get("type").and_then(Value::as_str).unwrap_or("");
        let at = describe_transform(layer);
        match kind {
            "text" => {
                let empty = Value::Object(Map::new());
                let style = content.get("style").unwrap_or(&empty);
                let family = match style.get("fontFamily") {
                    Some(Value::Array(families)) => {
                        Some(families.iter().map(js).collect::<Vec<_>>().join(", "))
                    }
                    Some(value) if truthy(Some(value)) => Some(js(value)),
                    _ => None,
                };
                let mut font = vec![family.unwrap_or_else(|| "default font".to_owned())];
                if truthy(style.get("fontSize")) {
                    font.push(format!("{}px", js(&style["fontSize"])));
                }
                if truthy(style.get("fontWeight")) {
                    font.push(js(&style["fontWeight"]));
                }
                let color = if truthy(style.get("fill")) {
                    format!(" {}", describe_paint(style.get("fill")))
                } else {
                    String::new()
                };
                let mut extras = Vec::new();
                if truthy(content.get("maxWidth")) {
                    extras.push(format!("maxWidth {}", fmt(num(&content["maxWidth"]))));
                }
                if style.get("lineBreak").and_then(Value::as_str) == Some("phrase") {
                    extras.push("lineBreak phrase".to_owned());
                }
                if truthy(content.get("baselineAnchor")) {
                    extras.push("baseline anchor".to_owned());
                }
                let extras: String = extras.iter().map(|extra| format!(", {extra}")).collect();
                let _ = writeln!(
                    out,
                    "{indent}text {} {at} [{}{color}{extras}]",
                    content["text"],
                    font.join(" ")
                );
            }
            "rect" => {
                let radius = if truthy(content.get("cornerRadius")) {
                    format!(" radius {}", fmt(num(&content["cornerRadius"])))
                } else {
                    String::new()
                };
                let _ = writeln!(
                    out,
                    "{indent}rect {}×{} {at}{}{radius}",
                    fmt(num(&content["width"])),
                    fmt(num(&content["height"])),
                    fill_and_stroke(content)
                );
            }
            "path" => {
                let commands = content
                    .get("commands")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                let _ = writeln!(
                    out,
                    "{indent}path {commands} commands {at}{}",
                    fill_and_stroke(content)
                );
            }
            "image" | "video" | "psd" => {
                let asset = &content["asset"];
                let src = location_src(asset).unwrap_or("");
                let mut extra = String::new();
                if (kind == "image" || kind == "video")
                    && (truthy(content.get("width")) || truthy(content.get("height")))
                {
                    let side = |name| {
                        if truthy(content.get(name)) {
                            fmt(num(&content[name]))
                        } else {
                            "auto".to_owned()
                        }
                    };
                    let fit = content
                        .get("fit")
                        .filter(|fit| truthy(Some(fit)))
                        .map(|fit| format!(" {}", js(fit)))
                        .unwrap_or_default();
                    extra = format!(" size {}×{}{fit}", side("width"), side("height"));
                }
                if kind == "video" {
                    let _ = write!(
                        extra,
                        " (source {} s{})",
                        fmt(num(&content["timing"]["sourceTimeSeconds"])),
                        if truthy(content["timing"].get("loop")) {
                            ", looping"
                        } else {
                            ""
                        }
                    );
                }
                if kind == "psd" {
                    let visible = content
                        .get("visibleLayers")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len);
                    let shown = content
                        .get("enabledLayers")
                        .and_then(Value::as_array)
                        .filter(|layers| !layers.is_empty())
                        .map(|layers| {
                            format!(
                                ", shown {}",
                                layers.iter().map(js).collect::<Vec<_>>().join(", ")
                            )
                        })
                        .unwrap_or_default();
                    extra = format!(" ({visible} visible layers{shown})");
                    if visible == 0 {
                        warnings.push(format!(
                            "PSD {src} has no `layers`; it renders with its saved visibility"
                        ));
                    }
                }
                let marker = files.check(location_path(asset));
                let _ = writeln!(out, "{indent}{kind} {src} {at}{extra}{marker}");
            }
            "group" => {
                let clip = content
                    .get("clip")
                    .filter(|clip| truthy(Some(clip)))
                    .map(|clip| {
                        let radius = if truthy(clip.get("cornerRadius")) {
                            format!(" radius {}", fmt(num(&clip["cornerRadius"])))
                        } else {
                            String::new()
                        };
                        format!(
                            " clip {}×{} at ({}, {}){radius}",
                            fmt(num(&clip["width"])),
                            fmt(num(&clip["height"])),
                            fmt(num(&clip["x"])),
                            fmt(num(&clip["y"]))
                        )
                    });
                let _ = writeln!(out, "{indent}group {at}{}", clip.unwrap_or_default());
                let children = content
                    .get("layers")
                    .and_then(Value::as_array)
                    .map_or(&[][..], Vec::as_slice);
                print_layers(children, depth + 1, files, warnings, out);
            }
            "missingComponent" => {
                let component = js(&content["component"]);
                warnings.push(format!(
                    "component \"{component}\" is not registered; export will fail"
                ));
                let _ = writeln!(out, "{indent}UNREGISTERED COMPONENT {component} {at}");
            }
            _ => {
                let _ = writeln!(out, "{indent}{kind} {at}");
            }
        }
    }
}

/// The lines `inspect.mjs` prints for one frame: fonts, layers, and audio.
pub fn describe_frame(
    scene: &Value,
    audio: &[Value],
    files: &mut FileCheck,
    warnings: &mut Vec<String>,
) -> String {
    let mut out = String::new();
    for font in scene
        .get("fonts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let src = location_src(font).unwrap_or("");
        let marker = files.check(location_path(font));
        let _ = writeln!(out, "font {}: {src}{marker}", js(&font["id"]));
    }
    let layers = scene
        .get("layers")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    if layers.is_empty() {
        out.push_str("(no layers: the frame is black)\n");
    }
    print_layers(layers, 0, files, warnings, &mut out);
    for clip in audio {
        let src = clip.get("src").and_then(Value::as_str).unwrap_or("");
        let mut line = format!(
            "audio {src} from {} s for {} s",
            fmt(num(&clip["start"])),
            fmt(num(&clip["duration"]))
        );
        if truthy(clip.get("sourceStart")) {
            let _ = write!(
                line,
                ", starting {} s into the file",
                fmt(num(&clip["sourceStart"]))
            );
        }
        match clip.get("volume") {
            Some(Value::Number(volume)) if volume.as_f64() != Some(1.0) => {
                let _ = write!(line, ", volume {}", fmt(volume.as_f64().unwrap_or(0.0)));
            }
            Some(Value::Object(_) | Value::Array(_)) => line.push_str(", volume keyframes"),
            _ => {}
        }
        if truthy(clip.get("fadeIn")) {
            let _ = write!(line, ", fading in over {} s", fmt(num(&clip["fadeIn"])));
        }
        if truthy(clip.get("fadeOut")) {
            let _ = write!(line, ", fading out over {} s", fmt(num(&clip["fadeOut"])));
        }
        if let Some(group) = clip.get("duckGroup").and_then(Value::as_str) {
            let _ = write!(line, ", in duck group \"{group}\"");
        }
        if let Some(duck) = clip.get("duck").filter(|duck| duck.is_object()) {
            let under = match &duck["under"] {
                Value::String(group) => group.clone(),
                Value::Array(groups) => groups
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", "),
                _ => String::new(),
            };
            let _ = write!(line, ", ducking under {under}");
            match duck.get("to") {
                Some(Value::Number(gain)) => {
                    let _ = write!(line, " to {}", fmt(gain.as_f64().unwrap_or(0.0)));
                }
                Some(Value::Object(level)) => {
                    if let Some(db) = level.get("db").and_then(Value::as_f64) {
                        let _ = write!(line, " to {} dB", fmt(db));
                    }
                }
                _ => {}
            }
        }
        if truthy(clip.get("loop")) {
            line.push_str(", looping");
        }
        if truthy(clip.get("muted")) {
            line.push_str(", muted");
        }
        line.push_str(files.check(Some(src)));
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// Where two scenes differ; `None` is a value absent on that side.
#[derive(Clone, Debug, Serialize)]
pub struct Difference {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<Value>,
}

/// Where scene `end` (a frame past the end) differs from `start`. Numbers
/// may differ by rounding and hex colors by one step per channel; layer ids
/// and the scenes' `time` are not compared.
pub fn diff_scenes(end: &Value, start: &Value) -> Vec<Difference> {
    let without_time = |scene: &Value| match scene {
        Value::Object(map) => {
            let mut map = map.clone();
            map.remove("time");
            Value::Object(map)
        }
        other => other.clone(),
    };
    let mut out = Vec::new();
    diff(
        Some(&without_time(end)),
        Some(&without_time(start)),
        "",
        &mut out,
    );
    out
}

fn hex_color(text: &str) -> bool {
    let digits = text.strip_prefix('#').unwrap_or("");
    text.starts_with('#')
        && (digits.len() == 6 || digits.len() == 8)
        && digits.bytes().all(|b| b.is_ascii_hexdigit())
}

fn diff(end: Option<&Value>, start: Option<&Value>, at: &str, out: &mut Vec<Difference>) {
    let push = |out: &mut Vec<Difference>, end: Option<Value>, start: Option<Value>| {
        out.push(Difference {
            path: at.to_owned(),
            end,
            start,
        });
    };
    match (end, start) {
        (Some(Value::Number(e)), Some(Value::Number(s))) => {
            let (e, s) = (e.as_f64().unwrap_or(0.0), s.as_f64().unwrap_or(0.0));
            if (e - s).abs() > 1e-6 * 1f64.max(e.abs()).max(s.abs()) {
                push(out, end.cloned(), start.cloned());
            }
        }
        (Some(Value::String(e)), Some(Value::String(s)))
            if hex_color(e) && hex_color(s) && e.len() == s.len() =>
        {
            let channel =
                |text: &str, i: usize| i64::from_str_radix(&text[i..i + 2], 16).unwrap_or(0);
            if (1..e.len())
                .step_by(2)
                .any(|i| (channel(e, i) - channel(s, i)).abs() > 1)
            {
                push(out, end.cloned(), start.cloned());
            }
        }
        (Some(Value::Array(e)), Some(Value::Array(s))) => {
            if e.len() != s.len() {
                push(
                    out,
                    Some(Value::String(describe_list(e))),
                    Some(Value::String(describe_list(s))),
                );
                return;
            }
            for (i, (e_item, s_item)) in e.iter().zip(s).enumerate() {
                let id = e_item
                    .get("id")
                    .and_then(Value::as_str)
                    .map(|id| format!("({id})"))
                    .unwrap_or_default();
                diff(Some(e_item), Some(s_item), &format!("{at}[{i}]{id}"), out);
            }
        }
        (Some(Value::Object(e)), Some(Value::Object(s))) => {
            let keys: BTreeSet<&String> = e.keys().chain(s.keys()).collect();
            for key in keys {
                if key == "id" {
                    continue;
                }
                let path = if at.is_empty() {
                    key.clone()
                } else {
                    format!("{at}.{key}")
                };
                diff(e.get(key), s.get(key), &path, out);
            }
        }
        _ => {
            if end != start {
                push(out, end.cloned(), start.cloned());
            }
        }
    }
}

fn describe_list(list: &[Value]) -> String {
    if list.is_empty() {
        return "no layers".to_owned();
    }
    let types: Option<Vec<&str>> = list
        .iter()
        .map(|item| item.get("content")?.get("type")?.as_str())
        .collect();
    let Some(types) = types else {
        return format!("{} items", list.len());
    };
    let more = if list.len() > 6 { ", …" } else { "" };
    format!(
        "{} layers: {}{more}",
        list.len(),
        types[..types.len().min(6)].join(", ")
    )
}

/// A value as `inspect.mjs` prints a loop difference.
pub fn format_value(value: Option<&Value>) -> String {
    match value {
        None => "(absent)".to_owned(),
        Some(Value::Number(number)) => {
            let n = number.as_f64().unwrap_or(0.0);
            if n.fract() == 0.0 {
                format!("{}", n as i64)
            } else {
                format!("{}", (n * 1e6).round() / 1e6)
            }
        }
        Some(other) => {
            let text = other.to_string();
            if text.chars().count() > 60 {
                format!("{}…", text.chars().take(57).collect::<String>())
            } else {
                text
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn layer(id: &str, content: Value) -> Value {
        json!({
            "id": id,
            "transform": {
                "position": { "x": 10, "y": 20 }, "scale": { "x": 1, "y": 1 },
                "rotation": 0, "anchor": { "x": 0, "y": 0 }
            },
            "opacity": 1,
            "content": content,
        })
    }

    fn describe(layers: Vec<Value>, audio: Vec<Value>) -> (String, FileCheck, Vec<String>) {
        let mut files = FileCheck::new(std::env::temp_dir());
        let mut warnings = Vec::new();
        let scene = json!({ "time": { "value": 0, "timescale": 30 }, "layers": layers });
        let text = describe_frame(&scene, &audio, &mut files, &mut warnings);
        (text, files, warnings)
    }

    #[test]
    fn fmt_matches_inspect_mjs() {
        assert_eq!(fmt(2.0), "2");
        assert_eq!(fmt(1.5), "1.5");
        assert_eq!(fmt(1.0 / 3.0), "0.333");
        assert_eq!(fmt(-0.25), "-0.25");
    }

    #[test]
    fn describes_rects_text_and_groups() {
        let rect = layer(
            "r",
            json!({ "type": "rect", "width": 100, "height": 50,
                    "fill": { "type": "solid", "color": "#ff0000" } }),
        );
        let text = layer(
            "t",
            json!({ "type": "text", "text": "Hi",
                    "style": { "fontFamily": "sans-serif", "fontSize": 48,
                               "fill": { "type": "solid", "color": "#ffffff" } } }),
        );
        let group = layer("g", json!({ "type": "group", "layers": [rect.clone()] }));
        let (output, _, _) = describe(vec![rect, text, group], vec![]);
        assert_eq!(
            output,
            "rect 100×50 at (10, 20) fill #ff0000\n\
             text \"Hi\" at (10, 20) [sans-serif 48px #ffffff]\n\
             group at (10, 20)\n  rect 100×50 at (10, 20) fill #ff0000\n"
        );
    }

    #[test]
    fn describes_transforms_and_effects() {
        let mut moved = layer("m", json!({ "type": "rect", "width": 1, "height": 1 }));
        moved["transform"]["anchor"] = json!({ "x": 0.5, "y": 0.5 });
        moved["transform"]["scale"] = json!({ "x": 2, "y": 2 });
        moved["transform"]["rotation"] = json!(45);
        moved["opacity"] = json!(0.5);
        moved["effects"] = json!({ "blur": 4, "filters": [
            { "type": "shader", "id": "1b0a62b2133ed451" },
            { "type": "grayscale", "amount": 1 },
            { "type": "hue", "degrees": 30 },
            { "type": "tint", "color": "#704214", "amount": 0.35 },
            { "type": "vignette", "amount": 0.6, "radius": 0.5, "softness": 0.25 },
        ] });
        let (output, _, _) = describe(vec![moved], vec![]);
        assert_eq!(
            output,
            "rect 1×1 at (10, 20) anchor (0.5, 0.5) scale 2 rotate 45° opacity 0.5 blur 4 \
             filters shader 1b0a62b2133ed451 → grayscale(1) → hue(30°) → tint(#704214, 0.35) → \
             vignette(0.6, radius 0.5, softness 0.25)\n"
        );
    }

    #[test]
    fn omits_a_perspective_that_is_not_positive() {
        for perspective in [0, -5] {
            let mut flat = layer("p", json!({ "type": "rect", "width": 1, "height": 1 }));
            flat["transform"]["perspective"] = json!(perspective);
            let (output, _, _) = describe(vec![flat], vec![]);
            assert_eq!(
                output, "rect 1×1 at (10, 20)\n",
                "perspective {perspective}"
            );
        }
    }

    #[test]
    fn describes_skew_and_3d_transforms() {
        let mut tilted = layer("s", json!({ "type": "rect", "width": 1, "height": 1 }));
        tilted["transform"]["rotation"] = json!(45);
        tilted["transform"]["origin"] = json!({ "x": 5, "y": 0 });
        tilted["transform"]["skew"] = json!({ "x": 10, "y": 0 });
        tilted["transform"]["rotationX"] = json!(15);
        tilted["transform"]["rotationY"] = json!(30);
        tilted["transform"]["perspective"] = json!(800);
        tilted["transform"]["backfaceVisibility"] = json!("hidden");
        let (output, _, _) = describe(vec![tilted], vec![]);
        assert_eq!(
            output,
            "rect 1×1 at (10, 20) origin (5, 0) rotate 45° skew (10°, 0°) rotate X 15° rotate Y 30° \
             perspective 800 backface hidden\n"
        );

        let flat = layer("f", json!({ "type": "rect", "width": 1, "height": 1 }));
        let (output, _, _) = describe(vec![flat], vec![]);
        assert_eq!(output, "rect 1×1 at (10, 20)\n");
    }

    #[test]
    fn describes_stylize_effects_and_chroma_keys() {
        let mut keyed = layer("k", json!({ "type": "rect", "width": 1, "height": 1 }));
        keyed["effects"] = json!({ "filters": [
            { "type": "chromaKey", "color": "#00b140", "tolerance": 0.2, "softness": 0.1, "spill": 0.5 },
            { "type": "pixelate", "size": 8 },
            { "type": "wave", "amplitude": 6, "wavelength": 80, "phase": 90, "angle": 0 },
            { "type": "zoomBlur", "amount": 0.25, "centerX": 0.5, "centerY": 0.25 },
            { "type": "chromaticAberration", "offset": 4, "angle": 30 },
            { "type": "halftone", "size": 10, "angle": 45 },
            { "type": "noise", "amount": 0.2, "seed": 42, "monochrome": false },
            { "type": "scanlines", "amount": 0.5, "spacing": 4, "thickness": 0.5, "offset": 1.5 },
        ] });
        let (output, _, _) = describe(vec![keyed], vec![]);
        assert_eq!(
            output,
            "rect 1×1 at (10, 20) filters chromaKey(#00b140, tolerance 0.2, softness 0.1, spill 0.5) → \
             pixelate(8 px) → wave(6 px, wavelength 80 px, phase 90°, 0°) → zoomBlur(0.25, centre (0.5, 0.25)) → \
             chromaticAberration(4 px, 30°) → halftone(10 px, 45°) → noise(0.2, seed 42, color) → \
             scanlines(0.5, spacing 4 px, thickness 0.5, offset 1.5 px)\n"
        );
    }

    #[test]
    fn describes_the_size_of_images_and_videos() {
        let asset = |url: &str| json!({ "id": url, "location": { "type": "url", "url": url } });
        let timing = json!({ "sourceTimeSeconds": 1.5, "loop": true });
        let sized_video = layer(
            "v",
            json!({ "type": "video", "asset": asset("https://media.test/a.mp4"), "timing": timing, "width": 224 }),
        );
        let video = layer(
            "w",
            json!({ "type": "video", "asset": asset("https://media.test/b.mp4"), "timing": timing }),
        );
        let image = layer(
            "i",
            json!({ "type": "image", "asset": asset("https://media.test/c.png"), "height": 90, "fit": "cover" }),
        );
        let (output, _, _) = describe(vec![sized_video, video, image], vec![]);
        assert_eq!(
            output,
            "video https://media.test/a.mp4 at (10, 20) size 224×auto (source 1.5 s, looping)\n\
             video https://media.test/b.mp4 at (10, 20) (source 1.5 s, looping)\n\
             image https://media.test/c.png at (10, 20) size auto×90 cover\n"
        );
    }

    #[test]
    fn reports_missing_files_and_unregistered_components() {
        let image = layer(
            "i",
            json!({ "type": "image", "asset": { "id": "a", "location": { "type": "file", "path": "./nope.png" } } }),
        );
        let missing = layer(
            "c",
            json!({ "type": "missingComponent", "component": "Foo" }),
        );
        let audio = json!({ "src": "./nope.wav", "start": 0, "duration": 2.5, "volume": 0.5 });
        let (output, files, warnings) = describe(vec![image, missing], vec![audio]);
        assert_eq!(
            output,
            "image ./nope.png at (10, 20)  ← MISSING FILE\n\
             UNREGISTERED COMPONENT Foo at (10, 20)\n\
             audio ./nope.wav from 0 s for 2.5 s, volume 0.5  ← MISSING FILE\n"
        );
        assert_eq!(
            files.missing.iter().cloned().collect::<Vec<_>>(),
            ["./nope.png", "./nope.wav"]
        );
        assert_eq!(
            warnings,
            ["component \"Foo\" is not registered; export will fail"]
        );
    }

    #[test]
    fn names_audio_fades() {
        let audio = json!({ "src": "./bgm.wav", "start": 0, "duration": 10, "volume": 0.6, "fadeIn": 1.5, "fadeOut": 3, "loop": true });
        let (output, _, _) = describe(vec![], vec![audio]);
        assert!(
            output.ends_with(
                "audio ./bgm.wav from 0 s for 10 s, volume 0.6, fading in over 1.5 s, \
                 fading out over 3 s, looping  ← MISSING FILE\n"
            ),
            "{output}"
        );
    }

    #[test]
    fn names_audio_ducking() {
        let voice = json!({ "src": "./v1.wav", "start": 1, "duration": 2, "duckGroup": "voice" });
        let bed = json!({
            "src": "./bgm.wav", "start": 0, "duration": 10,
            "duck": { "under": ["voice", "sfx"], "to": { "db": -12 } },
        });
        let game = json!({ "src": "./game.wav", "start": 0, "duration": 10, "duck": { "under": "voice", "to": 0.5 } });
        let plain =
            json!({ "src": "./sfx.wav", "start": 0, "duration": 1, "duck": { "under": "voice" } });
        let (output, _, _) = describe(vec![], vec![voice, bed, game, plain]);
        for expected in [
            "audio ./v1.wav from 1 s for 2 s, in duck group \"voice\"",
            "audio ./bgm.wav from 0 s for 10 s, ducking under voice, sfx to -12 dB",
            "audio ./game.wav from 0 s for 10 s, ducking under voice to 0.5",
            "audio ./sfx.wav from 0 s for 1 s, ducking under voice  ←",
        ] {
            assert!(output.contains(expected), "{expected}\n{output}");
        }
    }

    #[test]
    fn an_empty_frame_is_black() {
        let (output, _, _) = describe(vec![], vec![]);
        assert_eq!(output, "(no layers: the frame is black)\n");
    }

    #[test]
    fn diff_ignores_ids_time_and_rounding() {
        let end = json!({ "time": { "value": 30 }, "layers": [
            { "id": "a", "opacity": 0.1, "fill": "#ff0000" } ] });
        let start = json!({ "time": { "value": 0 }, "layers": [
            { "id": "b", "opacity": 0.1000000001, "fill": "#fe0000" } ] });
        assert!(diff_scenes(&end, &start).is_empty());
    }

    #[test]
    fn diff_reports_changed_values_with_paths() {
        let end = json!({ "layers": [ { "id": "a", "opacity": 1, "fill": "#ff0000" } ] });
        let start = json!({ "layers": [ { "id": "a", "opacity": 0.5, "fill": "#fd0000" } ] });
        let differences = diff_scenes(&end, &start);
        let paths: Vec<&str> = differences.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(paths, ["layers[0](a).fill", "layers[0](a).opacity"]);
        assert_eq!(differences[1].end, Some(json!(1)));
        assert_eq!(differences[1].start, Some(json!(0.5)));
    }

    #[test]
    fn diff_reports_layer_lists_of_different_lengths_once() {
        let rect = json!({ "id": "r", "content": { "type": "rect" } });
        let end = json!({ "layers": [rect.clone(), rect.clone()] });
        let start = json!({ "layers": [rect] });
        let differences = diff_scenes(&end, &start);
        assert_eq!(differences.len(), 1);
        assert_eq!(differences[0].end, Some(json!("2 layers: rect, rect")));
        assert_eq!(differences[0].start, Some(json!("1 layers: rect")));
    }

    #[test]
    fn format_value_shortens_long_json() {
        assert_eq!(format_value(None), "(absent)");
        assert_eq!(format_value(Some(&json!(0.1234567))), "0.123457");
        assert_eq!(
            format_value(Some(&json!("x".repeat(70)))).chars().count(),
            58
        );
    }
}
