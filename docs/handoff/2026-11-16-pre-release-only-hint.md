# Name the pre-release when only pre-releases match (2026-11-16)

## Problem

`celesta plugin install --global avfoundation` failed with
`no version of avfoundation satisfies *: no version matches`, although the
registry lists `avfoundation` 0.10.0-alpha.3 for `macos-aarch64`. A plugin spec
without a requirement uses `VersionReq::STAR`, and with Cargo's semantics a
requirement matches a pre-release only when it names one, so `*` matches no
version of a plugin published only as pre-releases. That is the documented rule
(see [`2026-11-14-registry-client.md`](2026-11-14-registry-client.md)), but the
message read as if the plugin did not exist.
`avfoundation@0.10.0-alpha.3` installs.

## Specification

When no version matches the requirement, `refusal` in
`crates/registry-client/src/resolve.rs` looks for the newest pre-release whose
version without its pre-release part matches the requirement (`*` allows every
one, `^2` allows `2.0.0-rc.1` but not `3.0.0-alpha.1`). If there is one, the
reason is `only pre-releases match; ask for <id>@<version>`; otherwise it stays
`no version matches`. Resolution itself is unchanged: an unpinned `add` or
`install --global` still never chooses a pre-release.

Choosing the newest pre-release by default (when no stable version exists, or
when the running Celesta is a pre-release) was considered and left out; it
changes what a lock records and is the maintainer's call. The desktop app
(plan 5) faces the same question.

## Implementation

| Path | Change |
| --- | --- |
| `crates/registry-client/src/resolve.rs` | `newest_pre_release`, and its use in `refusal`. |
| `crates/registry-client/tests/resolve.rs` | `no_version_names_the_pre_release_to_ask_for`. |
| `packages/website/content/{en,ja}/docs/05-(more)/05-plugins.md` | The pre-release rule and the new message under "Declare a project's plugins". |
| `skills/celesta/references/plugins.md` | The rule, and to retry with the named spec only when the user wants the pre-release. |

## Checks

- `cargo test -p celesta-registry-client`
- `cargo clippy -p celesta-registry-client --all-targets -- -D warnings`
- `cargo fmt --check`
