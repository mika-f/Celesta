# Companion projects evaluate unfiltered (2026-10-28)

The exporter no longer copies a React export's companion project to drop its
`audio` items before evaluating its visuals (issue #572). The publish half of
#572 landed with `StagedFile` (see `2026-10-28-staged-file.md`): stills,
subtitles and video already publish through `exporter/src/output.rs`.

## What changed

- `crates/exporter/src/project.rs`: `visual_only_project` is gone. It cloned
  the whole project on every React video or stills export with a companion
  project.
- `render_react_video` (`render.rs`) and `export_react_stills_with`
  (`stills.rs`) build their `Evaluator` from the project as given.
- `Evaluator::visual_layer` (`crates/evaluator/src/lib.rs`) matches the
  content before evaluating transform, opacity and effects, so an `audio`
  item returns no layer without evaluating them. The filter used to skip that
  work; now every render path skips it, the editor's included. When an item
  has both a failing content and a failing transform, the content error now
  comes first.

## Behavior

Rendered frames are unchanged: an `audio` item already evaluated to no
layer. The exporter test `companion_project_visuals_keep_dialogue_but_skip_audio`
(formerly `visual_only_project_keeps_dialogue_and_component_but_drops_audio`)
checks that the unfiltered project gives the same single dialogue layer.

`Evaluator::new` now validates the whole companion project on the visual
path too. A project from `Project::load` (the CLI and editor) is validated
already, so only a caller of the public Rust API that passes an unvalidated
`Project` sees a difference:

- An invalid `audio` item, or an `audio` item sharing an id with another
  item, now fails React stills exports, which never validated it before.
- React video exports with a companion project already validated it in full
  when building the audio mixdown (`build_audio_graph`), but only after every
  frame was rendered. They now fail before rendering.

## Checks

```sh
PKG_CONFIG_PATH=/opt/homebrew/opt/ffmpeg@8/lib/pkgconfig cargo test -p celesta-evaluator -p celesta-exporter
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Locally, clippy 0.1.99 also flags `large_enum_variant` on
`crates/editor/src/layer_select.rs` `SourceLookup`, which this change does
not touch; every other crate passes.
