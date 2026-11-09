# Media components P3: export dialog and presets — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Export presets become real — a user `export-presets.json` beside
`settings.json` and an optional project `export-presets.json` with a
`default`, looked up by `--preset` (and MCP's and TypeScript's `preset`)
in `--preset-file`, then the project, then the user — and the editor's
Export… opens a dialog that chooses the container, the encoders and their
settings, and saves and applies presets, before the save panel.

**Architecture:**
- **The presets file** (`celesta-exporter`, new `presets.rs`). Typed
  `PresetsFile` / `Preset` / `PresetOutput`, read strictly by `version`,
  keeping keys this build does not know, written atomically. A
  `PresetLookup` names the three files an export may read (none by
  default) and finds a name in order.
- **Presets in `resolve_output`** (`resolve.rs`). Step 1 becomes real: a
  preset (named, or the project's `default` when no component is given) is
  the base, explicit choices and options are layered over it, and whatever
  of the base is stale (a gone or unusable component, an unknown key, a
  value out of range) is dropped or reset with a warning, while an explicit
  value stays strict (`invalid_option`). `ResolvedOutput` carries the
  warnings; `ExportSession::resolve` sends them as `ExportProgress::Warning`.
- **Front ends.** The CLI and MCP build `PresetLookup::for_source`; TypeScript
  already passes `--preset` / `--preset-file` and only its docs change.
- **The editor** (`crates/editor`). A pure `ExportDialogModel` (components,
  per-plugin `describe` state, the current choice, preset lists, warnings)
  and a GPUI `ExportDialog` entity that renders it in a gpui-kit `Dialog`.
  Structural changes (a preset, a container) are resolved off the UI thread
  through the same `resolve_preset_output` the CLI uses; field edits are
  local. Export… emits the explicit `OutputChoice`, opens the save panel with
  the muxer's first extension, records `last`, and starts the export.

**Tech stack:** Rust 2024 (MSRV 1.89), serde/serde_json, `tempfile` (already
a normal dependency of `celesta-exporter`), gpui-kit 0.9 (`Dialog`, `Select`
with `SelectGroup` and disabled items, `Input`, `Checkbox`, `Collapsible`,
`Button`, lucide `IconName`), `#[gpui_kit::test]` with gpui-kit's
`test-support` feature for one UI integration test.

**Spec:** `docs/superpowers/specs/2026-11-08-media-components-design.md`
(Phases → P3; Settings schema → Presets; Selection → Resolving an output
step 1 and 6, Editor, CLI, MCP and `@celesta.studio/exporter`; Exporter
API; Testing → `celesta-exporter`). Read it with this plan. P2a's plan and
handoff (`docs/superpowers/plans/2026-11-08-media-components-p2a.md`,
`docs/handoff/2026-11-08-media-encoders.md`) define `resolve_output`,
`OutputChoice`, `SettingInput`, `ExportSession::resolve`, and the error
codes this plan builds on.

## Global constraints

- This PR stacks on P2b's (`claude/media-components-p2b`, mika-f/Celesta#793).
  P2c (BT.709) is a sibling on its own branch; when its PR lands in the
  stack, this branch is **rebased** onto P2c's (never merged) and the PR's
  base changed. Nothing here touches color.
- Every name P2a/P2b define (`resolve_output`, `OutputChoice`, `EncoderChoice`,
  `AudioChoice`, `SettingInput`, `ResolvedOutput`, `ExportSession`,
  `ComponentRef`, `MediaComponentInfo`, `PluginDescription`, `SettingField`,
  `Settings`, `X264_PRESETS`) is used as spelled in the code at `0942fedb`;
  where this plan changes one, it says so.
- File name `export-presets.json` (`PRESETS_FILE`), `version` **1**
  (`PRESETS_VERSION`). The user file is
  `celesta_plugin_host::user_settings_dir()/export-presets.json`, beside
  `settings.json` (never inside it). The project file is
  `celesta_plugin_host::project_root(source)/export-presets.json`.
- The file shape is the spec's: `{ "version": 1, "presets": [ { "name",
  "muxer", "video": { "component", "options" }, "audio": { … } | "none" } ],
  "last": { … } }`; the project file has no `last` and may have
  `"default": "<name>"`. Component references are stored as
  `<plugin>/<component>` strings.
- Lookup order for a name: `--preset-file` → project → user; first match
  wins. A project preset hiding a user preset of the same name warns.
- Stale presets: the CLI (`--preset` and a project `default`), MCP, and TS
  warn on stderr and in the JSON report's `warnings` and drop the key /
  reset the value to its default / fall back to the default component; the
  editor does the same and shows the warnings on the preset. A value given
  explicitly (CLI flag, MCP, TS, the editor's own fields) fails with
  `invalid_option`.
- Writes are atomic: a sibling temporary file in the same folder, renamed
  over the target; a failed write leaves the old file and no temporary file.
  JSON is pretty-printed (two-space indent) with a trailing newline.
- The editor's GUI follows the GPUI Kit design guides. Implementers of
  Tasks 4–5 **load the `gpui-kit` and `gpui-kit-design-guides` skills
  first** and read the Design Guides in full. The user's own rules: no
  gradients, no uppercase labels, corner radius 0 (the editor theme's
  radius, never a literal), lucide icons (`IconName`) rather than glyph
  arrows. Sentence case; `…` on commands that open more UI (`Export…`,
  `Save as preset…`).
- JS/TS tests use Vitest in TypeScript; Rust tests stay in Rust (AGENTS.md).
- Never amend commits; each task ends in a new commit. Stage files
  explicitly; never commit `.superpowers/`, `plugins/dist`, or scratch
  files. Commits are signed through 1Password; if signing fails with
  "failed to fill whole buffer", stop and ask the user to unlock 1Password —
  never bypass signing. **Every commit message ends with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`**, whatever
  model wrote it.
- Builds: `export PATH="/Users/natsuneko/Library/Application Support/mbx/bin:$HOME/.cargo/bin:$HOME/.local/share/mise/installs/node/26/bin:$HOME/.local/share/mise/installs/pnpm/12/bin:$PATH"`
  and `export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"`
  (Homebrew's default `ffmpeg` is 9 and breaks the build). Workspace tests
  need `cargo build -p celesta-codec-test` first.
- **Builds between tasks.** Task 1 is additive. Task 2 changes
  `resolve_output`'s and `ExportSession::resolve`'s signatures and fixes
  every caller in `celesta-exporter`, so the workspace builds after it
  (the CLI and MCP pass `PresetLookup::default()` until Task 3). Tasks 4–5
  touch `crates/editor` only.

## Deviations from the spec (recorded in Task 6's spec edit)

Numbered from 40 (P2a used 8–18 and 24–26, P2b 19–23, 27, 28, P2c 30–31).

40. **`resolve_output` also takes the files presets are looked up in**
    (`presets: &PresetLookup`), beside P2a's `describe`, and
    `ResolvedOutput` gains `warnings`. A front end decides which files an
    export may read; `resolve_preset_output` takes an already-found preset
    instead, for the editor's `last` and its dialog.
41. **`ExportOptions::default()` reads no presets file**
    (`ExportOptions.presets: PresetLookup`, all `None` by default). The CLI,
    MCP, and the editor set it from the export's source. A library caller
    (and every test) is not changed by a file in the user's home.
42. **A presets file that cannot be read or parsed is a new error,
    `invalid_presets`** (`ExportError::InvalidPresets { path, message }`),
    as is a `version` other than 1. It is raised only when the lookup
    reaches that file: a broken user file does not fail an export that
    names no preset and whose project has no `default`. A `--preset-file`
    (or MCP `presetFile`) that is missing or invalid stays P2a's usage error
    (R15), now checked with the real parser.
43. **`UnknownPreset` gains `default_in: Option<PathBuf>`**: a project
    `default` naming a preset found in no file says so ("the default preset
    `x` of <path> is in no presets file"), with the same code
    `unknown_preset`.

## Decisions the spec left to this plan

- **When the project `default` applies:** no `preset`, no `muxer`, no video
  `component`, and audio `Encode` with no `component` (so
  `--audio-encoder none` counts as a component flag, as the spec's "no
  component flags" reads). Options alone (`--video-option crf=20`) do not
  stop it; they layer over it. The default's name is looked up in the
  same order as any name.
- **A preset's encoder options apply only to its own encoder:** to the
  resolved encoder when it is the preset's `component`, or when the preset
  names none and the choice names none either. An explicit other encoder
  drops them silently (not stale; overridden). A preset component that
  cannot be used falls back to the default with a warning, and its options
  are dropped with it.
- **Layering order inside one encoder:** the preset's options are
  sanitized (unknown keys and bad values dropped, each warned), explicit
  options go over them, a preset value the layered values hide (by
  `visibleWhen`) is dropped silently, and the result is validated strictly,
  so any remaining failure is an explicit value's (`invalid_option`).
- **Audio options with audio left out by a preset** (`"audio": "none"` and
  no explicit audio encoder, but `--audio-option` given) are ignored with
  the warning `audio options are ignored: preset `{name}` leaves the audio
  out`.
- **A named or preset muxer that does not list the output's extension** is
  used anyway (P2a's R7), with the warning `{muxer} does not write .{ext}
  files; the file is written as {muxer name} anyway`. A muxer chosen by the
  extension never warns.
- **Saved options** are the visible settings that differ from their
  defaults (`celesta_media::settings::differences`), so a preset follows a
  plugin's later defaults for what the user did not change; `last` is saved
  the same way.
- **The dialog's first choice:** the project's `default` preset when the
  project file has one; else the user file's `last`; else the default muxer
  for `.mp4` (or, without one, the first muxer in rank order: bundled, then
  user) with its default encoders. Within a session, reopening the dialog
  keeps its state; Reload Plugins drops it.
- **"Save as preset…"** opens an inline section at the bottom of the dialog
  (a name `Input`, a "Save to" `Select` of "This project" / "My presets", and
  `Save` / `Cancel`), not a second dialog (Design Guides: avoid stacking
  overlays). When the name (trimmed, case-sensitive) exists in the chosen
  file, the button reads `Replace` and replaces it in place. "This project"
  is disabled with the reason "Save the project first" for an untitled
  document. A file that could not be read is not written; the section
  shows its error.
- **Range and loudness in the dialog.** Loudness moves from the title bar
  into the dialog (a `Select` of `ExportLoudness::ALL`); the title bar keeps
  `Export…` and `Cancel export`. The range is shown as text ("Whole
  composition", or the in–out timecodes with a `Clear` button); in- and
  out-points are still set on the timeline (`SetExportIn`/`SetExportOut`).
  A React entry shows "Whole composition" only.
- **Component labels:** `"{name} · {plugin id}"`, with ` (project)` appended
  for a project plugin's; "Checking…" while its plugin's `describe` runs;
  unavailable ones are disabled items followed by their reason
  (`"{label} — {reason}"`).
- **Preset list:** a `Select` with a group "This project" above a group "My
  presets", and a first, ungrouped item "None" (defaults). Editing any field
  after applying a preset sets the selection back to "None"; the preset's
  warnings stay until the next preset or container change.

## Review focus

The tests that pin each of these are in the task named.

1. **A broken or newer user `export-presets.json`** must not stop an export
   that does not reach it, nor block the dialog: the dialog shows the error
   on "My presets" and refuses to write that file (Task 1,
   `a_broken_file_is_read_only_when_the_lookup_reaches_it`; Task 4,
   `a_broken_presets_file_is_shown_and_never_written`).
2. **A preset from an older or newer plugin** (unknown key, value out of
   range, removed component, unparsable reference) exports with warnings,
   while the same value given explicitly fails (Task 2,
   `stale_preset_values_warn_and_reset`, `a_gone_preset_component_falls_back`,
   `explicit_values_stay_strict_over_a_preset`).
3. **Saving under an existing name** replaces that preset in place and keeps
   every other preset and unknown key (Task 1,
   `put_replaces_in_place_and_keeps_unknown_keys`; Task 4,
   `saving_an_existing_name_is_a_replace`).
4. **A failed write** (the target is a folder, the folder is read-only)
   leaves the old file byte-identical and no temporary file (Task 1,
   `a_failed_save_leaves_the_file_as_it_was`).
5. **`--preset web out.mov` where `web` is MP4** writes MP4 with a warning
   naming both, rather than silently or failing (Task 2,
   `a_preset_muxer_against_another_extension_warns`).

---

### Task 1: The presets file and its lookup (`celesta-exporter`)

**Files:**
- Create: `crates/exporter/src/presets.rs`
- Modify: `crates/exporter/src/lib.rs` (`pub mod presets;` and re-exports), `crates/exporter/src/error.rs` (`InvalidPresets`; `UnknownPreset { name, default_in }`), `crates/exporter/src/report.rs` (`invalid_presets`; the `default_in` case of `unknown_preset`, no hint)
- Test: `crates/exporter/src/presets.rs` tests, `crates/exporter/src/report.rs` tests

**Interfaces:**
- Produces, in `celesta_exporter::presets` (re-exported from the crate root):
  ```rust
  pub const PRESETS_FILE: &str = "export-presets.json";
  pub const PRESETS_VERSION: u32 = 1;

  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] #[serde(rename_all = "camelCase")]
  pub struct PresetsFile {
      pub version: u32,
      #[serde(default)] pub presets: Vec<Preset>,
      #[serde(default, skip_serializing_if = "Option::is_none")] pub last: Option<PresetOutput>,
      #[serde(default, skip_serializing_if = "Option::is_none")] pub default: Option<String>,
      #[serde(flatten)] pub other: serde_json::Map<String, serde_json::Value>,
  }
  pub struct Preset { pub name: String, #[serde(flatten)] pub output: PresetOutput }
  pub struct PresetOutput {
      pub muxer: Option<String>,                 // skip_serializing_if none
      pub video: Option<PresetEncoder>,
      pub audio: Option<PresetAudio>,
      #[serde(flatten)] pub other: serde_json::Map<String, serde_json::Value>,
  }
  pub struct PresetEncoder { pub component: Option<String>, #[serde(default)] pub options: Settings }
  #[serde(untagged)] pub enum PresetAudio { None(NoAudio), Encode(PresetEncoder) }
  pub enum NoAudio { #[serde(rename = "none")] None }

  impl PresetsFile {
      pub fn new() -> Self;                                         // version 1, nothing else
      pub fn load(path: &Path) -> Result<Self, ExportError>;        // InvalidPresets: unreadable, not JSON, wrong shape, version ≠ 1
      pub fn load_if_present(path: &Path) -> Result<Option<Self>, ExportError>;   // Ok(None) for NotFound
      pub fn save(&self, path: &Path) -> Result<(), ExportError>;   // atomic; InvalidPresets "cannot be written: …"
      pub fn find(&self, name: &str) -> Option<&Preset>;            // first with that exact name
      pub fn put(&mut self, preset: Preset) -> bool;                // true: replaced the first of that name in place; false: appended
  }
  /// Loads `path` (or starts a new file when there is none), applies `change`, and saves it.
  pub fn update_presets(path: &Path, change: impl FnOnce(&mut PresetsFile)) -> Result<PresetsFile, ExportError>;
  /// `user_settings_dir()/export-presets.json`; None where the platform has no settings folder.
  pub fn user_presets_path() -> Option<PathBuf>;

  #[derive(Clone, Debug, Default, PartialEq, Eq)]
  pub struct PresetLookup { pub file: Option<PathBuf>, pub project: Option<PathBuf>, pub user: Option<PathBuf> }
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum PresetSource { File, Project, User }
  #[derive(Clone, Debug, PartialEq)]
  pub struct FoundPreset { pub preset: Preset, pub source: PresetSource, pub path: PathBuf, pub warnings: Vec<String> }
  impl PresetLookup {
      /// `file` as given, the project file at `project_root(source)`, and the user file.
      pub fn for_source(source: &Path, file: Option<PathBuf>) -> Self;
      /// The first file (file, project, user) holding `name`; missing files are skipped,
      /// a file reached that cannot be read is InvalidPresets.
      pub fn find(&self, name: &str) -> Result<Option<FoundPreset>, ExportError>;
      /// The project file's `default`, if it has the file and one.
      pub fn project_default(&self) -> Result<Option<String>, ExportError>;
  }
  ```
- `ExportError::InvalidPresets { path: PathBuf, message: String }`, displayed
  `"{path}: {message}"`; `UnknownPreset { name: String, default_in: Option<PathBuf> }`
  displayed `unknown preset `{name}`` or `the default preset `{name}` of {path} is in no presets file`.
- `version` other than 1: message `unsupported version {n}; this Celesta reads version 1`.
- `FoundPreset.warnings` holds the shadow warning
  `the project's preset `{name}` hides the user preset of the same name`
  when a project match also exists in the user file (the user file is then
  read, and a broken user file is then an error too).
- `save`: `tempfile::NamedTempFile::new_in(parent)`, write, `sync_all`,
  `persist(path)`; the parent folder must exist (`update_presets` of the
  user file creates it with `celesta_plugin_host::create_private_dir_all`).

- [ ] **Step 1: Write failing tests** in `presets.rs`:
  - `the_spec_example_round_trips`: the spec's user-file JSON (with `"audio": "none"` in a second preset) parses; `presets[0].output.video.component == Some("ffmpeg/libx264")`, its `options["crf"] == SettingValue::Number(18.0)`; `last` is present; serializing and parsing again gives an equal value.
  - `unknown_keys_survive_a_rewrite`: a file with top-level `"future": 1` and a preset with `"note": "x"` loads, saves, and reloads with both keys.
  - `the_version_must_be_one`: `{"version": 2, "presets": []}` is `InvalidPresets` whose message contains `unsupported version 2`; a file without `version` is `InvalidPresets`.
  - `put_replaces_in_place_and_keeps_unknown_keys`: three presets `a`, `b`, `c`; `put(b')` returns true, order stays `a`, `b'`, `c`; `put(d)` returns false and appends; the top-level `other` is untouched.
  - `lookup_goes_file_then_project_then_user`: `x` in all three files → `File`; without `file` → `Project` with the shadow warning; only in user → `User`, no warning; nowhere → `Ok(None)`.
  - `missing_files_are_skipped`: a lookup whose three paths do not exist finds nothing and `project_default()` is `Ok(None)`.
  - `a_broken_file_is_read_only_when_the_lookup_reaches_it`: a user file holding `{` and a project file without `x`: `find("x")` is `InvalidPresets` naming the user path; with a `file` holding `x`, `find("x")` succeeds without reading the user file; `project_default()` succeeds either way. (A project match reads the user file for the shadow check, so a broken user file fails it too, as Interfaces says.)
  - `project_default_reads_the_project_file`: `{"version":1,"presets":[],"default":"web"}` → `Some("web")`.
  - `a_failed_save_leaves_the_file_as_it_was`: save into a path that is a folder fails with `InvalidPresets`; on Unix, a read-only parent folder with an existing file: `save` fails, the file's bytes are unchanged, and the folder holds no other entry.
  - `update_presets_starts_a_new_file`: `update_presets` on a missing path creates `{"version": 1, "presets": [ … ]}` ending in `\n`.
  - `the_user_file_sits_beside_settings_json`: `user_presets_path()` is `user_settings_dir()` joined with `export-presets.json` (compare to `user_settings_path()`'s parent).
- [ ] **Step 2:** In `report.rs` tests: `invalid_presets` is the code of `InvalidPresets`, with no hint; `UnknownPreset { default_in: Some(_) }` has code `unknown_preset` and no x264 hint even for `name: "fast"`; P2a's x264 hint test passes unchanged.
- [ ] **Step 3:** `cargo test -p celesta-exporter presets` fails (module missing).
- [ ] **Step 4: Implement.** Existing `UnknownPreset { name }` constructors gain `default_in: None`.
- [ ] **Step 5:** `cargo test -p celesta-exporter` and `cargo clippy -p celesta-exporter --all-targets -- -D warnings` pass.
- [ ] **Step 6: Commit:** `feat(exporter): export-presets.json and its lookup`.

### Task 2: Presets in `resolve_output` (`celesta-media`, `celesta-exporter`)

**Files:**
- Modify: `crates/media/src/settings.rs` (`sanitize`, `layer`), `crates/media/src/lib.rs` (re-export if `settings` items are re-exported there), `crates/exporter/src/resolve.rs` (step 1, the layering, warnings, `resolve_preset_output`), `crates/exporter/src/options.rs` (`ExportOptions.presets`; `OutputChoice.preset`'s doc), `crates/exporter/src/encode.rs` (`ExportSession::resolve`), `crates/exporter/src/exporter.rs` and `crates/exporter/src/react_entry.rs` (emit the warnings), `crates/exporter/src/lib.rs` (re-exports), `crates/exporter/src/main.rs` and `crates/mcp/src/args.rs` (pass `PresetLookup::default()` for now)
- Test: `crates/media/src/settings.rs` tests, `crates/exporter/src/resolve.rs` tests, `crates/exporter/src/encode.rs` tests

**Interfaces:**
- Consumes: Task 1's `PresetLookup`, `FoundPreset`, `PresetOutput`, `PresetEncoder`, `PresetAudio`, `ExportError::UnknownPreset { name, default_in }`.
- Produces, in `celesta_media::settings`:
  ```rust
  /// `given` without the keys `schema` lacks and the values their fields do not take, and why each went.
  pub fn sanitize(schema: &[SettingField], given: &Settings) -> (Settings, Vec<InvalidSetting>);
  /// `over` layered on `base`: base values hidden by the layered values are dropped, then the
  /// whole is validated as `validate` does (so an error is about `over`, or about `base` when it was not sanitized).
  pub fn layer(schema: &[SettingField], base: &Settings, over: &Settings) -> Result<Settings, InvalidSetting>;
  ```
- Produces, in `celesta_exporter`:
  ```rust
  pub struct ExportOptions { /* … */ pub presets: PresetLookup }   // Default: reads none (deviation 41)
  pub struct ResolvedOutput { pub muxer, pub video, pub audio, pub warnings: Vec<String> }
  pub fn resolve_output(choice: &OutputChoice, output: &Path, registry: &MediaRegistry,
      presets: &PresetLookup, describe: &mut dyn FnMut(usize) -> Arc<PluginDescription>,
  ) -> Result<ResolvedOutput, ExportError>;
  /// `resolve_output` with the base already found: `base` is (the name warnings use, the preset).
  pub fn resolve_preset_output(choice: &OutputChoice, base: Option<(&str, &PresetOutput)>, output: &Path,
      registry: &MediaRegistry, describe: &mut dyn FnMut(usize) -> Arc<PluginDescription>,
  ) -> Result<ResolvedOutput, ExportError>;
  impl ExportSession {   // pub(crate)
      pub(crate) fn resolve(&mut self, choice: &OutputChoice, presets: &PresetLookup, output: &Path, frame: (u32, u32))
          -> Result<(OutputPlan, Vec<String>), ExportError>;
  }
  ```
- `resolve_output`: `choice.preset` → `presets.find(name)?` → `None` is
  `UnknownPreset { name, default_in: None }`; without a preset and with no
  component given (Decisions), `presets.project_default()?` → its name
  found the same way, or `UnknownPreset { name, default_in: Some(project path) }`.
  The found preset's `warnings` come first in `ResolvedOutput.warnings`.
  Then `resolve_preset_output`.
- `resolve_preset_output`, per step, over P2a's code:
  - Muxer: `choice.muxer`, else the base's `muxer` parsed and found as a
    muxer (`named`), else the default by extension. A base muxer that does
    not parse, is not installed, is not a muxer, or is described
    unavailable falls back to the default with
    `preset `{name}`: {reference} is unavailable ({reason}); using the default instead`
    (an unparsable reference's reason is `not a component reference`). The
    extension warning (Decisions) follows for a named or preset muxer.
  - Video / audio encoder: `choice.*.component`, else the base's component
    (same fallback and warning, including `IncompatibleComponents` with the
    chosen muxer, whose `reason` is the warning's reason), else P2a's
    default. Audio: `AudioChoice::None` → none; `Encode` without component
    and a base `PresetAudio::None` → none (and the audio-options warning
    when options were given).
  - Options: when the base's options apply (Decisions), `sanitize` them,
    warning for each dropped key —
    `preset `{name}`: {component} has no setting `{key}`; it is left out`
    for an unknown key, else
    `preset `{name}`: {component} {key} {message}; using its default`
    — then `layer(schema, kept, explicit)`; otherwise P2a's `validate` of
    the explicit options. Explicit failures stay `InvalidOption`.
- `ExportSession::resolve` returns `resolved.warnings` beside the plan;
  `exporter.rs` and `react_entry.rs` send each as
  `ExportProgress::Warning` before `ExportProgress::Output`.

- [ ] **Step 1: Write failing tests** in `settings.rs`, with P2a's x264 schema fixture:
  - `sanitize_drops_unknown_keys_and_bad_values`: `{crf: 99, tune: "film", speed: 3}` keeps `tune`, drops `crf` (`InvalidSetting` key `crf`) and `speed`.
  - `layer_drops_base_values_the_layer_hides`: base `{crf: 20}`, over `{rateControl: "vbr"}` → `{rateControl: "vbr", bitrate: 8000000, …}` without `crf`; over `{crf: 99}` is `Err` naming `crf`.
- [ ] **Step 2: Write failing tests** in `resolve.rs`, over P2a's fixtures (`bundled()` `b`, `user()` `u`, `project()` `p`) and a helper writing a `PresetsFile` into a `tempfile::tempdir()` and returning its `PresetLookup`:
  - `a_named_preset_is_the_base`: preset `web` = `{muxer: "b/mp4", video: {component: "b/x264", options: {crf: 20, preset: "slow"}}, audio: "none"}`; `resolve` of `{preset: "web"}` for `film.mov` → muxer `b/mp4`, video settings `crf 20`, `preset slow`, no audio.
  - `explicit_choices_layer_over_the_preset`: the same with `--video-option crf=22` (`SettingInput::Text`) → `crf 22`, `preset slow`; with `video.component = u/h264` → `u/h264` with its own defaults and no warning.
  - `stale_preset_values_warn_and_reset`: preset options `{crf: 99, speed: 1}` → `crf 18` (default) and two warnings, one containing `crf`, one `has no setting `speed``.
  - `explicit_values_stay_strict_over_a_preset`: preset `web` with `--video-option crf=99` is `InvalidOption { key: "crf" }`.
  - `a_gone_preset_component_falls_back`: preset video `b/gone` → `b/x264` and a warning containing `b/gone is unavailable (not installed)`; preset video `nonsense` → reason `not a component reference`; a preset video described unavailable falls back with that reason; the preset's options are not applied to the fallback.
  - `a_preset_muxer_against_another_extension_warns`: `{preset: "web"}` for `film.mov` warns `b/mp4 does not write .mov files; the file is written as MP4 anyway`; for `film.mp4` no warning.
  - `the_project_default_applies_without_components`: project file `default: "web"`; `OutputChoice::default()` → `web`'s settings; with only `--video-option crf=22` → `web` with `crf 22`; with `--audio-encoder none` (`AudioChoice::None`) → no preset (x264 defaults).
  - `a_missing_default_is_unknown_preset`: `default: "gone"` → `UnknownPreset { name: "gone", default_in: Some(project path) }`.
  - `an_unknown_name_is_still_unknown_preset`: `{preset: "ultrafast"}` with an empty lookup → `UnknownPreset { name: "ultrafast", default_in: None }` (P2a's hint test keeps passing).
  - `the_shadow_warning_reaches_the_resolved_output`: `x` in project and user files → its warning is `warnings[0]`.
  - `audio_options_without_audio_are_ignored`: preset audio `"none"` and `--audio-option bitrate=1` → no audio and the audio-options warning.
- [ ] **Step 3: Write a failing test** in `encode.rs`: `resolve_warnings_reach_the_caller`: a session over the test codec, a preset with an unknown key for `celesta-test-codec/video`, `resolve` returns one warning.
- [ ] **Step 4:** `cargo test -p celesta-media settings` and `cargo test -p celesta-exporter resolve` fail.
- [ ] **Step 5: Implement**, updating every caller (P2a's tests pass `&PresetLookup::default()`).
- [ ] **Step 6:** `cargo build -p celesta-codec-test && cargo test --workspace` and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.
- [ ] **Step 7: Commit:** `feat(exporter): presets in resolve_output`.

### Task 3: `--preset`, `--preset-file`, MCP, and TypeScript

**Files:**
- Modify: `crates/exporter/src/main.rs` (`job` sets `options.presets = PresetLookup::for_source(cli.source(), cli.preset_file.clone())` for video outputs; `read_preset_file` → `PresetsFile::load` as the usage check; help texts), `crates/mcp/src/args.rs` (the same for `export_video`; `check_preset_file` → `PresetsFile::load`; the `preset` / `presetFile` field docs), `crates/mcp/src/docs.rs` if it documents `preset`, `packages/exporter/src/types.ts` (the `preset` / `presetFile` docs), `packages/exporter/README.md`
- Test: `crates/exporter/src/main.rs` tests, `crates/exporter/tests/integration/cli_formats.rs`, `crates/mcp/src/args.rs` tests, `crates/mcp/tests/integration/export.rs`

**Interfaces:**
- Consumes: Task 1's `PresetsFile::load`, `PresetLookup::for_source`; Task 2's `ExportOptions.presets`.
- Doc copy (exact):
  - CLI `--preset`: `An export preset: looked up in --preset-file, then the project's export-presets.json, then yours.`
  - CLI `--preset-file`: `An export-presets.json to look --preset up in first.`
  - MCP `preset`: `An export preset's name, looked up in presetFile, then the project's export-presets.json, then the user's. Without it, the project's default preset applies when no component is chosen.`
- Stills and subtitle-only jobs keep `PresetLookup::default()` (presets name video components); `--preset` with stills stays P2a's usage error.

- [ ] **Step 1: Write failing tests:**
  - `cli_formats.rs` `a_project_preset_exports`: a temp project folder with `celesta.config.json`, a copy of `examples/minimal.celesta.json`, and `export-presets.json` holding preset `fast` for the test codec's muxer and video encoder with one setting; `--json --preset fast` with an output whose extension the test codec's muxer lists (`crates/codec-test/src/plugin.rs`), the test codec's plugin folder as `--plugin-dir`, and `HOME`/`XDG_CONFIG_HOME`/`APPDATA` at an empty temp dir succeeds and the report's `output.video.component` is the test codec's.
  - `cli_formats.rs` `a_user_preset_is_found_and_a_stale_key_warns`: the preset in `$HOME/Library/Application Support/Celesta/export-presets.json` (macOS) / `$XDG_CONFIG_HOME/celesta/…` with an unknown key: success, and `warnings` contains `has no setting`.
  - `cli_formats.rs` `the_project_default_applies`: project file with `default`; no `--preset`; the report shows the default's encoder setting in `--json-progress`'s `output` event settings.
  - `cli_formats.rs` `a_broken_preset_file_is_a_usage_error`: `--preset-file` with `{"version": 2}` exits 2 with a message containing `unsupported version 2`.
  - `cli_formats.rs`: P2a's `--preset ultrafast` test keeps passing (no files → `unknown_preset` with the hint).
  - `args.rs` `a_preset_file_must_be_a_presets_file` replaces `a_preset_file_must_be_a_json_object`: `{}` (no `version`) and `{"version": 2}` are errors; a valid file passes and `job()`'s `options.presets.file` is its absolute path and `project` is the source's project file.
  - `mcp/tests/integration/export.rs` `export_video_uses_a_project_preset`: the same project fixture through the server's `export_video` with `preset`.
- [ ] **Step 2:** The tests fail (`--preset fast` is `unknown_preset`).
- [ ] **Step 3: Implement.** In `packages/exporter`, only docs change: `exportVideo` already passes `--preset` and `--preset-file` (`args.test.mts` covers them); run its tests unchanged.
- [ ] **Step 4:** `cargo test -p celesta-exporter -p celesta-mcp`, their clippy, and `pnpm --dir packages/exporter test` pass.
- [ ] **Step 5: Commit:** `feat(exporter): --preset and the project's default preset`.

### Task 4: The export dialog's model (`crates/editor`)

**Files:**
- Create: `crates/editor/src/export_dialog/mod.rs`, `crates/editor/src/export_dialog/model.rs`
- Modify: `crates/editor/src/main.rs` (`mod export_dialog;`), `crates/editor/Cargo.toml` (dev-dependency `celesta-codec-test.workspace = true`)
- Test: `crates/editor/src/export_dialog/model.rs` tests

**Interfaces:**
- Consumes: `celesta_exporter::{PresetsFile, Preset, PresetOutput, PresetEncoder, PresetAudio, PresetSource, ResolvedOutput, OutputChoice, EncoderChoice, AudioChoice, SettingInput}`; `celesta_media::{MediaRegistry, MediaComponentInfo, PluginDescription, ComponentRef, SettingField, SettingValue, Settings, settings::{validate, is_visible, differences}}`.
- Produces (all `pub(crate)`):
  ```rust
  pub enum Track { Video, Audio }
  pub enum Described { Checking, Done(Arc<PluginDescription>) }
  pub enum OptionState { Available, Checking, Unavailable(String) }
  pub struct ComponentOption { pub reference: ComponentRef, pub label: String, pub state: OptionState }
  pub struct EncoderPick { pub component: ComponentRef, pub settings: Settings }   // values given, not defaults-filled
  pub struct DialogChoice { pub muxer: ComponentRef, pub video: EncoderPick, pub audio: Option<EncoderPick> }
  pub struct PresetFileState { pub path: Option<PathBuf>, pub file: Result<Option<PresetsFile>, String> }
  pub struct FieldRow { pub field: SettingField, pub value: SettingValue, pub error: Option<String> }
  pub struct FieldGroup { pub group: Option<String>, pub rows: Vec<FieldRow> }   // visible rows only
  pub enum SaveTarget { Project, User }

  pub struct ExportDialogModel { /* components, described: HashMap<usize, Described>, choice: Option<DialogChoice>,
                                    project: PresetFileState, user: PresetFileState,
                                    selected: Option<(PresetSource, String)>, warnings: Vec<String>, error: Option<String> */ }
  impl ExportDialogModel {
      pub fn new(registry: &MediaRegistry, project: PresetFileState, user: PresetFileState) -> Self;
      pub fn described(&mut self, plugin: usize, description: Arc<PluginDescription>);
      /// What the dialog first resolves: the project's default preset, else `last`, else nothing (defaults).
      pub fn initial_base(&self) -> Option<(String, PresetOutput)>;
      /// The muxer to resolve defaults against: the default for `.mp4`, else the first ranked muxer.
      pub fn initial_output(&self) -> PathBuf;   // "export.<extension>"
      pub fn apply(&mut self, resolved: &ResolvedOutput, selected: Option<(PresetSource, String)>);  // sets choice, warnings
      pub fn muxer_options(&self) -> Vec<ComponentOption>;
      pub fn encoder_options(&self, track: Track) -> Vec<ComponentOption>;   // writing a codec the chosen muxer takes; empty audio for a muxer without audio
      pub fn choose_encoder(&mut self, track: Track, component: Option<ComponentRef>);  // None: audio off; settings reset; clears `selected`
      pub fn fields(&self, track: Track, advanced: bool) -> Vec<FieldGroup>;   // from the chosen component's described schema, `visibleWhen` on the effective values
      pub fn set_setting(&mut self, track: Track, key: &str, value: SettingValue);   // clears `selected`
      pub fn presets(&self) -> (Vec<&Preset>, Vec<&Preset>);   // (project, user)
      pub fn output_choice(&self) -> Result<OutputChoice, String>;   // explicit components and `SettingInput::Value`s; Err when a field is invalid or a component is unavailable or still checking
      pub fn extension(&self) -> Option<String>;   // the chosen muxer's first extension
      pub fn saved_output(&self) -> PresetOutput;   // muxer, components, `differences` of each
      pub fn can_save(&self, target: SaveTarget) -> Result<(), String>;   // "Save the project first" / the file's read error
      pub fn replaces(&self, target: SaveTarget, name: &str) -> bool;
  }
  ```
- Labels as Decisions says; a project plugin's component gets ` (project)`.
- `fields` puts `advanced` fields in their own call (`advanced: true`) so the
  view can collapse them; groups keep schema order, ungrouped fields first.
- A component whose plugin has no description yet is `Checking`, and its
  fields are empty.

- [ ] **Step 1: Write failing tests** in `model.rs`, over a `MediaRegistry` of P2a-style fixtures (a bundled `b` with `mp4`, `ivf`, `x264`, `vp9`, `aac`; a project `p` with `h264`) and hand-made `PluginDescription`s with P2a's x264 schema:
  - `components_show_their_plugin_and_origin`: `muxer_options()` labels are `MP4 · b`, `IVF · b`, `MP4 · p (project)`.
  - `components_are_checking_until_described`: before `described`, every option is `Checking`; after `b`'s description with `vp9` unavailable (`"no libvpx"`), `vp9` is `Unavailable("no libvpx")` and `x264` `Available`.
  - `encoders_follow_the_muxer`: with muxer `b/ivf`, `encoder_options(Video)` is only `vp9`, and `encoder_options(Audio)` is empty.
  - `fields_follow_visible_when_live`: with x264 chosen, the `Rate control` group shows `rateControl` and `crf`; after `set_setting(Video, "rateControl", "vbr")`, it shows `bitrate`, not `crf`; `tune` appears only with `advanced: true`.
  - `an_invalid_field_blocks_export`: `set_setting(Video, "crf", 99)` → that row's `error` is `Some`, and `output_choice()` is `Err`.
  - `output_choice_is_explicit`: after `apply` of a resolved output and one `set_setting`, `output_choice()` has `preset: None`, the muxer and both components `Some`, and the options as `SettingInput::Value`.
  - `editing_after_a_preset_clears_the_selection`: `apply(…, Some((Project, "web")))` then `set_setting` → `selected` is `None`, `warnings` kept.
  - `the_initial_base_prefers_the_project_default`: project `default: "web"` and user `last` → `web`; without the default → `last`; neither → `None`.
  - `saved_output_keeps_only_changes`: x264 with `crf 18` (default) and `preset slow` → options `{preset: "slow"}`.
  - `a_broken_presets_file_is_shown_and_never_written`: user `PresetFileState { file: Err("…: not JSON") }` → `can_save(User)` is `Err` with that message; `presets().1` is empty.
  - `an_untitled_project_cannot_take_a_preset`: project `path: None` → `can_save(Project) == Err("Save the project first")`.
  - `saving_an_existing_name_is_a_replace`: `replaces(User, " web ")` is true when the user file has `web`.
- [ ] **Step 2:** `cargo test -p celesta-editor export_dialog` fails.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** `cargo test -p celesta-editor` and `cargo clippy -p celesta-editor --all-targets -- -D warnings` pass.
- [ ] **Step 5: Commit:** `feat(editor): the export dialog's model`.

### Task 5: The export dialog (`crates/editor`)

Load the `gpui-kit` and `gpui-kit-design-guides` skills and read the Design
Guides in full before this task; check every gpui-kit API used against the
0.9 source in `~/.cargo/registry/src/*/gpui-component-0.9.0/`.

**Files:**
- Create: `crates/editor/src/export_dialog/view.rs`
- Modify: `crates/editor/src/export_dialog/mod.rs`, `crates/editor/src/export_control.rs` (`request_export` opens the dialog; the save panel follows its `Export`), `crates/editor/src/export_worker.rs` (`ExportRequest.output_choice: OutputChoice`, `ExportRequest.presets` not needed — the choice is explicit), `crates/editor/src/helpers.rs` (`export_suggested_name(path, project_name, extension)`, `export_output(path, extension)` keeps the muxer's extensions instead of `EXPORT_EXTENSIONS`), `crates/editor/src/panels.rs` (remove the title bar's loudness button), `crates/editor/src/view.rs` (`export_dialog: Option<Entity<ExportDialog>>`, dropped on Reload Plugins), `crates/editor/src/plugins.rs` (drop the dialog with the registry), `crates/editor/Cargo.toml` (dev-dependency `gpui-kit = { version = "0.9", features = ["test-support"] }`)
- Test: `crates/editor/src/export_dialog/view.rs` tests, `crates/editor/src/tests.rs`

**Interfaces:**
- Consumes: Task 4's `ExportDialogModel`; Task 2's `resolve_preset_output`; Task 1's `PresetLookup`, `PresetsFile::load_if_present`, `update_presets`, `user_presets_path`; `celesta_plugin_host::{project_root, PluginKillSwitch}`.
- Produces:
  ```rust
  pub(crate) struct ExportDialog { model: ExportDialogModel, registry: Arc<MediaRegistry>, kill: PluginKillSwitch, /* controls, subscriptions, tasks */ }
  pub(crate) enum ExportDialogEvent { Export { choice: OutputChoice, extension: String, last: PresetOutput } }
  impl EventEmitter<ExportDialogEvent> for ExportDialog {}
  impl ExportDialog {
      pub(crate) fn new(registry: Arc<MediaRegistry>, project_file: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self;
  }
  ```
- **Order, top to bottom** (the spec's): Preset; Container; Video encoder and
  its fields (grouped under their group labels; advanced fields in a
  `Collapsible` titled "Advanced", closed); Audio encoder (with "None") and
  its fields; Range and Loudness; then the footer: `Save as preset…`
  (outline, leading), `Cancel` (outline), `Export…` (primary, the Enter
  default, disabled while `output_choice()` is `Err`, with the reason under
  it). The body scrolls; title and footer stay.
- Controls: `select` → `Select`; `number` → `Input` read with
  `parse_number_input`, the field's `unit` after it, range errors under it;
  `boolean` → `Checkbox` (a form committed by Export…, not an immediate
  setting); `string` → `Input`. Field `description` is muted text under the
  control. Warnings and errors use the theme's warning/danger foreground with
  a lucide `IconName::TriangleAlert`, never color alone.
- Async: on `new`, one `cx.background_spawn` per media plugin with encoders
  or muxers calls `registry.describe(plugin, &kill)`; each answer updates the
  model and re-renders. The first resolve (`initial_base`, `initial_output`)
  and each preset or container change run `resolve_preset_output` on the
  background executor with `describe = |p| registry.describe(p, &kill)`;
  a failure is shown under the section that changed, keeping the previous
  choice. Dropping the dialog kills `kill`.
- Export flow: `Export…` emits `Export`; `EditorView` closes the dialog, saves
  `last` with `update_presets(user path, |f| f.last = Some(last))` (a failure
  is a problem in the Problems panel, not a blocked export), and opens the
  save panel in the document's folder with
  `export_suggested_name(path, name, &extension)`; the chosen path goes
  through `export_output(path, muxer extensions)` and `start_export(output,
  choice)`. `Cancel`/Escape closes the dialog and keeps its state.
- Save as preset: the inline section's `Save`/`Replace` calls
  `update_presets(target path, |f| f.put(Preset { name: trimmed, output: model.saved_output() }))`,
  reloads that file into the model, and selects the saved preset.

- [ ] **Step 1: Write failing tests:**
  - `tests.rs` `export_options_take_the_dialogs_choice`: `export_options` of a request whose `output_choice` names `b/mp4` passes it through unchanged (replacing P2a's `OutputChoice::default()` expectation).
  - `tests.rs` `the_suggested_name_takes_the_muxers_extension`: `export_suggested_name(Some("a/film.celesta.json"), "x", "mov") == "film.mov"`; `export_output("a/film", &["mov"])` is `a/film.mov`, `export_output("a/film.MOV", &["mov"])` keeps it.
  - `view.rs` `#[gpui_kit::test] export_dialog_emits_the_chosen_output`: a dialog over `MediaRegistry::new(vec![celesta_codec_test::registered()])` in a test window; run until parked; the container `Select` shows the test codec's muxer; set one video field through the model's control; dispatch the `Export…` click; the emitted `Export` names the test codec's muxer and encoder with that option.
- [ ] **Step 2:** The tests fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** `cargo test -p celesta-editor`, `cargo clippy -p celesta-editor --all-targets -- -D warnings`, and `cargo fmt --all -- --check` pass.
- [ ] **Step 5: Check in the app.** Stage the FFmpeg plugin (`node scripts/plugins/build.mts` as HANDOFF.md says) on `CELESTA_PLUGIN_PATH`, run `cargo run -p celesta-editor -- examples/greenscreen/composite.tsx`, open Export…, and take window screenshots (`screencapture -l <window id>`) into the session's scratchpad: the dialog after `describe` (MP4, x264 fields), with Advanced open, with a preset applied that has a warning, and the inline "Save as preset…". If the session cannot click in the app, a local, uncommitted patch may open the dialog on start (as `docs/handoff/2026-11-07-editor-cleanups.md` did); say so in the handoff. Export once from the dialog and check the file plays and `last` was written.
- [ ] **Step 6: Commit:** `feat(editor): the export dialog`.

### Task 6: Docs, the spec's deviations, and full verification

**Files:**
- Modify: `packages/website/content/en/docs/02-(guides)/09-export.md` and its `ja` twin (presets: the file, where it lives, the project `default`, lookup order, stale presets; the editor's export dialog), `packages/website/content/en/docs/05-(more)/04-troubleshooting.md` and `ja` (`invalid_presets`, `unknown_preset`), `skills/celesta/references/export.md` and `errors.md` (replace "Presets are not available yet"; `--preset`, `--preset-file`, the project `default`, MCP's `preset`), `skills/celesta/SKILL.md` if it lists export flags, `HANDOFF.md` (the exporter row: presets; the editor row: the export dialog), `docs/superpowers/specs/2026-11-08-media-components-design.md` (deviations 40–43 under Presets, Resolving an output, and Exporter API; the Phases row for P3 "As built"), `crates/exporter/src/{resolve.rs,options.rs}` doc comments still saying "presets arrive with P3"
- Create: `docs/handoff/2026-11-09-export-presets-dialog.md` (specification, deviations, decisions, docs, checks, not done)
- Test: `pnpm --dir packages/website test`

- [ ] **Step 1:** `grep -rn "not available yet\|arrive with P3\|until P3" crates packages skills docs/handoff HANDOFF.md` finds only historical handoff notes.
- [ ] **Step 2:** Examples: check that no example's README or script is superseded by presets (P2a moved them to `--video-option`, which stays valid); if one documents a repeated set of flags, give it an `export-presets.json` and use `--preset` (AGENTS.md).
- [ ] **Step 3: Full verification:** `cargo build -p celesta-codec-test` then `cargo test --workspace`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `cargo fmt --all -- --check`; `cargo test --manifest-path plugins/Cargo.toml --workspace --locked` and its clippy with `-D warnings`; `pnpm run test:runtime`; `pnpm --dir packages/exporter test`; `pnpm --dir packages/website test`; `git diff --check`. Record the counts in the handoff note.
- [ ] **Step 4: Commit:** `docs: export presets and the export dialog`.
