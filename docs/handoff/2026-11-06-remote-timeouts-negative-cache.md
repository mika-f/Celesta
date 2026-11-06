# Remote download timeouts and failure memory (2026-11-06)

Issue #705: `RemoteAssetCache::fetch` (`crates/remote/src/lib.rs`) set only a
connect timeout on its shared `ureq` agent, so a host that accepted the TCP
connection and never answered blocked the React bridge handshake, frame
decoding, and export forever. It also cached only successes, so a failing URL
(404, offline, DNS failure) was requested again on every preview frame by
`AnimatedImages::is_animated` and the GPU renderer's `image_texture`.

## Specification

- The agent bounds every phase before the body at 30 s each:
  `timeout_resolve`, `timeout_connect`, `timeout_send_request`, and
  `timeout_recv_response`.
- The body fails once it goes 30 s without data (`timeout_per_read`, which
  restarts on every read). There is no total body or global limit, so a large
  video that keeps arriving is never cut off.
- A body read failure (such as a stall) is reported as
  `RemoteAssetError::Request` with the `ureq` error, not as a cache I/O error.
  The staged file is dropped, so nothing partial is left at the entry path.
- A failed download (`Request` or `Io` from the network path) is remembered
  for 30 s, keyed by cache root and normalized URL. During that time `fetch`
  returns `RemoteAssetError::RecentlyFailed { url, message, retry_after }`
  with the original message, without touching the network; afterwards it
  tries again. The memory is process-wide (a `OnceLock<Mutex<HashMap>>`),
  since `RemoteAssetCache::standard()` makes a new cache for every lookup.
  Expired entries are pruned on every insert.
- `data:` URLs, invalid URLs, and unsupported schemes are not remembered:
  they fail without a request. Local files are unaffected, so
  `AnimatedImages` still retries a missing local file on a later frame.

## Implementation notes

- The limits live in a private `Limits` struct (`STANDARD_LIMITS`), and
  `agent_config(limits)` builds the agent configuration, so tests build a
  cache with 300 ms limits and run in about a second.
- The en/ja media guide and `skills/celesta/references/react-core.md` say
  that a download stops after a period without data and a failed URL is tried
  again after a short delay.

## Checks

- `cargo test -p celesta-remote`: new tests cover a server that accepts and
  never answers (timeout, no file), headers followed by a stalled body
  (timeout, no file), a slow body that keeps arriving past the stall limit
  in total (succeeds), and a counting 404 server (one request within the
  window across new cache instances, a separate root not sharing it, and a
  new request after the window).
- `cargo clippy -p celesta-remote --all-targets -- -D warnings`,
  `cargo fmt --check`.
