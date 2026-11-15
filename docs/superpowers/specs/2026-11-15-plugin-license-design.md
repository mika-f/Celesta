# Plugin license metadata (SPDX expressions)

Date: 2026-11-15. Spans this repository and `mika-f/registry.celesta.studio`.

## Why

Nothing in the plugin pipeline says what license a plugin is distributed under.
`plugin.json`'s `licenses` only lists the license *files* the build copies into
the plugin directory. Someone deciding whether they may use or redistribute a
plugin has to download the archive and read those files. The registry should
state the license next to the plugin, in a form tools can read.

## Goals

- A plugin declares its license as one SPDX license expression.
- Multiple licenses are the normal case: a choice (`OR`), several that all
  apply (`AND`), and exceptions (`WITH`).
- Licenses outside the SPDX list (proprietary, closed, custom) can be declared.
- The declaration flows from `plugin.json` to `celesta-plugin.json`, the
  registry's database, the signed index and search files, the web API, the
  site, and `celesta plugin info`.
- The registry checks the expression when a version is published.

## Non-goals

- Making the declaration required. A version without one is shown as
  "not declared".
- Checking that the declaration is true (that the archive's license files
  match it).
- A per-component breakdown (which bundled library is under which license).
- Showing the license of installed plugins in the editor.
- Backfilling published versions.

## Approaches

1. **One SPDX expression string** (chosen). `"(MIT OR Apache-2.0) AND
   LGPL-2.1-or-later"`. Standard (a normative annex of the SPDX specification),
   and what npm, crates.io, and PyPI (PEP 639) use. Needs an expression parser
   where it is checked.
2. **A flat array of identifiers.** No parser, but it cannot say whether the
   entries are a choice or all apply. npm and PyPI moved away from this for
   that reason.
3. **A structured array** (`anyOf` inside an all-apply list). Lossless without
   a string parser, but a format of our own that no other tool reads.

The maintainer chose 1 once it was confirmed that maintained parsers exist
(`spdx-expression-parse` for JavaScript, which the npm CLI uses; the `spdx`
crate for Rust, which crates.io uses).

## The fields

In `plugin.json` and `celesta-plugin.json`, both optional:

```json
{
  "license": "GPL-3.0-only OR LicenseRef-Acme-Commercial",
  "licenseRefs": {
    "LicenseRef-Acme-Commercial": {
      "name": "Acme Commercial License",
      "url": "https://acme.example/license"
    }
  }
}
```

- `license`: an SPDX license expression for the plugin as distributed (the
  archive, including what it bundles), not only its source.
- `licenseRefs`: one entry for each `LicenseRef-*` the expression names, with
  a display `name` and an optional https `url`. This is how a license outside
  the SPDX list is declared; a fully closed plugin is
  `"license": "LicenseRef-Proprietary"` with its entry.
- `plugin.json`'s existing `licenses` (the files to copy) is unchanged. The
  two keys follow Cargo's `license` and `license-file`.

### Canonical form

The registry accepts an expression only in this form, so every reader can
show it, or split it on spaces and parentheses, without a parser:

- It parses as an SPDX license expression with identifiers from the SPDX
  license list, spelled in the list's case.
- `AND`, `OR`, and `WITH` are upper case, with a space on each side.
- No deprecated identifier (`GPL-2.0`; write `GPL-2.0-only` or
  `GPL-2.0-or-later`).
- No `DocumentRef-`: there is no SPDX document to refer to.
- No leading or trailing blank, single spaces only and none just inside a
  parenthesis, at most 512 characters.
- The `LicenseRef-*` identifiers in it are exactly the keys of `licenseRefs`.
  `licenseRefs` without `license` is refused.
- A `licenseRefs` entry has a `name` of 1 to 200 characters that is not all
  blanks and has no control or format character, and an optional `url` that
  starts with `https://`. No two `LicenseRef-*` differ only in case.

## Where each part lives

- **Build script** (`scripts/plugins`): accepts the two keys, checks their
  shape (types, key spelling, each key named in the expression), and copies
  them into `celesta-plugin.json`. It does not parse the expression: the
  script runs without installed packages (`node scripts/plugins/build.mts` in
  packaging), so it takes no runtime dependency. A test in `scripts/plugins`
  parses every first-party `plugin.json`'s `license` with
  `spdx-expression-parse` (a dev dependency).
- **Registry format** (`crates/registry-format`): `versions[].license`
  (string or null) and `versions[].licenseRefs` (object) in the index, and
  `license` (string or null) in a search entry. Optional, so this stays format
  1: older clients ignore the fields, and older files parse with them absent.
  The schema and the parsers check shape only (a non-empty string of the
  expression's characters; `LicenseRef-` keys; `name`). The canonical form is
  the registry's publish rule.
- **Registry**: checks the canonical form at publish with
  `spdx-expression-parse`, stores `license` and `license_refs` on the version
  row, and copies them to the plugin row when the version is the highest (as
  `description` and `homepage` are), for the search file and the list views.
- **Web API and site**: a plugin summary carries `license`; a plugin's detail
  carries `license` and `licenseRefs` on each version. The plugin page shows
  the latest version's license in the side panel, each SPDX identifier linked
  to `spdx.org/licenses/`, each `LicenseRef-*` shown by its `name` (linked to
  its `url`), and "Not declared" without one. A version's details show its
  own license.
- **CLI**: `celesta plugin info` prints `license: <expression>` for the newest
  version, or `license: not declared`.

## First-party values

- `ffmpeg`: `(MIT OR Apache-2.0) AND LGPL-2.1-or-later`
- `ffmpeg-x264`: `(MIT OR Apache-2.0) AND GPL-2.0-or-later`
- `vrm`, `avfoundation`, `mediafoundation`: `MIT OR Apache-2.0`

These name the plugin's own code and the FFmpeg build it links. The
maintainer confirms them before the next release.

## Rollout

1. This repository: the format (schema, fixtures, types), the build script,
   the first-party values, the CLI, and the docs.
2. The registry: `format/PIN` moves to the commit with the new schema; the
   migration, publish check, files, API, and site follow.

The registry's change can deploy before or after a Celesta release: both
sides treat the fields as optional.
