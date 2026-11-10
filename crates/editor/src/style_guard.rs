//! Keeps text sizes on the editor's type roles.
//!
//! At the editor's 13px `rem`, `text_xs` is about 9.75px, too small to read,
//! and a pixel `text_size` steps outside the scale altogether, except on a
//! painted scale whose labels share its pixel geometry (marked `physical:`
//! on the line). Body text, values and titles are `text_base`; labels and
//! secondary text `text_sm`.

use std::path::Path;

const FORBIDDEN: [&str; 2] = ["text_xs(", "text_size(px("];

/// The editor's sources, recursively.
fn sources(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs")
            && path
                .file_name()
                .is_some_and(|name| name != "style_guard.rs")
        {
            files.push(path);
        }
    }
}

fn editor_sources() -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    sources(
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")),
        &mut files,
    );
    // A guard that scans nothing passes vacuously.
    assert!(files.len() > 50, "only {} sources found", files.len());
    files
}

/// `Button` chains that are `xsmall` and set a `.label(`: gpui-kit sizes an
/// xsmall button's label `text_xs` (9.75px at the 13px base), below the
/// `text_sm` floor. `CompactLabel::compact_label` keeps the height and the
/// floor. A chain runs from `Button::new(` to the next one, at most 25 lines.
#[test]
fn xsmall_buttons_keep_their_labels_at_the_floor() {
    let mut found = Vec::new();
    for path in editor_sources() {
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        for (start, line) in lines.iter().enumerate() {
            if !line.contains("Button::new(") {
                continue;
            }
            let chain: Vec<&str> = lines[start..]
                .iter()
                .take(25)
                .enumerate()
                .take_while(|(index, line)| *index == 0 || !line.contains("Button::new("))
                .map(|(_, line)| *line)
                .collect();
            if chain.iter().any(|line| line.contains(".xsmall()"))
                && chain.iter().any(|line| line.contains(".label("))
            {
                found.push(format!("{}:{}", path.display(), start + 1));
            }
        }
    }
    assert!(
        found.is_empty(),
        "{} xsmall buttons with a text_xs label:\n{}",
        found.len(),
        found.join("\n")
    );
}

fn offenders(dir: &Path, found: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            offenders(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs")
            && path
                .file_name()
                .is_some_and(|name| name != "style_guard.rs")
        {
            let text = std::fs::read_to_string(&path).unwrap();
            for (number, line) in text.lines().enumerate() {
                // A painted scale may keep a pixel size, said on its line.
                if FORBIDDEN.iter().any(|pattern| line.contains(pattern))
                    && !line.contains("physical:")
                {
                    found.push(format!("{}:{}", path.display(), number + 1));
                }
            }
        }
    }
}

#[test]
fn text_sizes_stay_on_the_type_roles() {
    editor_sources();
    let mut found = Vec::new();
    offenders(
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")),
        &mut found,
    );
    assert!(
        found.is_empty(),
        "{} uses of text_xs or a pixel text_size:\n{}",
        found.len(),
        found.join("\n")
    );
}

/// Files whose layout is on `rem` tokens: a `px(` there is a physical size
/// (a hairline, a hit tolerance, measured pixels) and says so on its line.
const PIXEL_FREE: &[&str] = &[
    "timeline.rs",
    "timeline_header.rs",
    "timeline_clip.rs",
    "meter.rs",
];

#[test]
fn pixel_layout_stays_out_of_the_timeline() {
    let src = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
    let mut found = Vec::new();
    for name in PIXEL_FREE {
        let path = src.join(name);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (number, line) in text.lines().enumerate() {
            if line == "#[cfg(test)]" {
                break;
            }
            if line.contains("px(") && !line.contains("physical:") {
                found.push(format!("{}:{}", path.display(), number + 1));
            }
        }
    }
    assert!(
        found.is_empty(),
        "{} pixel sizes without `physical:`:\n{}",
        found.len(),
        found.join("\n")
    );
}

/// The body of `fn <name>(` in `text`: from its line to the next line that
/// starts another method at the same indent.
fn function_body<'a>(text: &'a str, name: &str) -> Vec<(usize, &'a str)> {
    let lines: Vec<&str> = text.lines().collect();
    let head = format!("fn {name}(");
    let Some(start) = lines.iter().position(|line| line.contains(&head)) else {
        return Vec::new();
    };
    let indent = lines[start].len() - lines[start].trim_start().len();
    lines
        .iter()
        .enumerate()
        .skip(start)
        .take_while(|(index, line)| {
            *index == start
                || !(line.len() - line.trim_start().len() == indent
                    && (line.trim_start().starts_with("fn ")
                        || line.trim_start().starts_with("pub(crate) fn ")))
        })
        .map(|(index, line)| (index + 1, *line))
        .collect()
}

/// Icon-only buttons in the transport bar and the timeline footer carry an
/// accessible name: a tooltip only describes a button.
#[test]
fn transport_icon_buttons_have_accessible_names() {
    let src = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
    let mut found = Vec::new();
    for (file, function) in [
        ("viewer.rs", "transport_bar"),
        ("timeline.rs", "timeline_footer"),
    ] {
        let text = std::fs::read_to_string(src.join(file)).unwrap();
        let body = function_body(&text, function);
        assert!(!body.is_empty(), "{file}: no fn {function}");
        for (offset, (number, line)) in body.iter().enumerate() {
            if !line.contains("Button::new(") {
                continue;
            }
            let chain: Vec<&str> = body[offset..]
                .iter()
                .take(25)
                .enumerate()
                .take_while(|(index, (_, line))| *index == 0 || !line.contains("Button::new("))
                .map(|(_, (_, line))| *line)
                .collect();
            if chain.iter().any(|line| line.contains(".icon("))
                && !chain
                    .iter()
                    .any(|line| line.contains(".accessibility_label("))
            {
                found.push(format!("{file}:{number}"));
            }
        }
    }
    assert!(
        found.is_empty(),
        "{} icon buttons without an accessible name:\n{}",
        found.len(),
        found.join("\n")
    );
}

/// Calls of the Inspector's old row and section helpers, which the shared
/// `InspectorRow` and `InspectorSection` replaced.
fn old_helper_calls(dir: &Path, found: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            old_helper_calls(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs")
            && path
                .file_name()
                .is_some_and(|name| name != "style_guard.rs" && name != "inspector.rs")
        {
            let text = std::fs::read_to_string(&path).unwrap();
            for (number, line) in text.lines().enumerate() {
                if [
                    "inspector_row(",
                    "inspector_section(",
                    "inspector_note(",
                    "LABEL_WIDTH",
                ]
                .iter()
                .any(|pattern| line.contains(pattern))
                {
                    found.push(format!("{}:{}", path.display(), number + 1));
                }
            }
        }
    }
}

#[test]
fn old_inspector_helpers_are_gone() {
    let mut found = Vec::new();
    old_helper_calls(
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")),
        &mut found,
    );
    assert!(
        found.is_empty(),
        "{} uses of the old inspector helpers:\n{}",
        found.len(),
        found.join("\n")
    );
}

/// The Inspector's own files pad their lines with
/// `metrics::inspector_padding()`, as `InspectorRow` does, so every value
/// edge beside `inspector_label_width()` lands on the rows' edge.
#[test]
fn inspector_lines_share_the_row_padding() {
    let src = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
    let mut found = Vec::new();
    for name in [
        "layer_panel.rs",
        "element_rows_view.rs",
        "property_panel_view.rs",
    ] {
        let text = std::fs::read_to_string(src.join(name)).unwrap();
        for (number, line) in text.lines().enumerate() {
            if line.contains(".px_3()") {
                found.push(format!("{name}:{}", number + 1));
            }
        }
    }
    assert!(
        found.is_empty(),
        "{} lines padded px_3 beside the rows:\n{}",
        found.len(),
        found.join("\n")
    );
}
