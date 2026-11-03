//! Finds the `celesta` skill that `src/docs.rs` embeds. In the repository it
//! is `skills/celesta` at the root; a published crate cannot reach outside its
//! own folder, so `scripts/bundle-crate-files.mjs` copies the skill into
//! `bundled/` before `cargo publish`.

use std::path::PathBuf;

fn main() {
    let manifest =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let repository = manifest
        .ancestors()
        .nth(2)
        .map(|root| root.join("skills").join("celesta"));
    let dir = repository
        .filter(|dir| dir.join("SKILL.md").is_file())
        .unwrap_or_else(|| manifest.join("bundled").join("skills").join("celesta"));
    assert!(
        dir.join("SKILL.md").is_file(),
        "the celesta skill is missing from {}: run `node scripts/bundle-crate-files.mjs` before packaging",
        dir.display()
    );
    println!("cargo::rerun-if-changed={}", dir.display());
    println!("cargo::rustc-env=CELESTA_SKILL_DIR={}", dir.display());
}
