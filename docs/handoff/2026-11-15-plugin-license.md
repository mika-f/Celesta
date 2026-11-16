# Plugin license metadata (2026-11-15)

A plugin can declare the license it is distributed under as an SPDX license
expression. The design, with the approaches considered, is
`docs/superpowers/specs/2026-11-15-plugin-license-design.md`. The registry's
half (the publish check, the database, the API, the site) is in
`mika-f/registry.celesta.studio`.

## Specification

- `plugin.json` and `celesta-plugin.json` take two optional keys: `license`,
  an SPDX license expression for the plugin as distributed, and `licenseRefs`,
  an object from each `LicenseRef-*` identifier in the expression to a `name`
  and an optional `https://` `url`. A license outside the SPDX list
  (proprietary, custom) is declared as a `LicenseRef-*`.
- `plugin.json`'s `licenses` (the license files the build copies) is
  unchanged and separate.
- The registry format stays `1`. A version entry of `index/<id>.json` has
  optional `license` (string or null) and `licenseRefs`; a `search.json` entry
  has optional `license`, that of the plugin's highest published version.
- A version without a license is "not declared". Nothing requires one.
- The registry accepts an expression only in the canonical form the format's
  README gives, so clients show it without parsing.

## Implementation

- `scripts/plugins/manifest.mts`: `checkLicense` checks shape only (characters,
  single spaces, `licenseRefs` keys and entries, and that the keys are exactly
  the `LicenseRef-*` the expression names); `pluginManifest` copies both keys.
  The build takes no runtime dependency, since packaging runs it without
  installed packages. `spdx-expression-parse` is a dev dependency:
  `test/manifest.test.mts` parses the `license` of every `plugins/*/plugin.json`
  and requires each to declare one.
- `plugins/*/plugin.json`: `MIT OR Apache-2.0` for `vrm`, `avfoundation`,
  `mediafoundation`, `aivis-speech`, `code`, `lottie`, and `voicevox`; `(MIT OR Apache-2.0) AND LGPL-2.1-or-later` for `ffmpeg`;
  `(MIT OR Apache-2.0) AND GPL-2.0-or-later` for `ffmpeg-x264`. These name the
  plugin's code and the FFmpeg build it links, whose copyleft terms change
  what a user may do. A permissively licensed dependency is not named
  (`code` bundles twinkleplop, MIT): its text is in the plugin's `licenses/`.
- `crates/registry-format`: `VersionEntry::license`, `VersionEntry::license_refs`
  (`LicenseRef`), `SearchEntry::license`, and `FormatError::InvalidLicense` for
  a value of the wrong shape. The schemas gain the same fields. Fixtures:
  `index-vrm.json` (one version with a license, one without),
  `index-third-party.json` (a `LicenseRef-*`), `search.json` (a license, a
  `LicenseRef-*`, and `null`), and the invalid `index-bad-license-ref.json` and
  `index-empty-license.json`. `fixtures/signed/` is regenerated.
- `crates/cli`: `celesta plugin info` prints `license: <expression>` for the
  highest version after `roles:`, an indented line for each `licenseRefs`
  entry, or `license: not declared`. With no versions it prints no license
  line.
- `crates/plugin-host`'s `Manifest` does not read the keys; it ignores unknown
  ones, so older Celesta versions load a manifest that has them.

## Checks

- `cargo test -p celesta-registry-format -p celesta-registry-client -p celesta-plugin-host -p celesta-cli`
- `cargo clippy -p celesta-registry-format -p celesta-cli --all-targets -- -D warnings`
- `pnpm --dir scripts/plugins test`

## Not done

- The editor does not show an installed plugin's license.
- `plugin.json` still cannot give `description` or `homepage`, which the
  registry reads from the manifest; the build script refuses those keys.
- The registry's `format/PIN` must move to the merged commit of this change.
