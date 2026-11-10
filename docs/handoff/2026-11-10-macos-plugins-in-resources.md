# macOS package: bundled plugins in `Contents/Resources/PlugIns`

Both Package jobs had failed on every push to `main` since 2026-11-06. This
fixes the two causes and lets a manual run of CI build the packages on a
branch.

## Specification

- **Where the macOS app keeps its plugins.** The bundled plugin folder of the
  macOS app is now `Celesta.app/Contents/Resources/PlugIns` (the codec plugin
  in `Contents/Resources/PlugIns/ffmpeg`), not `Contents/PlugIns`. Windows
  (`plugins` beside the executable), other Unixes
  (`../lib/celesta/plugins`), `CELESTA_SYSTEM_PLUGIN_DIR`, and a macOS build
  outside an app bundle are unchanged.
- **Why.** `codesign` takes everything in `Contents/PlugIns` for nested code.
  A Celesta plugin folder also holds `celesta-plugin.json`, `licenses/`, and
  an optional `js/`, so signing the app's main executable failed with `code
  object is not signed at all`, naming
  `Contents/PlugIns/ffmpeg/licenses/LICENSE-APACHE`. In `Contents/Resources`,
  those files are sealed as the app's resources, and the plugin's program and
  `Frameworks/` libraries are signed one by one with the app's other Mach-O
  files (the packaging script already signed every Mach-O file it found).
- **Nothing to migrate.** The bundled folder is found from the running
  executable each time, and bundled plugins are never asked about, so no
  setting names its path.
- **Root workspace metadata is read online.** The release build downloads
  only the crates it compiles, so `cargo metadata --offline` on the root
  workspace stopped at `lz4_flex`, which `celesta-codec-test` uses only in
  tests. Both `package-macos.py` and `package-windows.ps1` now read it without
  `--offline`, as they already read the plugins workspace.
- **CI builds the packages on a manual run.** `ci.yml`'s Package job runs on
  `github.event_name != 'pull_request'`: pushes to `main` as before, and
  `workflow_dispatch` on any branch (unsigned, `release: false`, nothing
  published). The release-profile cache is still saved from `main` alone.
  Pull requests still skip it. `package.yml`, the release workflow, is
  unchanged.
- The MCP tool list in `scripts/test-package.mjs` matches `main`'s server
  (no `list_components` / `describe_component` yet), so it is unchanged here;
  the media-components stack updates it when it lands.

## Implementation

| Where | What |
| --- | --- |
| `crates/plugin-host/src/locations.rs` | `bundled_plugin_dir_for("macos", …/Contents/MacOS/x)` returns `Contents/Resources/PlugIns`; its test expects that. |
| `scripts/package-macos.py` | The plugin build script lays plugins out in `resources / "PlugIns"`. `validate_dependencies` takes a `.dylib` as a plugin library when it is in `Contents/Resources/PlugIns/<plugin>/Frameworks` (the full path, not just a parent named `PlugIns`). Root `cargo metadata` without `--offline`. |
| `scripts/package-windows.ps1` | Root `cargo metadata` without `--offline`. |
| `scripts/test_package_macos.py`, `scripts/test-package.mjs` | The codec plugin's macOS path. |
| `.github/workflows/ci.yml`, `package-build.yml` | Package job on a manual run; header comment. |
| Docs | `README.md`, `packaging/GPL-SOURCE-OFFER.md`, `packaging/macos/README.md` (layout tree, and why not `Contents/PlugIns`), and the en/ja website pages `05-(more)/04-troubleshooting.md` and `05-plugins.md`. The Celesta Agent Skill does not name the macOS path, so it is unchanged. Older handoff notes keep the path they described. |

## Checks

- A minimal bundle (`MacOS/Celesta` and one plugin folder with an unsigned
  program, `celesta-plugin.json`, and `licenses/LICENSE-APACHE`):
  under `Contents/PlugIns`, `codesign --force --sign - Contents/MacOS/Celesta`
  fails with `code object is not signed at all` in subcomponent
  `celesta-plugin.json`; under `Contents/Resources/PlugIns`, signing the main
  executable, the plugin program, and the app in the script's order succeeds,
  and `codesign --verify --deep --strict` accepts the app.
- `cargo test -p celesta-plugin-host --lib locations`
- `python3 -m unittest scripts/test_package_macos.py`
- A manual run of CI on this branch (`gh workflow run ci.yml --ref
  claude/macos-plugins-resources`): see the pull request for its Package jobs.
