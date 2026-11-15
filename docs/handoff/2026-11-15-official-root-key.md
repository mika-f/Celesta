# The official registry's root key (2026-11-15)

Official builds now trust the production plugin registry,
`https://registry.celesta.studio`. Until now `official_root_key()` returned
`None`, so the `celesta` command printed the no-key message for every command
that needed the registry; see
[`2026-11-14-celesta-command.md`](2026-11-14-celesta-command.md) (decision 2)
and [`2026-11-14-registry-client.md`](2026-11-14-registry-client.md)
(decision 6).

## Specification

- The root public key is the Ed25519 key
  `eqOBn7d6SvlrSXx+f3oWpJPhxxAn209/9F8QqhIBSs0=` (standard base64, 32 bytes).
  It only certifies the online signing keys `config.json` lists, by the key
  certification message in
  [`crates/registry-format/README.md`](../../crates/registry-format/README.md);
  the first online key is `2026-11`, expiring `2027-11-15T00:00:00Z`. Rotating
  an online key needs no Celesta release; replacing the root key does.
- `official_root_key()` in `crates/registry-client/src/registry.rs` returns
  `RootKey`, not `Option<RootKey>`: every build has the key, parsed from the
  constant `OFFICIAL_ROOT_KEY` with `RootKey::from_base64`. `OFFICIAL_REGISTRY`
  was already `https://registry.celesta.studio`.
- With neither `CELESTA_REGISTRY` nor `CELESTA_REGISTRY_ROOT_KEY` set, the
  `celesta` command uses the official registry with that key over HTTPS. The
  override works as before: both variables together, one alone a usage error
  (exit 2), a bad key a usage error.
- The no-key path is gone, since nothing reaches it: `NO_KEY_MESSAGE`, the
  `NoKey` transport, `Session::has_key`, and `needs_registry` in `run()`.
  `Session::registry_within` returns a `Registry`. `install` without
  `--offline` now tries the registry instead of silently going offline for
  want of a key; a lock whose plugins are all in the store still installs
  without reaching it (as before), and a version missing from the store fails
  with the fetch error (`could not fetch …`).
- The user guide (`05-plugins.md`, en and ja) and the Agent Skill
  (`skills/celesta/references/plugins.md`) no longer say the build has no
  key; they name the official registry and say what still works when it
  cannot be reached. The Skill's error table swaps the no-key row for
  `could not fetch <url>: …` and `/<file>.json: signature error: …`.

## Tests

- `the_official_root_key_is_pinned` (`celesta-registry-client`): the constant
  parses, `official_root_key()` returns it, it round-trips to the same base64,
  and it is a valid Ed25519 public key.
- `the_registry_variables_go_together` (`celesta-cli` unit test): the official
  selection has the pinned key and the HTTP transport.
- The CLI integration tests must never reach the real registry, which the
  official key now makes reachable. `Home::command()` in
  `crates/cli/tests/common/mod.rs` therefore sets `CELESTA_REGISTRY` to
  `http://127.0.0.1:<port>`, a port that was free when the `Home` was made
  (connections are refused), with a test root key. `with_registry` overrides
  both. The tests that covered the no-key path now cover an unreachable
  registry: `an_unreachable_registry_is_a_failure` (was
  `no_key_fails_with_the_exact_message`),
  `install_without_the_registry_uses_the_store`,
  `list_works_without_the_registry`, and `audit_works_without_the_registry`
  (which now removes the file registry's served folder and runs against the
  same base, so the files that run cached are used, instead of copying the
  override cache into the official one).

## Validation

| Check | Result |
| --- | --- |
| `cargo test -p celesta-cli -p celesta-registry-client --locked --no-fail-fast` | Passed. |
| `cargo clippy -p celesta-cli -p celesta-registry-client --all-targets --locked -- -D warnings` | Passed. |
| `cargo check --workspace --all-targets --locked` | Passed. |
| `cargo fmt --all -- --check`, `git diff --check` | Passed. |
| `celesta plugin search vrm` from a debug build, temporary home, no override variables | Exit 0, `No plugin matches vrm`: the live `config.json` verified against the pinned root key (online key `2026-11`), and `search.json` and `advisories.json` against that online key; all four files landed in the official cache folder. |
| The same against `https://registry.celesta.studio` with another root key (`CELESTA_REGISTRY_ROOT_KEY` set to a different key) | Exit 1, `/config.json: signature error: no valid signing key`. |

Not run: the website build (the guide change is one paragraph, with no new
links or headings), the first-party plugins' workspace, and the rest of the
workspace's tests, which do not use `official_root_key()`.
