# Export presets and the export dialog, P3 (2026-11-09)

Phase P3 of the media components design
(`docs/superpowers/specs/2026-11-08-media-components-design.md`, #782;
plan `docs/superpowers/plans/2026-11-08-media-components-p3.md`), on top
of P2b (`docs/handoff/2026-11-08-media-pipelining.md`). An export can take
a named choice of muxer, encoders, and settings from an
`export-presets.json`, the user's or the project's, by name or as the
project's `default`; the CLI, MCP, and `@celesta.studio/exporter` look
names up the same way; and the editor's Export… opens an export dialog in
place of the bare save panel, with presets, the components and their
settings, the range, and the loudness. A stale preset warns and falls
back; an explicit value stays strict.

## Specification

- **The files** (`celesta_exporter::presets`, `crates/exporter/src/presets.rs`).
  `export-presets.json` (`PRESETS_FILE`), `version` 1 (`PRESETS_VERSION`).
  The user's is `celesta_plugin_host::user_settings_dir()/export-presets.json`
  (`user_presets_path`), beside `settings.json`: `~/Library/Application
  Support/Celesta` on macOS, `%APPDATA%\Celesta` on Windows,
  `$XDG_CONFIG_HOME/celesta` (default `~/.config/celesta`) elsewhere. The
  project's is at `project_root(source)` (the nearest folder holding
  `celesta.config.json`, else the entry's folder). `PresetsFile { version,
  presets: Vec<Preset>, last, default, other }`; `Preset { name, output:
  PresetOutput }` with `PresetOutput { muxer, video: PresetEncoder { component,
  options, other }, audio: PresetAudio::{Encode, None("none")}, other }`;
  component references are `<plugin>/<component>` strings. `last` is kept
  in the user file only, `default` in the project file only. Unknown
  top-level, per-preset, and per-encoder (`video`, `audio`) keys survive a
  rewrite (`other`), which matters because every Export… in the app
  rewrites the user file; `"audio": "none"` still reads as
  `PresetAudio::None` (the untagged enum tries it first).
- **Reading and writing.** `PresetsFile::load` checks `version` on the raw
  JSON before the shape, so a version-2 file of another shape still says
  `unsupported version 2; this Celesta reads version 1`; other failures are
  `cannot be read: …`, `is not valid JSON: …`, and `is not an export
  presets file: …`, each as `ExportError::InvalidPresets { path, message }`
  (`invalid_presets`, deviation 42). `load_if_present` is `None` for a
  missing file. `save` writes pretty JSON (two spaces) and a trailing
  newline through a `NamedTempFile` in the same folder, renamed over the
  target; a failure leaves the old file. `update_presets(path, change)`
  loads (or starts) a file, applies `change`, and saves; `put` replaces a
  preset of the same name in place or appends it.
- **Lookup** (`PresetLookup { file, project, user }`,
  `PresetLookup::for_source(source, preset_file)`). `find(name)` tries
  `--preset-file`, then the project file, then the user file, and the
  first match wins. A missing file is skipped; a file the lookup reaches
  and cannot read is `invalid_presets`, so a broken user file does not fail
  an export that names no preset in a project without a `default`. A
  project match also reads the user file and warns
  ``the project's preset `{name}` hides the user preset of the same name``.
  `project_default()` reads the project file's `default`. `FoundPreset {
  preset, source: PresetSource::{File, Project, User}, path, warnings }`.
- **Resolving** (`resolve_output(choice, output, registry, presets,
  describe)`, deviation 40; `resolve_preset_output(choice, base, output,
  registry, describe)`). `choice.preset` is looked up; no match is
  `UnknownPreset { name, default_in: None }` (with P2a's x264 hint for an
  x264 preset name). Without one, the project `default` applies when the
  choice names no muxer, no video component, and audio `Encode` with no
  component; a default found in no file is `UnknownPreset { default_in:
  Some(project file) }`, "the default preset `x` of <path> is in no
  presets file", with no hint (deviation 43). Then over the base:
  - **Muxer**: explicit, else the preset's (parsed, installed, described
    available), else the extension's. A named or preset muxer whose
    `extensions` lack the output's (ignoring case) warns `{muxer} does not
    write .{ext} files; the file is written as {muxer name} anyway`; an
    output without an extension, and a muxer chosen by the extension,
    never warn.
  - **Encoders**: explicit, else the preset's, checked for kind, against
    the muxer's codecs (`writes_any_codec`), and for availability. One that
    cannot be used falls back to P2a's default with ``preset `{name}`:
    {text} is unavailable ({reason}); using the default instead``, and its
    options are dropped with it. A preset's audio encoder against a muxer
    that takes no audio leaves the audio out, with ``preset `{name}`:
    {text} is unavailable ({muxer} takes no audio); the audio is left
    out``. A preset with `"audio": "none"` and no explicit audio encoder has
    no audio, and `--audio-option`s then warn ``audio options are ignored:
    preset `{name}` leaves the audio out``.
  - **Options**: a preset's options apply to the resolved encoder only when
    it is the preset's `component`, or when neither the preset nor the
    choice names one. They are `celesta_media::settings::sanitize`d
    (``preset `{name}`: {component} has no setting `{key}`; it is left out``,
    ``preset `{name}`: {component} {key} {message}; using its default``),
    explicit options go over them with `settings::layer` (which drops a base
    value the layered values hide, repeating until nothing more is hidden),
    and the result is validated strictly, so a remaining failure is an
    explicit value's `invalid_option`.
  - **Deviation 44**: a project `default` (not an explicit `preset`) that
    resolves a component of a `MediaOrigin::Project` plugin warns
    ``the project's default preset `{name}` uses {reference}, from the
    project's plugin {plugin}``, once per such muxer, video, and audio
    component.
  - Warning order: the lookup's, then the preset's, then deviation 44's.
    `ResolvedOutput.warnings` reach the caller as `ExportProgress::Warning`
    before `ExportProgress::Output` (video and React paths), so stderr's
    `warning:` lines and the JSON report's `warnings` carry them.
- **Front ends.** `ExportOptions.presets: PresetLookup`, empty by default
  (deviation 41). The CLI's `--preset <name>` and `--preset-file <path>`
  set it with `PresetLookup::for_source(source, --preset-file)` for video
  outputs only (stills and subtitle-only exports keep the empty lookup;
  both flags are component flags that stills reject). `--preset-file` is
  read with `PresetsFile::load`, and a missing or invalid one is a usage
  error, `--preset-file <path>: <message>` (P2a's R15, now with the real
  parser). MCP `export_video`'s `preset` and `presetFile` do the same
  (`presetFile` made absolute against the server's working directory;
  `presetFile <path>: <message>` as its argument error).
  `@celesta.studio/exporter`'s `preset` and `presetFile` pass
  `--preset`/`--preset-file`. Report: `invalid_presets` has no hint, and
  `unknown_preset` with `default_in` has none.
- **The dialog's model** (`crates/editor/src/export_dialog/model.rs`,
  `ExportDialogModel`). Holds the registry's encoders and muxers, ranked
  bundled, user, then project, each plugin `Described::Checking` until its
  `describe` lands. Component labels `"{name} · {plugin}"`, plus
  ` (project)`; `OptionState::{Checking, Available, Unavailable(reason)}`.
  `encoder_options(track)` lists the encoders whose codecs the chosen muxer
  takes (none for audio when it takes no audio). `apply(resolved, selected,
  asked)` sets the choice from a resolve, keeping each encoder's
  `settings::differences`, and records which encoders count as chosen
  from `asked: Asked::{Preset(output), Defaults, Container}` (see the
  container rule below); `choose_encoder` resets that encoder's settings
  and marks it chosen (`None` audio as chosen "None");
  `set_setting` drops values the new one hides; editing clears the
  selected preset but keeps its warnings. `fields(track, advanced)` gives
  the visible fields, ungrouped first, then groups in schema order, each
  with `sanitize`'s message as its error. `output_choice()` is an explicit
  choice (`preset: None`, every component named, every given value as
  `SettingInput::Value`), or the reason Export… is blocked ("Checking…",
  `"{label} — {reason}"`, `"{label}: {field} {message}"`).
  `saved_output()` writes the muxer, components, and differences, and
  `"none"` for no audio. `initial_base()` returns `InitialBase::{Preset(source,
  name, output), Last(output), None}` and warnings (ruling 3): the project
  file's and the user file's read errors (always, since the list then
  leaves that file's presets out and nothing else would say so), and a
  `default` naming no preset;
  `initial_output(base)` is `export.<ext>` from the base's muxer when
  installed, else the `.mp4` muxer, else the first ranked non-project
  muxer (rulings 2, 4). `container_choice(muxer)` follows the container
  rule (ruling 9):
  - The video encoder is kept, with its settings, only when it counts as
    chosen (picked with `choose_encoder`, or named by the applied preset,
    not by `last`) and the new muxer takes its codec; otherwise the choice
    names none and the resolve takes the new muxer's default, so MP4
    (x264, AAC) → QuickTime gives ProRes 4444 and PCM, as `.mov` did before
    P3. An encoder's settings are dropped with it.
  - Audio chosen as "None" (or a preset's `"none"`, or `last`'s/defaults'
    no audio where the muxer takes audio) stays `AudioChoice::None`. An
    audio encoder that counts as chosen is kept when the new muxer takes
    it; when the new muxer takes no audio at all, it is set aside with its
    settings (`ChosenAudio::SetAside`) and comes back with the next muxer
    that takes it. Audio that was the default, or that a muxer without
    audio dropped, resolves the new muxer's default
    (`AudioChoice::Encode(default)`), so MP4 → IVF → MP4 has AAC again.
  `can_save(target)` is "Save the
  project first" for an untitled document, the file's read error, or "This
  system has no settings folder to save presets in"; `replaces(target,
  name)` compares the trimmed name case-sensitively.
- **The dialog** (`export_dialog/view.rs`, `view/{choice,fields,save}.rs`).
  Export… in the title bar opens it; the title bar keeps `Export…` and
  `Cancel export`, and the loudness `Select` moves into the dialog. From
  top to bottom: Preset (an ungrouped "None", then "This project", then
  "My presets"; empty groups left out), Container, Video encoder and its
  fields (groups under muted labels, advanced fields in a `Collapsible`
  under a ghost "Advanced" button, closed), Audio encoder (with "None") and
  its fields, Range ("Whole composition", or the in–out timecodes with
  `Clear`; a React entry shows "Whole composition" only; in- and out-points
  are still set on the timeline), and Loudness (`ExportLoudness::ALL`).
  Footer: `Save as preset…`, `Cancel`, `Export…` (primary, Enter),
  disabled while blocked, with the reason under it. Each plugin with an
  encoder or muxer is described on the background executor when the dialog
  is made; unavailable components are disabled items with their reason.
  The first resolve and every preset or container change run
  `resolve_preset_output` in the background; warnings show under the
  section that resolved, with the `TriangleAlert` icon; the view shows the
  exporter's `code` quoting as typographic quotes (“Web”), the CLI keeps
  the backticks. Number fields read
  through `parse_number_input`, and a typo blocks Export… ("Delay: Enter a
  number") until the field is fixed or reset.
- **Save as preset…** opens an inline form above the footer: a Name
  `Input`, a "Save to" `Select` (This project / My presets; This project
  disabled with `can_save`'s reason), a `Save` button that reads `Replace`
  for an existing name, and a close (X) icon. `Save` is disabled while
  `Export…` is (a resolve running, an invalid field, a typed number that is
  not one), and the form shows that reason above the footer's buttons in
  place of the footer's own line. Saving creates the user settings folder
  with `celesta_plugin_host::create_private_dir_all` (0700; a project's
  root already exists), runs `update_presets(.., put)`, takes the written
  file into the model, and selects the preset.
- **Export…** emits `ExportDialogEvent::Export`; `EditorView` writes
  `last` with `export_control::remember_last` (`update_presets` on the user
  file, creating its folder with `create_private_dir_all`). A failure goes
  to Problems under the new `Source::ExportPresets` ("Export presets", a
  warning, "Couldn’t remember the export settings"), which `start_export`'s
  `clear(Source::Export)` does not touch, and the next successful write
  clears; the export goes ahead. It then opens the save
  panel with `export_suggested_name(path, name, first extension)` and
  passes the chosen path through `export_output(path, the muxer's
  extensions)`, which keeps a path with one of them and otherwise sets the
  first. `ExportRequest.output_choice` carries the dialog's explicit choice
  to `ExportOptions.output`, so the editor's export reads no presets file.
  `Cancel`, Escape, or the backdrop close the dialog and keep its state;
  `set_plugins` (Reload Plugins, another project) drops it, and a dialog
  whose project presets file differs (an untitled document saved since)
  is rebuilt.

## Deviations from the spec

Numbered from 40 (P2a used 8–18 and 24–26, P2b 19–23, 27, 28, P2c 30–31);
recorded in the spec's Presets, Resolving an output, Exporter API, and
Phases sections, and in Decision 4 and the Ranking paragraph (44).

40. **`resolve_output` also takes the files presets are looked up in**
    (`presets: &PresetLookup`), beside P2a's `describe`, and
    `ResolvedOutput` gains `warnings`. A front end decides which files an
    export may read. `resolve_preset_output` takes an already-found preset
    instead, for the editor's `last` and its dialog.
41. **`ExportOptions::default()` reads no presets file**
    (`ExportOptions.presets`, all `None`). The CLI and MCP set it from the
    export's source; a library caller, and every test, is not changed by a
    file in the user's home. The editor's dialog reads the files itself and
    exports an explicit choice.
42. **A presets file that cannot be read or parsed is a new error,
    `invalid_presets`** (`ExportError::InvalidPresets { path, message }`),
    as is a `version` other than 1, raised only when the lookup reaches
    that file. A `--preset-file` (MCP `presetFile`) that is missing or
    invalid stays a usage error, now checked with `PresetsFile::load`.
43. **`UnknownPreset` gains `default_in: Option<PathBuf>`**: a project
    `default` naming a preset found in no file says so ("the default preset
    `x` of <path> is in no presets file"), with the same code
    `unknown_preset` and no hint.
44. **A project's `default` preset counts as naming its components**
    (review of mika-f/Celesta#783). Decision 4 says a project plugin's
    components are never chosen by default; a project `default` naming one
    uses it (the plugin must still be approved, or it is not installed and
    the preset falls back as stale), and the export warns once per such
    component. An explicit `preset` naming one does not warn. As built, the
    CLI and MCP search no project plugin folders (`--plugin-dir` plugins
    are `MediaOrigin::User`), and the editor exports an explicit choice, so
    the warning is reached only by a `resolve_output` caller whose registry
    has project plugins; the dialog marks such components "(project)".
    `a_project_default_naming_a_project_component_warns` covers it.

## Decisions

The spec left these to the plan:

- **When the project `default` applies:** no `preset`, no muxer, no video
  `component`, and audio `Encode` with no `component`, so
  `--audio-encoder none` counts as a component flag. Options alone
  (`--video-option crf=20`) layer over it. Its name is looked up in the
  same order as any name.
- **A preset's encoder options apply only to its own encoder**, or to the
  default encoder when neither the preset nor the choice names one. An
  explicit other encoder drops them silently (overridden, not stale). A
  preset component that cannot be used falls back with a warning, and its
  options go with it.
- **Layering inside one encoder:** sanitize the preset's options (each
  drop warned), put explicit options over them, drop silently a preset
  value the result hides, validate strictly.
- **Audio options with the audio left out by a preset** are ignored with a
  warning.
- **A named or preset muxer that does not list the output's extension** is
  used (P2a's R7) with a warning; one chosen by the extension never warns.
  Explicit `--muxer` (MCP/TS `muxer`) gets this warning too, which is new
  for P2a's users.
- **Saved options** (presets and `last`) are the visible settings that
  differ from their defaults (`settings::differences`), so a preset follows
  a plugin's later defaults for what the user did not change.
- **The dialog's first choice:** the project's `default` (looked up in the
  project file, then the user file); else the user file's `last`; else the
  default muxer for `.mp4` (or the first ranked non-project muxer) with its
  default encoders. A `default` naming no preset, or an unreadable project
  file, warns under Preset and falls back to `last`; an unreadable user
  file warns under Preset too.
- **Save as preset…** is an inline section, not a second dialog; `Replace`
  for an existing name in the chosen file, replaced in place; "This
  project" disabled with "Save the project first" for an untitled
  document; a file that could not be read is not written, and the section
  shows its error.
- **Range and loudness** live in the dialog; the range is text with
  `Clear`, set on the timeline as before.
- **Labels:** `"{name} · {plugin}"`, ` (project)`, "Checking…", and
  `"{label} — {reason}"` for an unavailable one.
- **Preset list:** "None" first, then "This project", then "My presets";
  editing any field sets the selection back to "None"; the warnings stay
  until the next preset or container change.

## Rulings

From the session's ledger; each with what it costs if it is wrong.

1. `PresetsFile` also implements `Default` (= `new()`), which clippy's
   `new_without_default` requires. Costs nothing.
2. The dialog's model re-ranks muxers locally (bundled before user, never
   project) for `initial_output` rather than widening the exporter's API.
   If wrong, a later change to the ranking needs two edits (P2a's
   `default_muxer` and the model).
3. `initial_base` returns `InitialBase::{Preset(PresetSource, String,
   PresetOutput), Last(PresetOutput), None}` and warnings, rather than the
   plan's `(String, PresetOutput)`, so the dialog can select the preset
   and show why a `default` was not used. If wrong, one more enum for the
   view to match.
4. `initial_output(base)` takes the base and uses its muxer's first
   extension; the model gains `extension_of(&ComponentRef)`. This avoids a
   false extension warning on open and on a container change. Cheap if
   wrong.
5. The model gains `choice()`, which the view needs to render its
   `Select`s. No cost.
6. No copied helpers: `celesta_media::settings::effective` became `pub`,
   and the encoder/muxer codec check became the exporter's public
   `writes_any_codec`, used by `resolve.rs` and the model. One rule in one
   place; no cost.
7. The view's interface additions are accepted: `ExportDialog::new` takes
   `user_file` (so tests never read the real home) and `loudness`; events
   `Loudness` and `ClearRange`; the model's `container_choice`. The save
   form sits above the footer with an X instead of a second `Cancel`, and
   the dialog tests use a `gpui-pre` `test-support` dev-dependency instead
   of gpui-kit's (whose `test-support` pulls gpui-fast, which pins `taffy`
   and `unicode-properties` versions the workspace cannot take). If wrong,
   a small layout move; the tests drive Enter instead of a click.
8. `last` is saved when Export… is pressed, before the save panel, so
   cancelling the panel still updates it: the spec says the choice is
   remembered, not only on a finished export. If wrong, one call moves
   after the panel. A failure is kept under its own Problems source
   (`ExportPresets`) rather than by moving the call after `start_export`,
   so this ruling holds.
9. **A container change keeps only encoders chosen explicitly** (final
   review): one picked in the dialog or named by the applied preset, and
   only when the new muxer takes its codec; everything else takes the new
   muxer's defaults. `last` counts as naming nothing (else every user who
   ever exported would keep H.264 in QuickTime). Audio the user (or a
   preset) left out stays out; audio a muxer without audio dropped comes
   back. This matches `.mov` → ProRes before P3 and the guide's
   transparent-video path. If wrong, users who switch containers lose an
   encoder (and its settings) they did not choose explicitly.
10. **Final review minors deferred** to Not done below: none produces wrong
    output; the cost is small UX rough edges.

## Docs

- The user guide's export page (en and ja): "From the app" describes the
  dialog, its sections, "Checking…", disabled components, "(project)", the
  first choice, the save panel's extension, Save as preset… (This project
  or My presets, Replace, saving the project first), and stale presets in
  the dialog; "Choosing encoders and settings" says a `--muxer` that does
  not list the extension warns; a new "Export presets" section gives the
  two files and where they live, the file's shape, `--preset` and
  `--preset-file` and the lookup order, the shadowing warning, how flags go
  over a preset, when the project's `default` applies, `unknown_preset`
  and `invalid_presets`, stale presets, and MCP's and TypeScript's
  `preset`/`presetFile`, with a macOS/Windows example; the loudness and
  transparent-video paragraphs point to the dialog; the JSON paragraph
  lists `invalid_presets`. The plugins page (en and ja) says a project's
  own components are used only in the app (the CLI and MCP never load a
  project's plugins): when chosen in the dialog, marked "(project)", or
  when the project's default preset names them. The final fix wave added
  to "From the app": the container rule (MP4 → QuickTime gives ProRes 4444
  and PCM; None stays off; audio a container dropped comes back), an
  unreadable presets file named under Preset, `last` failures under
  Problems → Export presets, and `Save` blocked like `Export…`. Troubleshooting (en and ja) gains "An export
  fails with unknown_preset or invalid_presets". The website's content
  test counts the new tab group and toggle.
- The Agent Skill: `references/export.md` (the `--preset` and
  `--preset-file` rows, the muxer extension warning, an "Export presets"
  section, MCP's and TypeScript's `preset`/`presetFile`, the
  `unknown_preset` and `invalid_presets` codes), `references/errors.md`
  (both `unknown_preset` messages, `invalid_presets`, the extension and
  stale-preset warnings), and `SKILL.md`'s reference row.
- HANDOFF.md: the `celesta-exporter` row (presets, `presets.rs`), the
  `celesta-editor` row (the export dialog, `export_dialog/`), the
  `@celesta.studio/exporter` row, and the export notes.
- The spec: deviations 40–44 under Presets, Resolving an output, and
  Exporter API; Decision 4 and the Ranking paragraph (deviation 44); the
  Phases row's "As built".
- Doc comments: Tasks 1–3 already replaced the "presets arrive with P3"
  comments (`resolve.rs`, `options.rs`, `main.rs`, MCP `args.rs`,
  `packages/exporter`); `grep -rn "not available yet\|arrive with P3\|until
  P3" crates packages skills docs/handoff HANDOFF.md` finds only
  `docs/handoff/2026-11-08-media-encoders.md`, which stays as history.
- Examples: unchanged. `examples/greenscreen`, `afterimage`, `prism`, and
  `perfect-loop` give one or two `--video-option`s per command (P2a's
  move), which stay valid and read more plainly than a presets file;
  `afterimage/render.py` repeats its two flags per chunk from one place in
  the script. A project `default` would also change what the app's Export…
  starts with for those projects, which none of them asks for.

## Checks

On macOS 26.6.2, Apple M4 (10 cores, 32 GB), Metal, with `PKG_CONFIG_PATH`
at Homebrew's `ffmpeg@8`, at `7d17994a` plus this note's commit:

- `cargo build -p celesta-codec-test`, then `cargo test --workspace`
  (2,695 passed, 0 failed, 1 ignored as before),
  `cargo clippy --workspace --all-targets --locked -- -D warnings`, and
  `cargo fmt --all -- --check`.
- `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`
  (468 passed) and its clippy with `-D warnings`.
- `pnpm run test:runtime` (17 packages, 1,046 tests, all passed;
  `@celesta.studio/cli` 119, `@celesta.studio/components` 179), `pnpm --dir packages/exporter
  test` (24 passed), `pnpm --dir packages/website test` (52 passed),
  and `git diff --check`.

Unit and integration tests cover the rest (Tasks 1–5): the file's shape,
round trip, unknown keys, version, atomic save, and lookup order with
missing and broken files (`presets.rs`, 11 tests); `sanitize` and `layer`
(`settings.rs`); every resolve rule with exact warning texts (`resolve.rs`:
`a_named_preset_is_the_base`, `explicit_choices_layer_over_the_preset`,
`stale_preset_values_warn_and_reset`, `explicit_values_stay_strict_over_a_preset`,
`a_gone_preset_component_falls_back`,
`a_preset_muxer_against_another_extension_warns`,
`the_project_default_applies_without_components`,
`a_project_default_naming_a_project_component_warns`,
`a_missing_default_is_unknown_preset`, `the_shadow_warning_reaches_the_resolved_output`,
`audio_options_without_audio_are_ignored`, and more);
`encode::tests::resolve_warnings_reach_the_caller`; the report's codes;
the CLI (`cli_formats`: `a_project_preset_exports`,
`a_user_preset_is_found_and_a_stale_key_warns`,
`the_project_default_applies`, `a_broken_preset_file_is_a_usage_error`)
and MCP (`args::a_preset_file_must_be_a_presets_file`,
`export::export_video_uses_a_project_preset`); the dialog's model (17
tests); and the dialog in a GPUI window (`export_dialog_emits_the_chosen_output`,
`a_typed_number_that_is_not_one_blocks_export`,
`a_typo_left_behind_is_replaced_by_the_fields_value`,
`without_a_media_plugin_export_says_why`) and the editor's export helpers
(`export_options_take_the_dialogs_choice`,
`the_suggested_name_takes_the_muxers_extension`,
`export_output_keeps_the_muxers_extensions`). The live CLI and MCP preset
exports skip without a GPU adapter (they fail under
`CELESTA_REQUIRE_GPU=1`); they ran here.

**After the rebase.** P3 was rebased onto `claude/media-components-p2b`
`c7234365` after Task 4 (`git rebase --onto c7234365 0942fedb`; the stack
had been rebased upstream, and `settings::validate` now refuses an integer
outside `i64`). The rebased branch passed `cargo test --workspace` (2,688
passed, 0 failed), clippy with `-D warnings`, and `cargo fmt --check`
before Task 5; the counts above are from the rebased branch with Task 5.

**Final fix wave** (the final review's findings, on `6b992b2b`): `cargo
build -p celesta-codec-test`, then `cargo test --workspace` (2,703 passed,
0 failed, 1 ignored), `cargo clippy --workspace --all-targets --locked --
-D warnings`, `cargo fmt --all -- --check`, `pnpm --dir packages/website
test` (52 passed), and `git diff --check`. New tests, each seen failing on
the old code first: `unknown_keys_survive_a_rewrite` (keys inside `video`
and `audio`, and `"audio": "none"` still read as none),
`a_gone_preset_component_falls_back` (the audio-left-out warning), the
model's `a_container_change_takes_the_muxers_default_encoders`,
`audio_left_out_by_a_container_comes_back`,
`a_preset_names_what_a_container_change_keeps`, and
`a_broken_user_file_warns_when_the_dialog_opens`, the dialog's
`a_broken_user_file_shows_under_preset`,
`save_as_preset_waits_for_what_blocks_export`, and
`warnings_quote_without_code_marks`, and the editor's
`a_failed_last_survives_the_export_it_goes_on_with` (which also checks the
settings folder is created 0700).

**In the app.** The FFmpeg plugin was staged with `node
scripts/plugins/build.mts plugins/ffmpeg` (into `plugins/dist`) and
`target/debug/celesta-editor examples/greenscreen/composite.tsx` run with
`CELESTA_PLUGIN_PATH`. The session could not click in the app, so a local,
uncommitted patch (reverted before commit, checked with `grep`) opened the
dialog on start when `CELESTA_SHOT_DIALOG` was set (skipping the trust
prompt), set its state (Advanced open, or the save form open with the name
"Web"), pressed Export… after 6 seconds in an "export" mode, and skipped
the save panel when `CELESTA_SHOT_OUTPUT` was set. Windows were captured
with `screencapture -x -o -l <window id>` (the id from
`CGWindowListCopyWindowInfo`). The four screenshots are attached to the
PR, not committed:

- `export-dialog-1-default.png`: after `describe`, Preset None, "MP4 ·
  ffmpeg", "H.264 (x264) · ffmpeg", the x264 fields (Preset, Bit depth,
  then the "Rate control" group with Rate control and CRF, with their
  descriptions), Advanced closed, and the footer.
- `export-dialog-2-advanced.png`: Advanced open, showing Tune.
- `export-dialog-3-preset-warnings.png`: a temporary
  `examples/greenscreen/export-presets.json` (deleted since) with
  `default: "Web"`, `crf: 99`, and an unknown `speed`: "Web" applied, the
  x264 Preset field at "Slow", and the two warnings under Preset.
- `export-dialog-4-save-as-preset.png`: the inline form above the footer,
  with the name "Web", "This project", `Replace`, and the close icon.

The final fix wave added three, with the same kind of local patch (it
opened the dialog shortly after start in `examples/editor-demo.celesta.json`,
set in- and out-points at frames 24 and 72, and switched the container
to QuickTime; the window left the screen after about two seconds in that
session, so the patch's timers were short and frames were captured in a
burst):

- `export-dialog-5-range-loudness.png`: QuickTime, with the whole body
  in view: Range "00:00:00:24 – 00:00:01:12" with `Clear`, and Loudness
  "Off".
- `export-dialog-6-quicktime.png`: MP4 · ffmpeg (x264, AAC) changed to
  QuickTime · ffmpeg gives ProRes (prores_ks) with Profile 4444 and PCM at
  24-bit integer.
- `title-bar-after.png`: the title bar with `Create New Project…`,
  `Open…`, and `Export…`, without the loudness button.

This is not a rendering change, so there is no before/after pair; the
only UI removed is the title bar's loudness button.

**One real export** from the dialog, with the "Web" project default and
`HOME` set to a scratch folder: `export-out.mp4` is H.264 1280×720 with
AAC, 3.0 s, and `ffmpeg -f null` decodes it cleanly; the scratch user
file's `last` is `{ muxer ffmpeg/mp4, video ffmpeg/libx264 { preset: slow },
audio ffmpeg/aac {} }`, with the out-of-range `crf` dropped.

Not run: Linux and Windows (CI), the dialog by hand with a mouse (the
tests drive Enter and the model's controls), and a project plugin's
components in the dialog.

## Not done

- **P2c's rebase.** When P2c (BT.709) lands in the stack, this branch is
  rebased onto it (never merged) and the PR's base changed; nothing here
  touches color.
- **Presets files.**
  - Saved files are created `0600` (`NamedTempFile`'s mode), so a
    project's `export-presets.json` written by the app loses group and
    other read; keeping the existing file's mode, or the umask default,
    would suit a file meant for a repository.
  - The failed-save test does not assert that the temporary file is gone.
  - A missing `--preset-file` is skipped silently by `PresetLookup`; the
    CLI and MCP check it first (usage error), but a library caller is not
    told.
  - `PresetsFile::load`'s shape errors ("is not an export presets file")
    carry serde's message without a line and column, since the shape is
    read from an already-parsed `serde_json::Value`.
- **Resolving.**
  - `layer` drops a base value hidden by another base value silently, so a
    plugin that adds a `visibleWhen` hides a stale value without a
    warning; its doc comment says "hidden by the layered values", and no
    test covers its repeated drops.
  - A preset-muxer fallback warning is lost when the extension's default
    then fails.
  - `resolve_preset_output` names its base `""` when it has none (never
    shown).
- **Front ends.** The live CLI and MCP preset tests need a GPU adapter; a
  GPU-free lookup case (`--preset nope` against a project file, a broken
  user file not failing an export that names no preset) would cover them
  in CI without one. The stale-preset warning on stderr without `--json`
  has no CLI test. The 30-line project fixture is duplicated between
  `cli_formats.rs` and MCP's `export.rs`. The usage messages rely on
  `PresetsFile::load`'s text starting with the path, and the CLI passes a
  relative `--preset-file` as given.
- **The dialog's model.**
  - A container change drops the settings of an encoder that was not
    chosen explicitly even when the new muxer's default is that same
    encoder (MP4 · ffmpeg → another MP4 muxer loses a CRF edited on the
    default x264); choosing the encoder in the list first keeps them.
  - A preset naming an audio encoder for a muxer that takes no audio does
    not set that encoder aside: a later container with audio takes its
    default, not the preset's encoder.
  - `choose_encoder` accepts any
  reference, and `Some` audio for a muxer without audio; `(Video, None)`
  does nothing. Untested: `output_choice`'s error for an unavailable
  component, and `set_setting`'s drop of a hidden value.
- **The dialog.**
  - Reopening the dialog does not re-read the presets files: a file edited
    by hand, or by another window, shows after Reload Plugins or another
    project.
  - `set_plugins` drops the dialog even while it is open; the open overlay
    then shows an empty dialog until it is closed.
  - The section "Preset" (the export preset) and the x264 field "Preset"
    share a label.
  - The dialog tests' `gpui = { package = "gpui-pre", version = "=0.3.11" }`
    dev-dependency pins gpui-pre exactly (ruling 7) and needs a bump with
    gpui-kit.
  - A plugin can be described twice at once on first open (the initial
    resolve and the per-plugin task; the registry's cache has no
    single-flight).
  - Edits made while a preset or container resolve runs are overwritten
    when it lands.
  - The preset `Select`'s section headers use gpui-component's deprecated
    `section()` hook rather than `render_section_header`.
  - A first-resolve failure shows twice (under Preset and as the footer's
    reason).
  - `#[allow(clippy::too_many_arguments)]` and `#[expect]` are mixed in
    `view.rs`.
  - Every keystroke rebuilds all the `Select`s through `sync`.
  - No view test for a whole save (writing the file), a stale preset's
    warnings, a container change in the window (the model covers the
    rule), Escape keeping state, or a click on Export… (the gpui-kit
    test-support conflict, ruling 7).
