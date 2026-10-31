# Group every fframes crate in Renovate (2026-10-31)

## Specification and implementation

`fframes`, `fframes_skia_renderer`, and `fframes_native_player` are released
together and pinned to the same version in `examples/versus/bench/fframes`, so
Renovate should update them in one pull request.

The `fframes` group matched by `matchSourceUrls`. On crates.io only
`fframes_native_player` publishes a `repository` URL; `fframes` has only a
homepage and `fframes_skia_renderer` has neither. The other two had no source
URL to match, fell out of the group, and arrived as separate pull requests
(#618 for `fframes`, #626 for `fframes_skia_renderer`, while the group branch
`renovate/fframes` carried only `fframes_native_player` in #624).

The rule now matches crate names `fframes` and `fframes_*` instead.

## Checks

- Queried the crates.io API for each crate and confirmed which ones publish a
  `repository` URL.
- Validated `renovate.json` with `renovate-config-validator`.
- `git diff --check` passed.
