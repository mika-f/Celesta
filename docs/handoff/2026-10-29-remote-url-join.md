# Stylesheet URL join: keep the hand-written resolver (2026-10-29)

Issue #600 proposed replacing `join_url` / `remove_dot_segments` in
`crates/remote/src/stylesheet.rs` with `url::Url::join`. After investigating,
the hand-written resolver stays. Two real gaps in it are fixed here.

## Specification and implementation

`join_url` resolves a relative `url(...)` in an `@font-face` stylesheet loaded
from a URL. Two changes:

- **Fragment-only references keep the base query.** RFC 3986 section 5.2.2
  says a reference with no path and no query keeps the base query. Before, base
  `https://example.com/a/s.css?v=1` plus `#x` gave
  `https://example.com/a/s.css#x`. It now gives
  `https://example.com/a/s.css?v=1#x`. A `?q` reference still replaces the base
  query.
- **Raw spaces are percent-encoded.** A quoted `url("My Font.ttf")` can hold
  spaces, and `ureq` rejects a URL containing them (it parses each URL as an
  `http::Uri`). `resolve_reference` now turns each space into `%20` for remote
  results: both a joined reference and an absolute `http(s)` reference. A
  stylesheet loaded from a local file still joins raw paths, because spaces
  are valid in file names.

## Where the resolved URL goes

`stylesheet_font_faces` has one production caller:
`TextRasterizer::load_font_asset` in `crates/renderer/src/fonts.rs`. It runs
when a `<Font src>` asset is a web-font stylesheet. Each resolved URL goes to
`RemoteAssetCache::fetch`. There the string is the download target, the cache
directory name (`fnv1a`), the cached file name, and the text of every
`RemoteAssetError`. Nothing else stores it.

Google Fonts stylesheets name absolute `fonts.gstatic.com` URLs without spaces,
so they see no change.

## Why not `url::Url::join`

- **The fragment fix has little practical value.** In `@font-face`, fragments
  appear only after a path: `webfont.eot?#iefix` for old IE, and
  `webfont.svg#Name` for SVG fonts. The old resolver already handled both. A
  fragment-only `url(#x)` points at the stylesheet itself, which is not a font,
  so even the corrected URL fails to load as one.
- **The dependency cost is real.** `url` 2.5.8 is in `Cargo.lock`, but only
  `gpui-pre` pulls it, so only the editor builds it. Depending on it from
  `celesta-remote` would make the exporter, renderer, GPU renderer, media, and
  React bridge crates build 26 more crates: `url`, `idna`, and the ICU4X
  normalizer and properties stack with its data and support crates
  (`cargo tree -p celesta-exporter -e normal,build`, compared without and with
  the dependency).

## Differences from `Url::join` that remain

Main's `join_url` was run next to `Url::join` (`url` 2.5.8) on 33 references.
Each output was also parsed as an `http::Uri` (`http` 1.5.0). The existing cases
match, and so do `?q`, `..`, `../`, `.`, a non-default port, an already
percent-encoded path, and userinfo.

These differences stay. None appears in font stylesheets in practice.

| Reference (base) | This resolver | `Url::join` |
| --- | --- | --- |
| `a.ttf` (`HTTPS://Example.COM/…`) | keeps the case | lowercases the scheme and host |
| `a.ttf` (`https://example.com:443/…`) | keeps `:443` | drops `:443` |
| `フォント.woff2` | raw UTF-8, which `http::Uri` accepts | percent-encoded |
| `a<TAB>b.ttf` | raw tab, rejected by `http::Uri` | tab removed |
| `//例え.jp/a.ttf` | raw host, rejected by `http::Uri` | punycode |
| `%2e%2e/a.ttf` | literal segment | treated as `..` |
| `..\fonts\a.ttf` | literal backslashes | `\` as `/` |
| `https:a.ttf` | appended as a path segment | relative reference |
| `ftp://…`, `c:/x.ttf` | appended as a path | kept as an absolute URL |

## Checks

- `cargo test -p celesta-remote`: 18 passed. The tests added here,
  `fragment_only_references_keep_the_base_query` and
  `encodes_spaces_in_remote_references`, failed before the fix.
- `cargo clippy -p celesta-remote --all-targets -- -D warnings` passed.
- `cargo fmt --all -- --check` passed.
- `Cargo.lock` and `crates/remote/Cargo.toml` match `main`.
