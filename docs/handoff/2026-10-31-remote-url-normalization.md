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
- writes an IDN host as punycode.

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

An HTTP error no longer leaves an empty entry directory behind, because the
directory is created only when a file is stored.

## Behavior changes

- A remote URL with spaces, other characters a request line cannot carry, or
  a raw IDN host now downloads natively, as it does in the browser.
- A cached file whose URL was written in a non-normalized form, such as an
  uppercase host, `:443`, or a raw non-ASCII path, is downloaded once more
  under its normalized key. Everything else keeps its cache entry: a URL that
  is already in normalized form serializes to the same string.

The user guide's media page (en/ja) now says URLs are percent-encoded before
the download.

## Checks

- `cargo test -p celesta-remote`: 24 passed. The new test,
  `normalizes_urls_as_a_browser_fetch_does`, failed before the change, because
  the raw-space URL could not be requested.
- `cargo test -p celesta-renderer`: 224 + 1 passed.
- `cargo clippy -p celesta-remote -p celesta-renderer -p celesta-react-bridge --all-targets -- -D warnings`
  and `cargo fmt --all -- --check` passed.
- `pnpm --dir packages/web test`: typecheck passed and 150 tests passed. Run
  `pnpm run build:runtime` first to generate `packages/react/src/generated/`.
