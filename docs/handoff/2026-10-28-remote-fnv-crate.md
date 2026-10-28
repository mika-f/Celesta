# Remote cache keys use the `fnv` crate (2026-10-28)

`celesta-remote` keyed its per-user disk cache with a hand-rolled 64-bit
FNV-1a over the URL bytes. It now uses `fnv::FnvHasher` from
[servo/rust-fnv](https://github.com/servo/rust-fnv). `fnv` 1.0.7 was already
in `Cargo.lock` as a transitive dependency, so no new crate is downloaded.

## What changed

- `crates/remote/Cargo.toml`: adds `fnv = "1.0.7"`.
- `crates/remote/src/lib.rs`: `fnv1a` keeps its signature and feeds the bytes
  through `Hasher::write`, then `finish`. It does not go through `Hash`,
  because `impl Hash for [u8]` writes a length prefix first and would change
  every cache directory name, orphaning existing cache entries.
- New test `cache_keys_are_64_bit_fnv1a` pins three published FNV-1a 64
  vectors (`""`, `"a"`, `"foobar"`), so a change in how the hash is fed
  fails the test instead of silently moving the cache.

The cache directory layout (`remote-v1/<16 hex digits>/<file name>`) is
unchanged, so caches written by earlier builds are still hit.

The TypeScript FNV-1a functions (`packages/math/src/hash.ts`,
`packages/react/src/shader.ts`) are untouched: `rust-fnv` is a Rust crate.

## Checks

```bash
cargo test -p celesta-remote
cargo clippy -p celesta-remote --all-targets -- -D warnings
cargo fmt -p celesta-remote --check
```

No user-facing behavior or API changed, so the user guide and the Agent Skill
are untouched.
