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
const PIXEL_FREE: &[&str] = &["timeline.rs", "timeline_header.rs", "timeline_clip.rs"];

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
