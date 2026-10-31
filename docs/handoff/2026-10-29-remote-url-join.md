# Stylesheet reference resolution with `url::Url::join` (2026-10-29)

## Specification and implementation

An `@font-face` stylesheet loaded from a URL names its font files with
`url(...)` references, which `crates/remote/src/stylesheet.rs` resolves against
the stylesheet's own URL. That resolution used a hand-written `join_url` and
`remove_dot_segments`. It now calls `url::Url::parse(base)?.join(reference)`,
and both helpers are gone. `celesta-remote` depends on `url` 2.5.8 directly.

An absolute `http(s)` reference used to keep its exact string. It is now parsed
with `url::Url::parse`, so it gets the same encoding and normalization as a
joined one. This applies to stylesheets loaded from a URL and from a local
file. Google Fonts stylesheets name lowercase ASCII `fonts.gstatic.com` URLs,
so their strings do not change.

A reference that `Url::parse` or `Url::join` rejects is an error.
`stylesheet_font_faces` returns `RemoteAssetError::InvalidUrl`, which names the
reference, and the renderer reports it for the font asset as it would a failed
download. Before this change such a reference was dropped silently. A
stylesheet then lost that face without a message. If it was the only face, the
renderer reported the misleading "the stylesheet has no @font-face url()".

Every reference that fails here would also have failed to load before:
- `http::Uri`, which ureq uses, rejected most of them, for example a host with
  a space, `<`, or `|`.
- The rest named hosts that cannot resolve, such as `300.1.1.1` or `[zz]`.

What changes is that the error now says which URL is wrong.

Unchanged paths:

- `data:` references are still skipped. (Superseded by
  `2026-10-31-font-face-src-selection.md`, where native decodes them as web
  does.)
- A stylesheet loaded from a local file still resolves references against its
  folder with `Path::join`.

## Where the resolved URL goes

`stylesheet_font_faces` has one production caller:
`TextRasterizer::load_font_asset` in `crates/renderer/src/fonts.rs`. It runs when a
`<Font src>` asset turns out to be a CSS stylesheet with `@font-face` rules. For
each face, the resolved `AssetLocation::Url` goes to
`local_asset_path` → `resolve_asset_path` → `RemoteAssetCache::fetch`. There the
URL string:

- is the download target (`ureq`, which parses it as an `http::Uri`);
- names the cache directory (`fnv1a` of the exact string);
- names the cached file (`file_name`, from the last path segment);
- appears in every `RemoteAssetError` message.

Nothing else stores it. A changed string means a new cache entry, so an
already-cached face is downloaded once more.

## Old and new output compared

Main's `join_url` was copied unchanged into a scratch program and run next to
`Url::join` (`url` 2.5.8). Each output was also parsed as an `http::Uri` 1.5.0,
which `ureq` does before sending. "Sendable" means that parse succeeded.

Base `B` is `https://cdn.example.com/kit/css/fonts.css?v=2`, and base `Q` is
`https://example.com/a/s.css?v=1`.

Same output:

| Base | Reference | Output |
| --- | --- | --- |
| `B` | `../fonts/a.woff2?v=3#iefix` | `https://cdn.example.com/kit/fonts/a.woff2?v=3#iefix` |
| `B` | `./b.woff` | `https://cdn.example.com/kit/css/b.woff` |
| `B` | `/root.ttf` | `https://cdn.example.com/root.ttf` |
| `B` | `//other.example.com/c.ttf` | `https://other.example.com/c.ttf` |
| `https://example.com` | `d.ttf` | `https://example.com/d.ttf` |
| `https://example.com/a/b/` | `../../../e.ttf` | `https://example.com/e.ttf` |
| `Q` | `?q` | `https://example.com/a/s.css?q` |
| `https://example.com?x` | `a.ttf` | `https://example.com/a.ttf` |
| `https://example.com:8080/css/s.css` | `a.ttf` | `https://example.com:8080/css/a.ttf` |
| `B` | `My%20Font.ttf` | `https://cdn.example.com/kit/css/My%20Font.ttf` |
| `https://example.com/%E3%83%95/s.css` | `a.ttf` | `https://example.com/%E3%83%95/a.ttf` |
| `B` | `..`, `../` | `https://cdn.example.com/kit/` |
| `B` | `.` | `https://cdn.example.com/kit/css/` |
| `B` | `a/./b/../c.ttf` | `https://cdn.example.com/kit/css/a/c.ttf` |
| `https://user:pw@example.com/css/s.css` | `a.ttf` | `https://user:pw@example.com/css/a.ttf` |

Different output, with the decision for each:

| Base | Reference | Old | New | Decision |
| --- | --- | --- | --- | --- |
| `Q` | `#x` | `…/s.css#x` | `…/s.css?v=1#x` | Take new: RFC 3986 keeps the base query. |
| `Q#f` | `#x` | `…/s.css#x` | `…/s.css?v=1#x` | Take new, same reason. |
| `Q` | empty | `…/s.css` | `…/s.css?v=1` | Take new. Unreachable: an empty `url()` is dropped (`first_url` here, `font_source` since the source-selection change). |
| `HTTPS://Example.COM/css/s.css` | `a.ttf` | `HTTPS://Example.COM/css/a.ttf` | `https://example.com/css/a.ttf` | Take new. Same file, new cache key. |
| `https://example.com:443/css/s.css` | `a.ttf` | keeps `:443` | drops `:443` | Take new. Same file, new cache key. |
| `B` | `//EXAMPLE.org/a.ttf` | `https://EXAMPLE.org/a.ttf` | `https://example.org/a.ttf` | Take new. Same file, new cache key. |
| `B` | `フォント.woff2` | raw UTF-8 path | percent-encoded path | Take new. The old string was sendable, as raw UTF-8 bytes; the new one is the standard form. New cache key. |
| `https://example.com/フ/s.css` | `a.ttf` | raw UTF-8 path | percent-encoded path | Take new, same reason. |
| `B` | `My Font.ttf` | raw space, not sendable | `My%20Font.ttf` | Take new: fixes a URL that failed. |
| `B` | `a.ttf?x=1 2` | raw space, not sendable | `?x=1%202` | Take new: fixes a URL that failed. |
| `B` | `a<TAB>b.ttf` | raw tab, not sendable | `ab.ttf` (tab removed, as browsers do) | Take new: fixes a URL that failed. |
| `B` | `//例え.jp/a.ttf` | raw host, not sendable | `https://xn--r8jz45g.jp/a.ttf` | Take new: fixes a URL that failed. |
| `B` | `%2e%2e/a.ttf` | `…/css/%2e%2e/a.ttf` | `…/kit/a.ttf` | Take new: browsers treat `%2e%2e` as `..`. |
| `B` | `..\fonts\a.ttf` | `…/css/..\fonts\a.ttf` | `…/kit/fonts/a.ttf` | Take new: browsers treat `\` as `/` in `http(s)`. |
| `B` | `https:a.ttf` | `…/css/https:a.ttf` | `…/css/a.ttf` | Take new: a same-scheme reference is relative in browsers. |
| `B` | `ftp://example.com/a.ttf` | glued onto `…/css/` as a path | `ftp://example.com/a.ttf` | Take new: the fetch now fails with `UnsupportedScheme` instead of requesting a wrong URL. |
| `B` | `c:/x.ttf` | glued onto `…/css/` as a path | `c:/x.ttf` | Take new, same reason. |

The fragment-only case was reproduced before the switch: the new
`fragment_only_references_keep_the_base_query` test failed against the old
helper with `https://example.com/a/s.css#x`.

## Dependency cost

`url` 2.5.8 was already in `Cargo.lock`, pulled only by `gpui-pre` and
`gpui-pre-http-client`. That means only the editor built it. Adding it to
`celesta-remote` changes `Cargo.lock` by one line, `"url"` in
`celesta-remote`'s dependency list, and downloads nothing new.

Every crate that depends on `celesta-remote` now builds `url` too: the exporter,
renderer, GPU renderer, media, and React bridge crates. For `celesta-exporter`,
`cargo tree -e normal,build` lists 26 crates it did not build before. They are
`url`, `form_urlencoded`, `idna`, `idna_adapter`, the ICU4X normalizer and
properties crates with their data crates, and their support crates
(`zerovec`, `yoke`, `zerofrom`, `zerotrie`, `tinystr`, `litemap`, `writeable`,
`potential_utf`, `utf8_iter`, `stable_deref_trait`, `displaydoc`,
`synstructure`, and three derive crates).

## Alternative considered

Keeping the hand-written resolver was tried: fix the fragment-only case there,
and replace raw spaces with `%20`. Replacing one character is not enough.
`http::Uri` 1.5.0 also rejects every ASCII control character, `<`, `>`, a
backtick, and DEL in a path. A complete fix would need a defined
percent-encode set, which is what `url` already implements. Such a fix would
still differ from browsers on `%2e%2e`, backslashes, tabs, and IDN hosts.
This change takes `url` for that reason. The editor already ships the same
crates.

Pinning `idna_adapter` to `~1.0` would drop the ICU4X crates, but
`Cargo.lock` is shared by the workspace, so it would also turn off IDN support
in the editor's `gpui` dependency.

## Checks

- `cargo test -p celesta-remote`: 21 passed. The tests added here are
  `fragment_only_references_keep_the_base_query`,
  `resolves_edge_case_references_like_browsers`,
  `absolute_references_are_normalized_too`,
  `joined_urls_are_normalized`, and
  `references_that_are_not_urls_are_errors`.
- `cargo clippy -p celesta-remote --all-targets -- -D warnings` passed.
- `cargo fmt --all -- --check` passed.
