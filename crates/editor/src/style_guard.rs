//! Keeps text sizes on the editor's type roles.
//!
//! At the editor's 13px `rem`, `text_xs` is about 9.75px, too small to read,
//! and a pixel `text_size` steps outside the scale altogether, except on a
//! painted scale whose labels share its pixel geometry (marked `physical:`
//! on the line). Body text, values and titles are `text_base`; labels and
//! secondary text `text_sm`.

use std::path::Path;

const FORBIDDEN: [&str; 2] = ["text_xs(", "text_size(px("];

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
