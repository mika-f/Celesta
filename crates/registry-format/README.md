# celesta-registry-format

The plugin registry's client-facing file formats, and the code that reads and checks them.

This folder is the format's source of truth. This README describes what the code does.

- `schema/v1/*.schema.json` are JSON Schema (draft 2020-12) definitions. Objects allow
  additional properties, because clients ignore unknown fields.
- `fixtures/valid/` holds files every schema must accept. `fixtures/invalid/<schema>-<rule>.json`
  holds files a schema must refuse; the file-name prefix picks the schema.
- `fixtures/signed/` holds a deterministic signed tree (test keys only, see
  `fixtures/keys/README.md`). `fixtures/trees/` holds a plugin directory and its tree hash.
- `src/` holds typed parsing, signature verification, the tree hash, and advisory matching.
- `cargo test -p celesta-registry-format` checks all of it.

The crate depends on no other Celesta crate. It has `publish = false` for now.

## Conventions

- Every JSON file has `"format": 1`. Any other value is refused (`Format`); a missing
  `format` is a JSON error.
- Clients ignore unknown fields. The schemas and the Rust types accept them.
- A timestamp is RFC 3339 in UTC with whole seconds, spelled exactly
  `YYYY-MM-DDTHH:MM:SSZ` (such as `2026-11-20T00:00:00Z`). The schemas and the parsers refuse
  every other spelling, including `+00:00`, other offsets, and fractional seconds such as
  `.000Z` (`InvalidTimestamp`). Write timestamps without the milliseconds JavaScript's
  `toISOString()` adds. The schema pattern checks each field's range but not the length of
  the month, so it accepts `2026-02-30T00:00:00Z`, which the parser refuses.
- A hash is written `sha256:` followed by 64 lowercase hex digits.
- Ed25519 public keys and signatures are canonical standard base64 with padding: a 32-byte
  key is 44 characters ending in one `=`, a 64-byte signature 88 characters ending in `==`.
  The decoder refuses a last character whose unused bits are not zero, and the schema
  patterns check the same.
- A version is a semantic version in semver's grammar: `major.minor.patch`, an optional
  pre-release, and optional build metadata. Numeric identifiers have no leading zeros and no
  identifier is empty, so `1.0.0-01` and `1.0.0-a..b` are refused. The schema patterns allow
  at most 19 digits per number, which always fits the parser's 64 bits.
- A plugin id is a dotless first-party name, or `<publisher>.<name>`. Each part uses
  lowercase ASCII letters, digits, `-`, and `_`, and starts with a letter or digit. There is
  at most one `.`. The publisher of a dotless id is `celesta`.
- A target is `<os>-<arch>` with `os` one of `windows`, `linux`, `macos` and `arch` made of
  lowercase ASCII letters, digits, and `_` (Rust's architecture names), or `any`.

## Files

### `config.json`

| Field | Meaning |
| --- | --- |
| `format` | `1` |
| `api` | Base URL of the registry API (a URI). |
| `keys` | At least one online key (below). |

An online key has `keyid` (a non-empty string), `public` (32 bytes, base64), `expires`
(timestamp), and `signature` (64 bytes, base64): the root key's certification of the key.
`OnlineKey` keeps the decoded values (`public`, `expires`) and the exact strings the file
holds (`public_b64`, `expires_text`).

### `index/<id>.json`

One file per plugin.

| Field | Meaning |
| --- | --- |
| `format`, `id`, `publisher` | `id` must be a valid id and equal the id the client asked for. |
| `kind` | `roles` (strings), `program` (bool), `module` (bool). |
| `description` | Text. |
| `homepage`, `repository` | Optional strings. |
| `deprecated` | Optional: `message`, and an optional `replacement` id. |
| `versions` | The version entries (below). |

A version entry has `version` (a version), `celesta` (a requirement on the Celesta version,
below), `protocol` (unsigned integer), `published` (timestamp), `yanked`, `withdrawn`
(bools), optional `deprecated` and `source` (`repository`, `commit`, `workflow`), and
`targets`, an object from target to an artifact: `url`, `size` (bytes), `sha256` (hash of
the downloaded archive), and `treeHash` (the tree hash of the unpacked directory, below).

`celesta` is a Cargo version requirement, as Rust's `semver::VersionReq` parses it:
comparators joined by `,`, each with an optional `=`, `<`, `<=`, `>`, `>=`, `~`, or `^`
(none means `^`), partial versions (`0.9`) and `*`, `x`, `X` wildcards allowed, or a lone
wildcard; at most 32 comparators, with spaces allowed around operators and commas. It is not
npm syntax: `>=0.9.0 <0.10.0` (comparators separated by a space), `||`, and hyphen ranges are
refused. The schema pattern accepts a subset of what the parser accepts: it also refuses
build metadata in a comparator, which the parser would drop.

### `search.json`

`generated` (timestamp) and `plugins`: entries with `id`, `publisher`, `kind`,
`description`, an optional `latest` version, and an optional `deprecated`. Every `id` must be
a valid id.

### `advisories.json`

`generated` (timestamp) and `advisories`: entries with `id`, `plugin`, `versions` (the
range, below), `title`, `description`, an optional `fixed` version, and `published`
(timestamp).

`plugin` follows the host's plugin id rule, not the registry id rule, because an advisory
matches a plugin from any source, hand-placed ones included: lowercase ASCII letters, digits,
`-`, `_`, and `.`, starting with a letter or digit, with any number of dots (`a.b.c` is
allowed). Anything else is refused with `InvalidId`.

### `<file>.sig`

A detached signature for a served file: JSON with `keyid` and `signature` (64 bytes, base64).
It has no `format` field. The signature covers the message below, not the bare file. Every
served JSON file has one, `config.json` included: `config.json.sig` is made by one of the
online keys that `config.json` itself lists.

## Signing

Both messages are byte strings. `\n` is one byte, 0x0A.

### File message

```text
"celesta-registry-file-v1\n" || path || "\n" || file bytes
```

`path` is the served path, such as `/index/vrm.json`. The file bytes are exactly the bytes
served, with no parsing or re-encoding. The online key signs this message with Ed25519.
Verification uses `verify_strict`.

### Key certification message

```text
"celesta-registry-key-v1\n" || keyid || "\n" || public_b64 || "\n" || expires
```

There is no trailing newline. The root key signs this message with Ed25519.

The message is built from the exact `keyid`, `public`, and `expires` string values in
`config.json`, as they appear in the file, with nothing decoded and re-encoded. The parsers
accept only one spelling of each (canonical standard base64 with padding, and the
`YYYY-MM-DDTHH:MM:SSZ` timestamp), so the certified text is also the canonical text.

### `Keyring`

`RootKey::from_base64` reads the pinned root public key. `Keyring::from_config(config, root,
now)` keeps each online key that has not expired at `now` (`expires <= now` is expired) and
whose root certification verifies. It fails with `Signature` when no key remains.

`Keyring::from_signed_config(config_bytes, sig_json, root, now)` is what clients call. It
parses `config.json`, builds the keyring as `from_config` does, and then verifies
`config.json.sig` with that keyring at the served path `/config.json`, returning the config
and the keyring. A signature by a key that is not in the keyring is `UnknownKey`; a bad one
is `Signature`.

A `Keyring` is a snapshot, valid at the `now` it was built with. It never re-checks expiry.
Callers rebuild it from a fresh, verified `config.json` and the current time each time they
need to verify, and do not keep one.

`Keyring::verify(path, bytes, sig_json)` parses the `.sig` file, finds the key by `keyid`
(`UnknownKey` if it is not in the keyring), and verifies the file message. A malformed `.sig`
or signature, or a failed check, is a `Signature` error.

### Online keys: lifetime and rotation

An online key is certified for at most one year: its `expires` is no more than one year
after the registry starts using it. The test keys in `fixtures/signed/` do not follow this.

To rotate, the registry:

1. Has the root certify a new online key and adds it to `config.json`, next to the old one.
2. Re-signs every served file with the new key, `config.json` included.
3. Removes the old key from `config.json`, and re-signs it, before the old key expires.

Between steps 1 and 3 both keys verify, so a client holding files signed by either key
still verifies them. A client needs no update for a rotation; only a new root key needs one.

## Tree hash

`tree_hash(dir)` hashes a plugin directory so that the same files give the same hash on
every platform.

```text
entries = every regular file under dir, walked without following links
key     = the relative path, components joined with "/"
sort entries by key, byte order of the UTF-8 bytes
sha256( "celesta-tree-v1\n"
        || for each entry: u64le(len(key)) || key || u64le(len(contents)) || contents )
```

- `len` counts bytes. `u64le` is eight bytes, little-endian.
- Each file is read whole.
- A name is hashed as its raw UTF-8 bytes. There is no Unicode normalization, so NFC and NFD
  spellings of one name are different keys. A name that is not valid UTF-8 is an error.
- File modes are ignored. Empty directories are not hashed.
- A symbolic link, anywhere in the tree or as `dir` itself, is refused. So is any entry that
  is not a regular file or a directory.
- The result is written `sha256:<hex>`.

`fixtures/trees/basic.hash` holds the hash of `fixtures/trees/basic`.

## Advisories

### Range rule

An advisory's `versions` uses only `=`, `<`, `<=`, `>`, and `>=`, each with a full
`major.minor.patch` (a pre-release is allowed, build metadata is not). Comparators are
comma-separated and all must hold. Anything else (`^`, `~`, `*`, bare or partial versions,
build metadata, an empty range, a version semver refuses) is refused with `InvalidRange`,
which carries the range as written.

These checks, and the `plugin` check, run while an `Advisory` is deserialized, so they apply
to every serde path into an `Advisory` or an `AdvisoryList`, not only to `AdvisoryList::parse`.

`AdvisoryList::blocking(plugin, version)` returns the first advisory, in file order, whose
`plugin` equals `plugin` and whose comparators all hold. Each comparator is evaluated by
semver precedence, not by `VersionReq::matches`, which skips pre-releases. So `<0.2.3` blocks
`0.2.3-rc.1`: a pre-release of the fixed version is below it, and is blocked. Build metadata
of the checked version is ignored.

To block every version of a plugin, use `>=0.0.0-0`: `0.0.0-0` is the lowest version by
precedence, so every version, pre-releases included, is at or above it.

An `Advisory` built in code, without deserializing, can hold a range the checks would
refuse. Matching fails closed for it: a comparator with an unsupported operator (`^`, `~`, a
wildcard), or without a minor or patch number, holds for every version, so it never narrows
the range. A range with no comparators (`*`) holds for every version of its plugin.

### Rollback and freeze rule

`AdvisoryList::check_newer(cached, now)` checks a verified list before it replaces the
cached one:

1. It fails with `FutureDated` when the list's `generated` is more than 10 minutes after
   `now` (`generated > now + 10 min`; exactly 10 minutes ahead passes). Otherwise a list
   dated in the future, once cached, would refuse every later list as a rollback.
2. It fails with `Rollback` when the list's `generated` is earlier than the cached list's.
   An equal timestamp is accepted. With no cached list, this step passes.

Freshness is part of the contract, so a client can tell a frozen list from a quiet one:

- The registry regenerates and re-signs `advisories.json` at least every 24 hours, and sets
  a new, later `generated` on every content change.
- A client warns when the newest list it has verified has a `generated` older than 7 days.
  The client (plan 3) enforces this; this crate only states it.

## Sync contract

The registry service does not keep its own copy of the schemas or fixtures. It syncs
`schema/` and `fixtures/` from a pinned commit of this repository, and runs its own
implementation against them: it must accept every file in `fixtures/valid/`, refuse every
file in `fixtures/invalid/`, produce and verify `fixtures/signed/` byte for byte, and
reproduce `fixtures/trees/basic.hash`. A change to the format here bumps the pinned commit
there.

Fixture bytes are exact (`-text` in `.gitattributes`), since the signatures and the tree hash
cover them. Regenerate `fixtures/signed/` with
`cargo run -p celesta-registry-format --example sign-fixtures`.
