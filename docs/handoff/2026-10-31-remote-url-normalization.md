# Remote asset URL normalization (2026-10-31)

## Problem

A remote asset URL written straight into `<Font src>`, `<Image src>`, or an
audio clip reached the HTTP client exactly as written. Nothing normalized it
on the way:

- JS passes `src` through raw: `packages/react/src/render.ts`,
  `text-measure.ts`, and `media.ts`.
- So does the bridge: `crates/react-bridge/src/audio.rs`, `bridge.rs`, and
  `decode.rs`.

`ureq` parses each URL as an `http::Uri`. That rejects control characters,
space, `<`, `>`, a backtick, DEL, and a raw IDN host. A URL such as
`https://example.com/My Font.ttf` therefore failed natively with
`RemoteAssetError::Request`. The browser renderer's `fetch` encodes it and
succeeds, so the two renderers diverged.

Only URLs inside a font stylesheet were normalized, since #605 resolved them
with `url::Url`.

## Implementation

Every remote asset goes through `RemoteAssetCache::fetch`: directly, or
through `resolve_asset_path`. `fetch` and `cached` now pass the URL through
`normalized` first. That function parses an `http(s)` URL with
`url::Url::parse` and serializes it again, which:

- lowercases the scheme and host,
- drops a default port,
- percent-encodes what a request line cannot carry,
- writes an IDN host as punycode,
- resolves dot segments, reads `\` as `/`, and adds `/` to an empty path.

`normalized` also drops the fragment. `http::Uri` never sends it, so before
this change `a.woff2#iefix` and `a.woff2` were one request but two cache
entries and two downloads. That predates the stack, since the key was the raw
string.

The normalized string is used both for the request and for the cache key
(`fnv1a`). The same URL written two ways now shares one entry. A URL that does
not parse fails with `RemoteAssetError::InvalidUrl`, which names it.

`data:` URLs and other schemes are left as they are. `data:` URLs are decoded,
and other schemes still fail with `UnsupportedScheme`.

The stylesheet code (`resolve_reference` in `crates/remote/src/stylesheet.rs`)
no longer parses absolute references itself. It returns them as written, and
the fetch normalizes them. Relative references are still resolved there with
`Url::join`. An absolute reference that is not a valid URL now fails when it
is fetched, rather than while the stylesheet is read. The error is the same.

`docs/handoff/2026-10-29-remote-url-join.md` (#605) is updated where this
change supersedes it: absolute references, the cache key, and the renamed
test.

## Behavior changes

- A remote URL with spaces, other characters a request line cannot carry, or
  a raw IDN host now downloads natively, as it does in the browser.
- A cached file whose URL was written in a non-normalized form is downloaded
  once more under its normalized key. Examples: an uppercase host, `:443`, a
  raw non-ASCII path, a bare origin without `/`, or a URL with a fragment.
  Everything else keeps its cache entry, because a URL already in normalized
  form serializes to the same string.

The user guide's media page (en/ja) now describes the normalization. The
Agent Skill (`SKILL.md` and `references/project-json.md`) says that only a
change to the path or query refreshes a cached URL. `project-json.md` also
notes that a `data:` URL source is decoded, which follows from the
source-selection change below this layer.

## Checks

- `cargo test -p celesta-remote`: 24 passed. The new test,
  `normalizes_urls_as_a_browser_fetch_does`, failed before the change, because
  the raw-space URL could not be requested.
- `cargo test -p celesta-renderer`: 226 + 1 passed.
- `cargo clippy -p celesta-remote -p celesta-renderer -p celesta-react-bridge --all-targets -- -D warnings`
  and `cargo fmt --all -- --check` passed.
- `pnpm --dir packages/web test`: typecheck passed and 150 tests passed. Run
  `pnpm run build:runtime` first to generate `packages/react/src/generated/`.
