# Plugins declare the Celesta versions they work with (2026-11-06)

Issue #717 (with #723, closed as its duplicate). A plugin's only
compatibility field was `protocol`, the wire format, which stays `1` while
features are added on top of it (3D scene content, plugin WGSL materials,
the `codec` role). A plugin's JavaScript module uses `@celesta.studio/*`
APIs that change between releases. Nothing let a plugin say which Celesta it
needs, so one made for another Celesta loaded and failed later.

## Specification

- `celesta-plugin.json` has a required `celesta`: the Celesta versions the
  plugin works with, as a Cargo version requirement (`semver::VersionReq`),
  such as `">=0.9.0, <0.10.0"`.
- `protocol` keeps meaning the wire format alone, which `hello` still
  checks. `celesta` covers everything built on it: features the process
  relies on, and the runtime packages (`react`, `@celesta.studio/*`) the
  module imports. Those packages are not published; entries resolve them to
  the runtime bundled with Celesta at Celesta's version, so the Celesta
  range covers them and the module needs no separate requirement.
- Matching is Cargo's (`VersionReq::matches`): a pre-release Celesta such as
  `0.10.0-beta.1` is in a range only when a comparator names a pre-release
  of `0.10.0` (`">=0.10.0-beta.1, <0.11.0"`).
- Discovery (`read_manifest`) checks, in order: the id, the range (missing
  or unreadable: `DiscoveryError::Invalid`, whose reason suggests the range
  for this Celesta), the rest of the manifest, then whether the range
  includes this Celesta (`CELESTA_VERSION`, the plugin-host crate's version,
  which every Celesta crate shares). A plugin outside its range is
  `DiscoveryError::Incompatible { path, id, range, celesta }`, reported
  before the walk over its files for permissions.
- A missing field is an error, not "any version": the issue asks for it to
  be required of every plugin, bundled ones included, and every plugin
  directory Celesta's packages ship is written by `build.mts`, which now
  writes it. Plugin directories built before this must be rebuilt.
- A skipped plugin takes no id, as before, so a later plugin with the same
  id is found in its place. The codec choice (`InstalledPlugins::codec`)
  takes the first codec among the plugins found, so an incompatible codec
  plugin earlier in search order falls back to the next compatible one.
- The errors reach every place skipped plugins already went: the editor's
  Problems tab and the bottom of the Plugins tab (`FoundPlugins::skipped`),
  and stderr from the exporter, MCP, and the bridge (`find_reporting`).

## Build script

`scripts/plugins/build.mts` writes `celesta` into each manifest from the
Celesta it belongs to: the root Cargo.toml's `[workspace.package]` version
(`workspaceVersion`), which is the version of the Celesta crates the
plugin's process builds with and of the runtime its module is checked
against. `celestaRange` (`manifest.mts`) gives that version, without build
metadata, up to the next release that may break it, as Cargo's `^` reads:
the next minor below 1.0, the next major from it. `celesta_range_for` in
`crates/plugin-host/src/manifest.rs` computes the same, for the message
about a missing range. plugin.json has no `celesta` key; a plugin cannot
widen its range by hand.

The CLI (`packages/cli/src/plugins.ts`) does not check the range: it reads
only the plugin directories the Rust side passes in `CELESTA_PLUGIN_PATH`
and `CELESTA_WAITING_PLUGIN_PATH`, which discovery has already filtered.

## Checks

- `cargo test -p celesta-plugin-host`: `celesta_ranges_reach_the_next_breaking_release`,
  `celesta_ranges_match_as_cargo_does` (bounds, pre-releases, `^`, `*`,
  unreadable), `skips_plugins_without_or_outside_a_celesta_range` (missing,
  unreadable, incompatible, compatible, and the messages), and
  `an_incompatible_codec_leaves_the_next_one`.
- `pnpm --dir scripts/plugins test`: `celestaRange` and the manifests
  `pluginManifest` and `build.mts` write.
- Every test that writes a manifest (plugin-host, codec-test, editor,
  react-bridge, the sample and VRM plugins' process tests) gives a range.
- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  and the plugins workspace's `cargo test`/`cargo clippy` with `--locked`.
