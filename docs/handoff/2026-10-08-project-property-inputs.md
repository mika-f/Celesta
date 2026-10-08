# Project property inputs from the command line (2026-10-08)

Issue #145. A React entry can be rendered with different titles, colors, and
data files without editing its source. The inputs reuse the existing project
property concept: `defineProjectProperties()` declares them, and
`useProjectProperty()` reads them. No second API was added for the same idea.

## Interface

- `celesta-exporter --react … --props-file <file.json> --props '<json>'`, and
  the same two options on `celesta-editor <entry.tsx>`. Both need a React
  entry. Each is a JSON object of values; `--props` wins over the file.
- `skills/celesta/scripts/inspect.mjs` accepts the same options.
- New schema field type `path` (a local file or an `http(s)` URL), shared
  with `registerComponent` schemas. Display-only there.
- `getProjectProperty(key, defaultValue?)` is a non-hook read for
  `prepare()` and for values computed outside components, such as
  `durationInFrames`. `useProjectProperty(key, defaultValue?)` now works
  without a `<ProjectProvider>`, and its `defaultValue` is optional for
  declared keys.

## Precedence

Highest first:

1. `--props`
2. `--props-file`
3. the `--project` companion file's `properties`
4. the nearest `<ProjectProvider>` (only `useProjectProperty`)
5. the declared `defaultValue`
6. the hook's or function's `defaultValue` argument

`getProjectProperty` cannot see a Provider, because the Provider exists only
inside the tree. `prepare()` and rendering agree whenever the values come
from levels 1–3 or 5.

## Validation

`packages/react/src/property-inputs.ts` (Node only; the browser runtime does
not import it) checks the values after the entry's module scope has run (the
schema exists only then) and before `prepare()`. It reports every problem in
one `{ "invalidProperties": [{ key, source, message }] }` line in place of
`Ready`.

- `--props`/`--props-file` are strict. A key must be declared, and any value
  is an error when the entry calls no `defineProjectProperties()`.
- A companion project's undeclared keys are ignored, so projects with
  `properties` meant for a Provider keep exporting. Its declared keys are
  checked.
- `string`/`number` (finite)/`boolean` check the JSON type. `color` must be
  `#RRGGBB`/`#RRGGBBAA`, the same rule as `CpuColor::from_hex`. `select`
  must be one of `options`. `path` must be an existing regular file, an
  `http(s)` URL (kept as is), or `""` (no file, so a variant can turn off a
  file the default supplies). Declared defaults are resolved but not
  checked: a variant or a Provider may supply the file instead.
- `min`/`max`/`step` stay Inspector hints and are not enforced (decided with
  the user). There is no `required` flag or JSON-object type; both can be
  added to the same validator later.
- `path` values become absolute. Relative paths resolve from the
  `--props-file` folder; for `--props`, from the current directory; for a
  companion project, from its asset root; for a declared default, from the
  entry's folder. An empty default stays `""`. Provider values are returned
  unchanged.
- Validated values and defaults are prototype-free objects, so a key such
  as `__proto__` is an ordinary property.
- A malformed handshake line (layers without object `values`, string
  `baseDir`, or a known `source`) fails as an `error` line, not a crash.
- Reading a property at module scope throws: values are held back
  (`holdProjectPropertyValues`) until validation completes.

The Rust side surfaces this as `ReactBridgeError::InvalidProperties`. The
exporter's `--json` reports it with the code `invalid_properties`, one
`issues` entry per key (`path` is the key). An entry without a schema yields
one issue per strict source with an empty key; its `issues` entry has no
`path`. The bridge kills the Node process before returning either startup
error. An unreadable or non-object
`--props`/`--props-file` is `ExportError::Properties`, with the same code.

## Plumbing

- `celesta_react_bridge::PropertyInputs` holds layers (`PropertySource`:
  `project`, `propsFile`, `props`, each with `baseDir` and an optional
  `file`), lowest precedence first. `PropertyInputs::load(file, inline)` and
  `with_project(...)` build them. `ReactBridge::spawn_with_properties` passes
  `--properties-stdin` and writes `{ "propertyInputs": [...] }` as the first
  stdin line. Plain `spawn` sends nothing. The CLI reads stdin only with that
  flag, so an older bridge or a direct `node cli.js entry` is unaffected.
- `ReactRuntimeOptions` gained `properties` / `with_properties`. Every
  exporter spawn goes through `ReactRuntimeOptions::spawn`, which adds the
  companion project's layer.
- Editor: `PropertyArgs` (the command-line options, made absolute) lives on
  `ReactPreview`. It is re-read on every reload, and its loaded
  `PropertyInputs` reach the preview bridge (`ReactPreviewContext`, so
  changed values respawn Node), the audio sweep, and the export worker. The
  reload watcher includes the `--props-file` mtime, since the file may live
  outside the entry's folder; it is sampled before the props file is read,
  so an edit while Node starts triggers another reload. A reload that
  finishes after another file was opened is discarded (session check).
  File > Open of another file drops the options; reopening the same entry
  keeps them. A JSON project's preview passes its own `properties` as the
  companion layer, so its components see the same values as
  `--react … --project …` exports.
  The React Preview panel shows which sources are active. A JSON project
  opened with these options is an error.

## Example

`examples/ranking/` is a ranking-card template with `variants/spring.json`
and `variants/autumn.json`. Each sets the title, subtitle, accent, theme,
and a data file. `prepare()` reads the data file through
`getProjectProperty('data')`, and the duration follows the row count (153,
169, and 177 frames for the default, spring, and autumn data).

## Validation run

- `pnpm -C packages/react test`: 30 files / 146 tests pass, including the new
  `test/project-properties.test.mjs`.
- `cargo test --workspace`: 388 tests pass, including
  `passes_project_property_inputs_over_the_provider_when_node_is_available`,
  `react_cli_props_win_over_the_companion_project_and_are_checked_first`, the
  bridge unit tests for loading, layer order, and the `invalidProperties`
  message, the `invalid_properties` report test, and the editor CLI parse
  test.
- `git diff --check` passes. With this machine's Rust 1.99 toolchain,
  `cargo fmt --all -- --check` reports diffs only in five unchanged files
  (`composition/src/animation.rs`, `exporter/tests/export_integration.rs`,
  `media/build.rs`, `media/tests/ffmpeg_integration.rs`,
  `renderer/build.rs`), which were left as they are.
  `cargo clippy --workspace --all-targets -- -D warnings` fails on lints in
  unchanged code (`celesta-budoux` `needless_range_loop`; `celesta-renderer`
  `chunks_exact_to_as_chunks` and others). No fmt or clippy finding is in a
  file this change touches.
- A one-second MP4 of the spring variant (`--from 2 --to 3`) exported.
- Exported PNG frames of both variants and of `--props` overrides with the
  real exporter, and checked the images. Exercised the exporter error paths
  (human-readable and `--json`).
- Editor: checked that invalid values and a JSON project with `--props` stop
  before a window opens, and that a valid `--props-file` starts. The preview
  was not checked visually.
