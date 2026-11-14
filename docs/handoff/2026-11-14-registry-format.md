# Plugin registry file format (2026-11-14)

Plan 2 of the plugin registry. The design is
`docs/handoff/2026-11-14-plugin-registry-design.md`. This plan adds the crate
`crates/registry-format` (`celesta-registry-format`): the schemas and fixtures
of the registry's client-facing files, and the code that parses and checks
them. The client (plan 3) is not started. The crate's `README.md` describes the
format in prose.

## Specification

- Files: `config.json`, `index/<id>.json`, `search.json`, `advisories.json`,
  and a detached `<file>.sig` next to each served file.
- Every JSON file has `"format": 1`. Clients ignore unknown fields; schemas set
  `additionalProperties: true` and the Rust types do not deny unknown fields.
- Timestamps are RFC 3339 UTC. Hashes are `sha256:<64 lowercase hex>`. Keys and
  signatures are standard base64 with padding.
- Ids are a dotless first-party name or `<publisher>.<name>`. Targets are
  `<os>-<arch>` or `any`.
- Signing: a pinned root key certifies short-lived online keys listed in
  `config.json`; an online key signs `celesta-registry-file-v1\n{path}\n` plus
  the file bytes. The root signs
  `celesta-registry-key-v1\n{keyid}\n{public_b64}\n{expires}`, rebuilt from
  the parsed key (base64 with padding; `expires` as `time` formats it: a zero
  offset becomes `Z`, other offsets and fractional seconds are kept). The
  registry must certify `expires` in the `Z` whole-second form.
- A `Keyring` is a snapshot valid at the `now` it was built with. Callers
  rebuild it from a fresh `config.json` and the current time.
- Tree hash: `sha256("celesta-tree-v1\n" || for each regular file sorted by
  relative path bytes: u64le(len(path)) || path || u64le(len(contents)) ||
  contents)`. Files are read whole, names are raw UTF-8 with no normalization,
  modes are ignored, symlinks are refused.
- Advisories: plain comparators only (`=`, `<`, `<=`, `>`, `>=`, each a full
  `major.minor.patch`), compared by semver precedence, so a pre-release of the
  fixed version is blocked; build metadata is ignored. A range that did not go
  through `AdvisoryList::parse` fails closed. A list older than the cached one
  is refused as a rollback; an equal timestamp is accepted.
- Sync contract: the registry service syncs `schema/` and `fixtures/` from a
  pinned commit of this repository and runs its own implementation against
  them.
- The crate depends on no Celesta crate and has `publish = false`.

## Implementation

| Path | Responsibility |
| --- | --- |
| `schema/v1/*.schema.json` | JSON Schemas (draft 2020-12) for config, index, search, advisories, and signature files. |
| `fixtures/valid/`, `fixtures/invalid/` | Files each schema must accept or refuse; the name prefix of an invalid file picks the schema. |
| `fixtures/signed/`, `fixtures/keys/README.md` | A deterministic signed tree (test keys from fixed seeds), including an expired key. |
| `fixtures/trees/` | A plugin directory and its expected tree hash. |
| `examples/sign-fixtures.rs` | Regenerates `fixtures/signed/` byte for byte. |
| `src/files.rs` | Typed files (`Config`, `Index`, `SearchList`, `AdvisoryList`), `Hash`, `Target`; format check; range validation. |
| `src/ids.rs` | `is_registry_id`, `publisher_of`. |
| `src/error.rs` | `FormatError`. |
| `src/signing.rs` | `signed_message` and `key_message`, the signed byte layouts. |
| `src/keyring.rs` | `RootKey`, `Keyring::from_config` (certification and expiry) and `Keyring::verify`. |
| `src/tree.rs` | `tree_hash` of a plugin directory. |
| `src/advisories.rs` | `AdvisoryList::blocking` and `check_newer`. |
| `tests/` | `schema`, `parse`, `signing`, `signed`, `tree`, `advisories`. |
| `README.md`, `src/lib.rs` docs | The format in prose. |
| `HANDOFF.md` | One workspace-map row. Its validation commands use `--workspace`, so they needed no change. |

## Checks

Run in this worktree, in this order.

| Command | Result |
| --- | --- |
| `cargo test -p celesta-registry-format --locked` | Passed: 49 tests (3 unit, 11 advisories, 8 parse, 2 schema, 4 signed, 11 signing, 10 tree). |
| `cargo clippy -p celesta-registry-format --all-targets --locked -- -D warnings` | Passed, no warnings. |
| `cargo fmt --all -- --check` | Passed. |
| `cargo test --workspace --locked` | Passed: 2977 tests passed, 0 failed, 1 ignored (summed over all test binaries). |

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
