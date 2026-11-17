# Plugin assets reach the plugin as absolute paths

## Bug

Celesta 0.10.0: `celesta-exporter --react film.tsx out.mp4`, run from the
entry's own folder, failed on any content plugin layer whose asset path is
relative:

```text
could not draw plugin layer …: plugin "flag": could not read ../assets/…/komugi-normal.png: No such file or directory
```

`./film.tsx`, `delta/film.tsx` from the parent folder, and an absolute path
all worked.

Two things combined:

- `Path::new("film.tsx").parent()` is `Some("")`, not `None`, so the
  `unwrap_or_else(|| Path::new("."))` fallbacks in the exporter never fired.
  `fs::canonicalize("")` fails, and the React export's asset root stayed the
  empty path. Relative asset paths stayed relative.
- The plugin host sent the paths to the plugin as they came. A plugin process
  runs in its own folder (`PluginChild` sets `current_dir`), so a relative
  path there names another file, or none. Celesta itself opens relative
  paths against its own working directory, so the renderer's own images,
  video, and fonts still loaded. Only plugin layers broke.

A project export had the same problem with any relative project path
(`celesta-exporter project/film.celesta …`): `export_project_*` passes the
project's folder to the renderer without making it absolute.

## Fix

- `celesta-exporter`: `project::folder_of(path)` is the folder a path's
  relative assets resolve from, `.` for a bare file name. The React entry,
  React stills, `export_file*`, and `ExportJob`'s project roots use it, so
  the existing `fs::canonicalize` gets a folder it can resolve.
- `celesta-plugin-host`: `PluginHost::open_remote` makes each asset path
  absolute (`std::path::absolute`, against Celesta's working directory)
  before it sends `open`. This is the opposite direction of the existing
  rule that a plugin's texture paths must be absolute. It covers every
  caller that hands the renderer a relative asset root: project exports,
  `celesta-mcp`'s inspection, and embedders.

## Tests

- `crates/exporter/tests/integration/export.rs`,
  `a_bare_entry_name_sends_a_plugin_absolute_asset_paths`: runs the
  `celesta-exporter` binary from a temporary entry folder with
  `--react film.tsx film.mp4` and a fake content plugin, `probe`, that
  records the requests it gets. The `open` request's `image` asset must be
  an absolute path to the entry's `probe.png`. Needs `packages/cli` built and
  a GPU; skipped otherwise, like the other React export tests.
- `crates/plugin-host/src/host.rs`,
  `a_relative_asset_path_is_sent_to_the_plugin_absolute`: opens a model
  with a relative asset and checks the `open` request the plugin got.

## Checks

```sh
export PKG_CONFIG_PATH=/opt/homebrew/opt/ffmpeg@8/lib/pkgconfig  # macOS
cargo build -p celesta-codec-test
CELESTA_REQUIRE_GPU=1 cargo test -p celesta-exporter --test integration a_bare_entry_name
cargo test -p celesta-plugin-host a_relative_asset_path
```

End to end, with the flag sample plugin:

```sh
node scripts/plugins/build.mts examples/plugins/flag plugins/ffmpeg
cd examples/plugins/flag
../../../target/debug/celesta-exporter --react \
  --plugin-dir ../dist --plugin-dir ../../../plugins/dist demo.tsx out.mp4
```

With the fix, this export completes (checked on macOS, 2026-11-17). The
failure itself was seen exporting `examples/delta/film-0.10.0.tsx` the same
way; the two tests above fail without the fix.
The tests above need `packages/react` and `packages/cli` built from the same
checkout (`pnpm run build:runtime`): an older `dist` without `PluginLayer`
fails the exporter test while it loads the entry.

## Follow-up

The DELTA 0.10.0 release film's PR exports with `./film-0.10.0.tsx` and
documents this bug in `examples/delta/README.md` and its handoff note. Once
both land, that workaround note can go.
