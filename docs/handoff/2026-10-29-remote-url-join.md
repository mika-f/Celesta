# Stylesheet reference resolution with `url::Url::join` (2026-10-29)

## Specification and implementation

An `@font-face` stylesheet loaded from a URL names its font files with
`url(...)` references, which `crates/remote/src/stylesheet.rs` resolves against
the stylesheet's own URL. That resolution used a hand-written `join_url` and
`remove_dot_segments`. It now calls `url::Url::parse(base)?.join(reference)`,
and both helpers are gone. `url` 2.5.8 was already in `Cargo.lock` as a
transitive dependency; `celesta-remote` now depends on it directly.

Unchanged paths:

- An absolute `http(s)` reference still bypasses the join and keeps its exact
  string.
- `data:` references are still skipped.
- A stylesheet loaded from a local file still resolves references against its
  folder with `Path::join`.

The old helper dropped the base query for a fragment-only reference: base
`https://example.com/a/s.css?v=1` plus `#x` gave `https://example.com/a/s.css#x`.
RFC 3986 keeps the query, and `Url::join` now gives
`https://example.com/a/s.css?v=1#x`.

## Differences in the resolved URL string

The resolved string is the remote cache key (`fnv1a` of the exact URL) and the
download target. All the existing join cases give the same string as before:
dot segments, a reference with its own query and fragment, an absolute path, a
network-path reference, a base without a path, and `..` past the root. The
Google Fonts style case is also unchanged.

`Url` normalizes the joined URL, which changes the string in these cases:

- The scheme and host are lowercased: `HTTPS://Example.COM/` becomes
  `https://example.com/`.
- The default port is dropped: `https://example.com:443/` becomes
  `https://example.com/`.
- Spaces and non-ASCII characters in the path are percent-encoded, so
  `My Font.ttf` becomes `My%20Font.ttf`. Before, the raw string went to the
  HTTP client, which cannot send it.
- An internationalized host becomes punycode.
- A backslash in an `http(s)` path is a path separator, as in browsers.
- A reference such as `ftp://host/a.ttf` (an absolute URL with a scheme the
  crate does not download) now resolves to itself and fails with
  `UnsupportedScheme`. Before, it was glued onto the stylesheet's folder as a
  path.

The first two cases address the same file under a new cache key, so a cached
font is downloaded once more. The others fix URLs that could not be fetched
before.

## Checks

- `cargo test -p celesta-remote`: 18 passed. The new tests are
  `fragment_only_references_keep_the_base_query` and
  `joined_urls_are_normalized`.
- `cargo clippy -p celesta-remote --all-targets -- -D warnings` passed.
- `cargo fmt --all -- --check` passed.
