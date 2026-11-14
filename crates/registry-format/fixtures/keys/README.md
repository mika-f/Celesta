# Test keys

The keys behind `fixtures/signed/` are test-only. Each is derived from a
fixed seed, so the signed tree regenerates byte for byte:

| Key | Seed | Role |
| --- | --- | --- |
| root | 32 bytes of `0x01` | signs the online keys in `config.json` |
| `2026-11` | 32 bytes of `0x02` | online key, expires 2099-01-01; signs `config.json`, `index/vrm.json`, `search.json`, `advisories.json` |
| `expired` | 32 bytes of `0x03` | online key, expired 2000-01-01; signs `expired/advisories.json` (for `/advisories.json`) |

The seeds are public, so these keys protect nothing, and their lifetimes
ignore the one-year limit real online keys follow. A release build never
trusts them; they exist only so tests can exercise verification.

Regenerate the tree with
`cargo run -p celesta-registry-format --example sign-fixtures`.
