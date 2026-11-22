# 2026-11-22: registry-client test fixed to the real clock

## Problem

`selection::tests::file_urls_become_dir_transports`
(`crates/registry-client/src/selection.rs`) failed on every CI OS from
2026-11-22 with `Format { path: "/config.json", source: FutureDated }`.

`TestRegistry::file()` (`crates/registry-client/src/testing.rs`) starts its
`generated` stamps at `OffsetDateTime::now_utc() - 1 day`, so the `celesta`
binary and the editor, which read the real clock, accept it. The test checked
the keyring at the fixed time `2026-11-21T00:00:00Z`. On 2026-11-22 the
`config.json` stamp became 2026-11-21T…, later than that fixed time, so the
client refused it as future-dated.

## Change

The test now checks at `OffsetDateTime::now_utc()`, like
`testing::tests::a_file_registry_is_fresh_now`. The fixture is unchanged: its
clock-based stamps are what the binary-level tests need.

## Other fixed times checked

- Every other `TestRegistry::file()` user (`crates/cli/tests`,
  `crates/editor/src/{plugin_page,registry}`) checks on the real clock.
- `crates/cli/src/advisories.rs` tests use `at("2026-11-21T00:00:00Z")`, but
  only against `AdvisoryList` values they build relative to that time, with no
  registry, so they do not depend on today's date.
- Tests on `TestRegistry::new()` / `with_base()` use the fixed
  `FIRST_GENERATED` stamp (2026-11-20), so fixed check times stay valid there.

## Checks

- Before the change, today: `cargo test -p celesta-registry-client --lib
  file_urls_become_dir_transports` fails with `FutureDated`.
- After: `cargo test -p celesta-registry-client` passes (all suites);
  `cargo clippy -p celesta-registry-client --all-targets` has no warnings.
