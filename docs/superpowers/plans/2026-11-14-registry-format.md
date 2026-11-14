# Registry Format Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Define the registry's client-facing files once (JSON Schema plus fixtures, public) and implement them in Rust: parsing, signature checks, the tree hash, and advisory matching.

**Architecture:** A new pure crate, `crates/registry-format` (`celesta-registry-format`), with no network and no Celesta dependencies. It holds the JSON Schemas and fixtures that the private registry service syncs from a pinned commit. Later plans build on it: the client store, the lock and resolver (plan 3), and the service (plan 2, in the private repository).

**Tech Stack:** Rust 2024 (MSRV 1.89), serde/serde_json, semver 1.0.28, sha2 0.11, `ed25519-dalek` 2 and `base64` 0.22 (new), `time` 0.3 with `serde-well-known` (RFC 3339), and `jsonschema` (dev-dependency only).

**Spec:** `docs/handoff/2026-11-14-plugin-registry-design.md`

## Roadmap (one plan per subsystem; this is plan 1)

1. **Registry format** (this plan, `mika-f/Celesta`).
2. **Service phase 1** (the registry service's private repository): Worker, D1, R2, publish API, admin workflow, generated and signed files. Needs 1.
3. **Client core** (`mika-f/Celesta`): store, `celesta.lock`, resolver, advisories fetch and enforcement, discovery integration in `celesta-plugin-host`. Needs 1. Can run in parallel with 2.
4. **`celesta` command**: `plugin search|info|add|remove|install|update|list|audit|gc|publish`. Needs 3.
5. **First-party publishing**: release workflow with attestations, then app packaging from the published archives. Needs 2 and 4.
6. **Desktop app**: Plugins tab sections, the plugin page, the missing-plugins question, Blocked notifications. Needs 3.
7. **Migration of the four packages**, and the public plugin-layer API for `vrm`. Needs 3 and 4.
8. **Service phase 2** (private). Needs 2.

## Global Constraints

- The crate depends on no Celesta crate, so `celesta-plugin-host` can depend on it later.
- `publish = false` in this plan. The crate becomes publishable in plan 3, when `celesta-plugin-host` first depends on it; that needs the maintainer's crates.io placeholder and Trusted Publishing configuration (see `2026-11-14-crates-io-trusted-publishing.md`).
- The format version is the integer `1` (`"format": 1`) in every JSON file.
- Clients ignore unknown fields. Schemas set `additionalProperties: true` at every object level, and the Rust types do not use `deny_unknown_fields`.
- Timestamps are RFC 3339 UTC strings.
- Hashes are written `sha256:<64 lowercase hex>`.
- Ed25519 public keys and signatures are standard base64 with padding.
- Ids: either a dotless first-party name, or `<publisher>.<name>`. Every part uses lowercase ASCII letters, digits, `-`, and `_`, starting with a letter or digit. At most one `.`.
- Targets are `<os>-<arch>`, with `os` in `windows | linux | macos` and Rust's arch names, or `any`.
- Tests are Rust (`cargo test`). Fixture files are JSON and plain files.

## Review Focus

- A file served under another path (index of `foo` returned for `bar`, or an old `advisories.json` replayed). Expected: refused. Covered: the signed message includes the path (Task 4), the index `id` must equal the requested id (Task 2), and an older `generated` is refused (Task 6).
- An expired online key, or a key not signed by the root. Expected: refused (Task 4).
- A tree on Windows and on macOS with the same files. Expected: the same tree hash. Covered: modes are not hashed, and `\` paths are normalized to `/` (Task 5).
- A plugin folder containing a symlink. Expected: the tree hash fails with an error naming the path (Task 5).
- A pre-release of the fixed version, such as `0.2.3-rc.1` against `<0.2.3`. Expected: blocked, because it predates the fix. semver's `matches` would let it through, so Task 6 compares by ordering and restricts advisory ranges to plain comparators.

---

### Task 0: Amend the design for decisions made while planning

**Files:**
- Modify: `docs/handoff/2026-11-14-plugin-registry-design.md`

- [ ] **Step 1: Apply four amendments, each marked "(designer's call, planning)"**
  - The per-target field is `treeHash`, not `fingerprint`, so it is not confused with the approval fingerprint.
  - The tree hash covers each regular file's relative path (with `/` separators) and its contents. File modes are left out because Windows cannot observe them. Registry archives may contain no symlinks: publishing refuses them and the tree hash fails on them.
  - Advisories match any plugin, from any source, by its manifest `id` and `version` against the advisory's range. Tree hashes are no longer used to match advisories, because junk files in a hand-placed folder (such as `.DS_Store`) would defeat that.
  - Signatures cover `celesta-registry-file-v1\n<path>\n` followed by the file's bytes, where `<path>` is the served path (such as `/index/vrm.json`).
- [ ] **Step 2: Commit**

```bash
git add docs/handoff/2026-11-14-plugin-registry-design.md
git commit -m "docs: settle the registry format's hashing, signing, and advisory matching"
```

### Task 1: Crate scaffold, schemas, and the fixtures layout

**Files:**
- Create: `crates/registry-format/Cargo.toml`, `crates/registry-format/src/lib.rs`, `crates/registry-format/README.md`
- Create: `crates/registry-format/schema/v1/{config,index,search,advisories,signature}.schema.json`
- Create: `crates/registry-format/fixtures/valid/{config.json,index-vrm.json,index-third-party.json,search.json,advisories.json}`
- Create: `crates/registry-format/fixtures/invalid/*.json`, one file per rule below
- Modify: `Cargo.toml` (members; `celesta-registry-format = { path = "crates/registry-format" }`, path only, like `celesta-codec-test`)
- Test: `crates/registry-format/tests/schema.rs`

**Interfaces:**
- Produces: the schema files and fixture paths that every later task and the private service use. The README states that the folder is the format's source of truth and that the service syncs it from a pinned commit.

Shapes, which the schemas encode:

```jsonc
// config.json
{ "format": 1, "api": "https://registry.celesta.studio/api/v1",
  "keys": [ { "keyid": "2026-11", "public": "<b64>", "expires": "2027-11-14T00:00:00Z", "signature": "<b64 by root>" } ] }
// <file>.sig
{ "keyid": "2026-11", "signature": "<b64>" }
// index/<id>.json
{ "format": 1, "id": "vrm", "publisher": "celesta",
  "kind": { "roles": ["content"], "program": true, "module": true },
  "description": "…", "homepage": null, "repository": "https://github.com/mika-f/Celesta",
  "deprecated": null,                       // or { "message": "…", "replacement": "<id>" | null }
  "versions": [ { "version": "0.2.3", "celesta": ">=0.9.0, <0.10.0", "protocol": 1,
      "published": "2026-11-20T00:00:00Z", "yanked": false, "withdrawn": false, "deprecated": null,
      "source": { "repository": "https://github.com/mika-f/Celesta", "commit": "<40 hex>", "workflow": ".github/workflows/package.yml" },
      "targets": { "macos-aarch64": { "url": "https://registry.celesta.studio/archives/vrm/0.2.3/macos-aarch64.zip",
                                      "size": 123, "sha256": "sha256:<hex>", "treeHash": "sha256:<hex>" } } } ] }
// search.json
{ "format": 1, "generated": "…", "plugins": [ { "id", "publisher", "kind", "description", "latest": "0.2.3" | null, "deprecated" } ] }
// advisories.json
{ "format": 1, "generated": "…", "advisories": [ { "id": "CELESTA-2026-0001", "plugin": "vrm", "versions": "<0.2.3",
    "title": "…", "description": "…", "fixed": "0.2.3" | null, "published": "…" } ] }
```

The publisher of a dotless id is `celesta`.

- [ ] **Step 1: Write `tests/schema.rs`**
  - `valid_fixtures_match_their_schema`: each `fixtures/valid/*.json` validates against its schema, matched by file-name prefix (`index-*` uses `index.schema.json`).
  - `invalid_fixtures_are_refused`: each `fixtures/invalid/<schema>-<rule>.json` fails validation against `<schema>`.

  Invalid fixtures, at least:
  - `index-bad-id` (`Vrm`)
  - `index-two-dots` (`a.b.c`)
  - `index-bad-target` (`macos`)
  - `index-bad-hash` (an uppercase hex digit)
  - `index-format-2`
  - `config-no-keys`
  - `advisories-no-range`
  - `advisories-caret-range` (`^0.2`)
  - `signature-no-keyid`
- [ ] **Step 2: Run it to make sure it fails**

  Run: `cargo test -p celesta-registry-format --test schema`

  Expected: FAIL (files missing).
- [ ] **Step 3: Write the schemas and fixtures above.** Put the id, target, and hash patterns in `$defs` and reference them.
- [ ] **Step 4: Run the tests**

  Same command. Expected: PASS.
- [ ] **Step 5: Commit**

  `git commit -m "feat(registry-format): schemas and fixtures for the client-facing files"`

### Task 2: Typed parsing

**Files:**
- Create: `crates/registry-format/src/{files.rs,ids.rs,error.rs}`
- Test: `crates/registry-format/tests/parse.rs`

**Interfaces:**
- Produces:
  - `pub fn is_registry_id(id: &str) -> bool`
  - `pub fn publisher_of(id: &str) -> &str` (`"celesta"` for a dotless id)
  - `pub struct Config { pub api: String, pub keys: Vec<OnlineKey> }` and `pub struct OnlineKey { pub keyid: String, pub public: [u8; 32], pub expires: OffsetDateTime, pub signature: [u8; 64] }`
  - `pub struct Index { pub id: String, pub publisher: String, pub kind: Kind, pub description: String, pub homepage: Option<String>, pub repository: Option<String>, pub deprecated: Option<Deprecation>, pub versions: Vec<VersionEntry> }`
  - `pub struct VersionEntry { pub version: semver::Version, pub celesta: semver::VersionReq, pub protocol: u32, pub published: OffsetDateTime, pub yanked: bool, pub withdrawn: bool, pub deprecated: Option<Deprecation>, pub source: Option<Source>, pub targets: BTreeMap<Target, Artifact> }`
  - `pub struct Artifact { pub url: String, pub size: u64, pub sha256: Hash, pub tree_hash: Hash }`, `pub struct Hash(pub [u8; 32])` (serde as `sha256:<hex>`), `pub struct Target(String)` with `Target::ANY` and `Target::host()`
  - `pub struct Deprecation { pub message: String, pub replacement: Option<String> }`, `pub struct Kind { pub roles: Vec<String>, pub program: bool, pub module: bool }`
  - `pub struct SearchList { pub generated: OffsetDateTime, pub plugins: Vec<SearchEntry> }`, `pub struct AdvisoryList { pub generated: OffsetDateTime, pub advisories: Vec<Advisory> }`, `pub struct Advisory { pub id: String, pub plugin: String, pub versions: semver::VersionReq, pub title: String, pub description: String, pub fixed: Option<semver::Version>, pub published: OffsetDateTime }`
  - `Config::parse(&[u8])`, `Index::parse(&[u8], expected_id: &str)`, `SearchList::parse(&[u8])`, `AdvisoryList::parse(&[u8])`, each `-> Result<Self, FormatError>`
  - `pub enum FormatError { Json(serde_json::Error), Format(u64), IdMismatch { expected: String, found: String }, InvalidId(String), InvalidTarget(String), InvalidHash(String), Signature(String), KeyExpired(String), UnknownKey(String), Rollback, TreeHash(PathBuf, String) }` (thiserror)

- [ ] **Step 1: Write `tests/parse.rs`**
  - Every valid fixture parses.
  - `Index::parse(index-vrm, "other")` is `IdMismatch`.
  - A fixture copy with `"format": 2` is `Format(2)`.
  - `is_registry_id` accepts `vrm`, `natsuneko.toon-shader`, `a_b`; rejects `Vrm`, `.a`, `a.`, `a.b.c`, `-a`, an empty string.
  - `publisher_of("natsuneko.toon-shader") == "natsuneko"`; `publisher_of("vrm") == "celesta"`.
  - `Target::host()` is one of the target values in the fixtures on the supported CI platforms.
  - Unknown extra fields added to a fixture copy still parse.
- [ ] **Step 2: Run it to make sure it fails**

  Run: `cargo test -p celesta-registry-format --test parse`

  Expected: FAIL (items undefined).
- [ ] **Step 3: Implement.** serde with `#[serde(rename_all = "camelCase")]`. Add a `format` check before typed decoding (read `format` from a `serde_json::Value`). `Target::host()` uses `std::env::consts::{OS, ARCH}`.
- [ ] **Step 4: Run the tests.** Expected: PASS.
- [ ] **Step 5: Commit**

  `git commit -m "feat(registry-format): parse config, index, search, and advisories"`

### Task 3: Fixture signing tool

**Files:**
- Create: `crates/registry-format/examples/sign-fixtures.rs`
- Create: `crates/registry-format/fixtures/keys/README.md`, stating that the keys are test-only, derived from fixed seeds, never trusted by a release build
- Create: `crates/registry-format/fixtures/signed/` (generated output: `config.json`, `index/vrm.json` + `.sig`, `advisories.json` + `.sig`, `search.json` + `.sig`, `root.pub`)

**Interfaces:**
- Produces:
  - `pub fn signed_message(path: &str, bytes: &[u8]) -> Vec<u8>`, which is `celesta-registry-file-v1\n{path}\n` + bytes
  - `pub fn key_message(keyid: &str, public_b64: &str, expires: &str) -> Vec<u8>`, which is `celesta-registry-key-v1\n{keyid}\n{public_b64}\n{expires}`

  Both go in `src/signing.rs` and are used by Task 4 and by the private service's port.

- [ ] **Step 1: Write the example.** The root seed is 32 bytes of `0x01` and the online seed is 32 bytes of `0x02`. It writes the signed tree. `config.json` lists two keys:
  - `"2026-11"`: the online key, expiring `2099-01-01T00:00:00Z`;
  - `"expired"`: seed `0x03`, expiring `2000-01-01T00:00:00Z`.

  Both are root-signed. The expired key signs only `fixtures/signed/expired/advisories.json` and its `.sig`.
- [ ] **Step 2: Run it**

  Run: `cargo run -p celesta-registry-format --example sign-fixtures`

  Expected: the files are written; running it twice leaves `git status` clean (Ed25519 is deterministic).
- [ ] **Step 3: Commit the example and its output**

  `git commit -m "test(registry-format): deterministic signed fixtures"`

### Task 4: Signature verification

**Files:**
- Create: `crates/registry-format/src/signing.rs` (with the Task 3 functions), `crates/registry-format/src/keyring.rs`
- Test: `crates/registry-format/tests/signing.rs`

**Interfaces:**
- Consumes: `Config`, `FormatError` (Task 2); `signed_message` and `key_message` (Task 3)
- Produces:
  - `pub struct RootKey(pub [u8; 32])`
  - `pub struct Keyring { … }`
  - `Keyring::from_config(config: &Config, root: &RootKey, now: OffsetDateTime) -> Result<Keyring, FormatError>`: keeps the keys whose root signature verifies and that are unexpired; fails with `Signature` when none remain
  - `Keyring::verify(&self, path: &str, bytes: &[u8], sig_json: &[u8]) -> Result<(), FormatError>`
- [ ] **Step 1: Write `tests/signing.rs`**
  - The signed fixtures verify with `root.pub` at `now = 2026-11-14T00:00:00Z`.
  - Verifying `index/vrm.json`'s bytes under the path `/index/other.json` fails (`Signature`).
  - One flipped byte in the file fails.
  - A `.sig` naming an unknown `keyid` is `UnknownKey`.
  - The expired key is dropped, so its file is `UnknownKey`.
  - A config whose key is signed by a different root yields `Signature` from `from_config`.
- [ ] **Step 2: Run it to make sure it fails**

  Run: `cargo test -p celesta-registry-format --test signing`. Expected: FAIL.
- [ ] **Step 3: Implement with `ed25519_dalek::VerifyingKey::verify_strict`.**
- [ ] **Step 4: Run the tests.** Expected: PASS.
- [ ] **Step 5: Commit**

  `git commit -m "feat(registry-format): verify signed registry files"`

### Task 5: Tree hash

**Files:**
- Create: `crates/registry-format/src/tree.rs`
- Create: `crates/registry-format/fixtures/trees/basic/` (a `celesta-plugin.json`, `bin/macos-aarch64`, `licenses/LICENSE-MIT`, an empty folder `empty/` with a `.gitkeep` that the test removes) and `fixtures/trees/basic.hash`
- Test: `crates/registry-format/tests/tree.rs`

**Interfaces:**
- Produces: `pub fn tree_hash(dir: &Path) -> Result<Hash, FormatError>`

Algorithm (fixed, since the service implements it too):

```text
entries = every regular file under dir, walked without following links
          a symlink anywhere → Err(TreeHash(path, "symbolic link"))
          a path that is not valid UTF-8 → Err(TreeHash(path, "not UTF-8"))
key     = relative path, components joined with "/"
sort entries by key, byte order
sha256( "celesta-tree-v1\n"
        || for each: u64le(len(key)) || key || u64le(len(contents)) || contents )
```

Empty directories and file modes are not hashed.

- [ ] **Step 1: Write `tests/tree.rs`**
  - `basic` equals `basic.hash`.
  - A copy with one byte changed differs.
  - A copy with a file renamed differs.
  - A copy with an extra empty directory is equal.
  - A copy whose file mode changed is equal (Unix only).
  - A copy with a symlink is `TreeHash(_, "symbolic link")` (Unix only).
  - Two directories built in different creation orders are equal.
- [ ] **Step 2: Run it to make sure it fails**

  Run: `cargo test -p celesta-registry-format --test tree`. Expected: FAIL.
- [ ] **Step 3: Implement with `std::fs::symlink_metadata` and a recursive walk.** Then compute `basic.hash` with the implementation, check it by hand once with a short script in the scratch folder, and commit it.
- [ ] **Step 4: Run the tests.** Expected: PASS.
- [ ] **Step 5: Commit**

  `git commit -m "feat(registry-format): tree hash of a plugin directory"`

### Task 6: Advisory matching and rollback

**Files:**
- Create: `crates/registry-format/src/advisories.rs`
- Test: `crates/registry-format/tests/advisories.rs`

**Interfaces:**
- Consumes: `AdvisoryList`, `Advisory` (Task 2)
- Produces:
  - `AdvisoryList::blocking(&self, plugin: &str, version: &semver::Version) -> Option<&Advisory>`
  - `AdvisoryList::check_newer(&self, cached: Option<&AdvisoryList>) -> Result<(), FormatError>`: `Rollback` when `self.generated < cached.generated`; equal is accepted
- [ ] **Step 1: Write `tests/advisories.rs`**
  - With the fixture `vrm <0.2.3`: `vrm 0.2.2` is blocked; `0.2.3` is not; `0.2.3-rc.1` is blocked; `voicevox 0.2.2` is not.
  - Two advisories for one plugin: the first in file order is returned.
  - `check_newer` against an older `generated` is `Ok`, against a newer one is `Rollback`, against `None` is `Ok`.
  - An advisory with `"versions": "^0.2"` fails to parse with `InvalidRange`.
  - `">=0.1.0, <0.2.3"` blocks `0.1.0` and `0.2.2`, and not `0.0.9`.
- [ ] **Step 2: Run it to make sure it fails**

  Run: `cargo test -p celesta-registry-format --test advisories`. Expected: FAIL.
- [ ] **Step 3: Implement without `VersionReq::matches`.** semver's `matches` leaves pre-releases out of ranges, so `<0.2.3` would not match `0.2.3-rc.1`. Instead, evaluate each comparator with `semver::Version`'s ordering, in which `0.2.3-rc.1 < 0.2.3`. The comparators must all hold. Advisory ranges allow only the operators `=`, `<`, `<=`, `>`, and `>=`, each with a full `major.minor.patch`, so this ordering is well defined. `AdvisoryList::parse` refuses any other operator (`^`, `~`, `*`) with `FormatError::InvalidRange(String)`; add that variant to Task 2's enum. Document the rule in the README.
- [ ] **Step 4: Run the tests.** Expected: PASS.
- [ ] **Step 5: Commit**

  `git commit -m "feat(registry-format): match advisories and refuse rollbacks"`

### Task 7: Handoff note and full checks

**Files:**
- Create: `docs/handoff/2026-11-14-registry-format.md` (specification, implementation table, checks), per `AGENTS.md`
- Modify: `HANDOFF.md`: one workspace-map row for `celesta-registry-format`; add `-p celesta-registry-format` to the validation commands if they list crates
- Modify: `crates/registry-format/README.md`: the format in prose, the signing messages, the tree-hash algorithm, the pre-release rule, and the sync contract for the service

- [ ] **Step 1: Run the full checks**

  ```bash
  cargo test -p celesta-registry-format
  cargo clippy -p celesta-registry-format --all-targets --locked -- -D warnings
  cargo fmt --all -- --check
  cargo test --workspace --locked
  ```

  Expected: all pass. `Cargo.lock` gains `ed25519-dalek`, `base64` (already present), and `jsonschema` (dev).
- [ ] **Step 2: Write the note and the map row.**
- [ ] **Step 3: Commit**

  `git commit -m "docs: hand off the registry format"`
