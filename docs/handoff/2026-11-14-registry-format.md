# Plugin registry file format (2026-11-14)

Plan 1 of the plugin registry. The design is
`docs/handoff/2026-11-14-plugin-registry-design.md`. This plan adds the crate
`crates/registry-format` (`celesta-registry-format`): the schemas and fixtures
of the registry's client-facing files, and the code that parses and checks
them. The client (plan 3) is not started. The crate's `README.md` describes the
format in prose.

## Specification

- Files: `config.json`, `index/<id>.json`, `search.json`, `advisories.json`,
  and a detached `<file>.sig` next to each of them, `config.json` included.
- Every JSON file except a `.sig` has `"format": 1`. A later format is served
  under new paths (`/v2/...`) while the v1 files stay. Clients ignore unknown
  fields; the schemas rely on the default (additional properties allowed) and
  the Rust types do not deny unknown fields. `index-vrm.json` carries unknown
  fields to prove it.
- Timestamps are RFC 3339 UTC with whole seconds, exactly
  `YYYY-MM-DDTHH:MM:SSZ`; schemas and parsers refuse offsets and fractions
  (`InvalidTimestamp`). Hashes are `sha256:<64 lowercase hex>`. Keys and
  signatures are canonical standard base64 with padding, of exact length.
- Versions use semver's grammar; `celesta` is Cargo `VersionReq` syntax. Each
  schema pattern accepts a subset of what its parser accepts, checked by
  tests on sample sets. The one gap: the timestamp pattern cannot check the
  length of a month (`2026-02-30` passes the schema; the parser refuses it).
- Ids are a dotless first-party name or `<publisher>.<name>`. An advisory's
  `plugin` uses the host's wider plugin id rule (several dots allowed).
  Targets are `<os>-<arch>` for any lowercase `os`, or `any`.
- Signing: a pinned root key certifies short-lived online keys listed in
  `config.json`; an online key signs `celesta-registry-file-v1\n{path}\n`
  plus the file bytes, where `path` starts with `/` and holds no `\n`. The
  root signs `celesta-registry-key-v1\n{keyid}\n{public_b64}\n{expires}`,
  built from the exact `public` and `expires` strings in `config.json`
  (`OnlineKey::public_b64`, `OnlineKey::expires_text`). `config.json.sig` is
  made by one of the keys `config.json` certifies;
  `Keyring::from_signed_config` checks it. Online keys live at most one year;
  the README gives the rotation steps.
- A `Keyring` is a snapshot valid at the `now` it was built with. Callers
  rebuild it from a fresh, verified `config.json` and the current time. A
  cached, verified `advisories.json` stays in force after its key expires,
  until a newer verified list replaces it.
- Tree hash: `sha256("celesta-tree-v1\n" || for each regular file sorted by
  relative path bytes: u64le(len(path)) || path || u64le(len(contents)) ||
  contents)`. Files are read whole, names are raw UTF-8 with no normalization,
  modes are ignored, symlinks are refused, and regular files named
  `.DS_Store`, `Thumbs.db`, or `desktop.ini` (ASCII case-insensitive) are
  skipped at any depth. Publishing refuses archives with those names, names
  differing only by case or normalization, names with `\`, and
  Windows-reserved names.
- Advisories: plain comparators only (`=`, `<`, `<=`, `>`, `>=`, each a full
  `major.minor.patch`, no build metadata), compared by semver precedence, so a
  pre-release of the fixed version is blocked; the checked version's build
  metadata is ignored. `>=0.0.0-0` blocks every version. The checks run while
  an `Advisory` deserializes; an `Advisory` built in code with another range
  fails closed. `check_newer(cached, now)` refuses a list generated more than
  10 minutes after `now` (`FutureDated`) and one older than the cached list
  (`Rollback`); an equal timestamp is accepted. The registry re-signs
  `advisories.json` at least every 24 hours with a fresh `generated`; clients
  warn when the newest verified list is older than 7 days (plan 3).
- Sync contract: the registry service syncs `schema/` and `fixtures/` from a
  pinned commit of this repository and runs its own implementation against
  them. It reproduces every signature value; `config.json` and the `.sig`
  files are compared by value, not byte for byte.
- The crate depends on no Celesta crate and has `publish = false`.

## Implementation

| Path | Responsibility |
| --- | --- |
| `schema/v1/*.schema.json` | JSON Schemas (draft 2020-12) for config, index, search, advisories, and signature files. |
| `fixtures/valid/`, `fixtures/invalid/` | Files each schema and parser must accept or refuse; the name prefix of an invalid file picks both. |
| `fixtures/signed/`, `fixtures/keys/README.md` | A deterministic signed tree (test keys from fixed seeds), including `config.json.sig` and an expired key. |
| `fixtures/trees/` | A plugin directory and its expected tree hash. |
| `examples/sign-fixtures.rs` | Regenerates `fixtures/signed/` byte for byte. |
| `src/files.rs` | Typed files (`Config`, `OnlineKey`, `Index`, `SearchList`, `Advisory`, `AdvisoryList`), `Hash`, `Target`; format check; strict timestamps; advisory range and id validation during deserialization. |
| `src/ids.rs` | `is_registry_id`, `is_plugin_id`, `publisher_of`. |
| `src/error.rs` | `FormatError`. |
| `src/signing.rs` | `signed_message` and `key_message`, the signed byte layouts. |
| `src/keyring.rs` | `RootKey`, `Keyring::from_signed_config`, `Keyring::from_config` (certification and expiry), and `Keyring::verify`. |
| `src/tree.rs` | `tree_hash` of a plugin directory, skipping OS junk files. |
| `src/advisories.rs` | `AdvisoryList::blocking` and `check_newer`. |
| `tests/` | `schema` (fixtures and pattern/parser agreement), `invalid` (parsers against `fixtures/invalid/`), `parse`, `signing`, `signed`, `tree`, `advisories`. |
| `README.md`, `src/lib.rs` docs | The format in prose. |
| `HANDOFF.md` | One workspace-map row. Its validation commands use `--workspace`, so they needed no change. |

## Checks

Run in this worktree, in this order.

| Command | Result |
| --- | --- |
| `cargo test -p celesta-registry-format --locked` | Passed: 75 tests (3 unit, 17 advisories, 1 invalid, 13 parse, 9 schema, 4 signed, 16 signing, 12 tree). |
| `cargo clippy -p celesta-registry-format --all-targets --locked -- -D warnings` | Passed, no warnings. |
| `cargo fmt --all -- --check` | Passed. |
| `cargo test --workspace --locked --no-fail-fast` | Passed: 3003 tests passed, 0 failed, 1 ignored (summed over all test binaries). An earlier run without `--no-fail-fast` stopped at a timing check in `celesta-plugin-host` (`a_process_left_holding_stderr_does_not_delay_the_diagnostic`, 2.6 s under the parallel load), which this crate does not touch; alone it passed three times out of three. |
| `cargo run -p celesta-registry-format --example sign-fixtures --locked`, twice | The second run leaves `fixtures/signed/` unchanged. |

Not run: the first-party plugins' separate workspace (`plugins/`), which this
crate does not touch; `pnpm` tests; the registry service's own tests (a
separate repository).

## Later plans

- Plan 3: `celesta-plugin-host` depends on this crate and implements the
  client (fetch, verify, cache, advisories).
- Making the crate publishable needs the maintainer's crates.io placeholder
  and Trusted Publishing setup (see
  `docs/handoff/2026-11-14-crates-io-trusted-publishing.md`). Until then
  `publish = false`.
- The registry service must pin a commit of this repository and run its own
  implementation against `schema/` and `fixtures/`.

## Known gaps for later plans

Found in the final review and left open on purpose:

- `Config` and `OnlineKey` keep both the raw strings (`public_b64`,
  `expires_text`) and the parsed values. Parsing keeps them consistent, but a
  value built or changed in code can disagree: certification checks the raw
  strings, while expiry and trust use the parsed ones. Plan 3 should make
  `Keyring::from_config` re-derive both from the raw strings (or make the
  fields private) before any code builds a `Config` by hand.
- The tree hash skips `.DS_Store`, `Thumbs.db`, and `desktop.ini`. A manifest
  that names such a path (say `"module": "desktop.ini"`) would hide that file
  from the hash. The publish contract (plan 2) and the host (plan 3) should
  refuse manifest-named paths whose last component is one of those names.
- `config.json`, `index/<id>.json`, and `search.json` carry no rollback stamp,
  so an older signed copy can be replayed while its key is valid. Only
  `advisories.json` has one (`generated`). Add stamps in plans 2 and 3.
