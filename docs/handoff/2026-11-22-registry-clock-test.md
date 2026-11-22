# File-registry test clock

`selection::tests::file_urls_become_dir_transports` validated a fixture using
a fixed 2026-11-21 clock, while `TestRegistry::file()` generated timestamps
relative to the real clock. After 2026-11-22 this failed with `FutureDated`.

Add `TestRegistry::file_at(now)` and use the same fixed `now` for fixture creation
and validation in that test. `file()` delegates with the real clock, preserving
the CLI integration fixtures. Production validation logic is unchanged.

Validation (`cargo test -p celesta-registry-client`):

```text
test result: ok. 62 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.55s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.32s
test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.34s
```

Clippy (`cargo clippy -p celesta-registry-client --all-targets -- -D warnings`)
finished successfully in 4.36s; root formatting and diff checks exited 0.
