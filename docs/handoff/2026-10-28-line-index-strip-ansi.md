# line-index and strip-ansi-escapes (2026-10-28)

Two small hand-rolled helpers listed in issue #569 now use crates.

## source-edit: `line-index`

`element_at` and `later_series_items` convert between byte offsets and the
1-based line / UTF-16 column that esbuild's `jsxDEV` reports. Both now go
through rust-analyzer's `line-index` (0.1.2; pulls in `text-size`, and
`nohash-hasher`, which was already in the lockfile).

- `locate.rs` `to_offset`: `LineIndex::to_utf8(WideEncoding::Utf16, …)`, then
  the line's range from `LineIndex::line`. `to_utf8` does not check the column
  against the line, so the function still returns `None` for a column past the
  line's `\n`, for a column inside a character (half a surrogate pair), and for
  line or column 0. A column at the `\n` itself, or at the end of the text, is
  still accepted, as before.
- `timing.rs` `position_of`: `line_col` + `to_wide`. `later_series_items`
  builds the index once for all siblings instead of rescanning the text per
  sibling.
- Only `\n` ends a line in `line-index`, which keeps the existing `\r\n`
  behavior (the `\r` belongs to the line).

New tests in `crates/source-edit/tests/locate_apply.rs` pin a column after a
surrogate pair and columns that would run into the next line. They pass on
both the old and the new code.

## react-bridge: `strip-ansi-escapes`

`stderr.rs` `clean` calls `strip_ansi_escapes::strip_str` (0.2.1; pulls in
`vte` 0.14) and then cuts the result to `MAX_BYTES` at a character boundary.
`str::floor_char_boundary` is newer than the workspace's `rust-version`
(1.89), so the cut searches for the boundary itself.

Differences from the old CSI-only loop, all within the kept stderr tail of an
`UnexpectedExit` error (what is forwarded to this process's stderr is
unchanged):

- OSC (hyperlinks, titles) and other escape strings are stripped too. Test:
  `strips_hyperlinks_from_kept_lines`.
- `vte` drops C0 control characters other than `\n`, so a `\t` in a line is
  removed (`a\tb` becomes `ab`), as are BEL and similar. Node's code frame of
  a tab-indented source line loses its indentation in the error text.

## Checks

```bash
cargo test -p celesta-source-edit -p celesta-react-bridge
cargo clippy -p celesta-source-edit -p celesta-react-bridge --all-targets -- -D warnings
cargo fmt --all -- --check
```

react-bridge needs `PKG_CONFIG_PATH` pointing at FFmpeg 8 to build. No
user-facing API changed, so the user guide and the Agent Skill are untouched.
