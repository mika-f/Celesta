# Issue 569 leftovers: asset refs, element return types, source-edit parsing (2026-11-07)

Addresses three of the items that mika-f/Celesta#569 left open after its
sub-issues closed (section 5 of its 2026-10-29 inventory). Each one keeps
behavior and public types as they were, apart from the return-type note
below. Other section-5 items were not touched. The fullscreen-triangle WGSL
and the shared affine helpers are already tracked in mika-f/Celesta#701.

## Specification

- **`useAssetReference` in `packages/react/src/components.ts`.** `<Image>`,
  `<Video>`, `<Font>` and `<Audio>` each repeated the same three steps:
  memoize the `AssetReference`, set an object `ref` during render, and
  register the imperative handle. One private hook now does this. It is
  memoized on `kind`, `src` and `name`. `kind` is a constant at each call
  site, so the memo works as before. `<Audio>` still passes no `name`, and
  `<Font>` still resolves `google` to a URL before the hook. The hook order
  in each component is unchanged: `useContext` (where used), `useMemo`,
  then `useImperativeHandle`.
- **`React.ReactElement` replaces `ReturnType<typeof React.createElement>`**
  in the 64 return types across `packages/*/src`. The old type picks the
  last `createElement` overload of `@types/react` 19, which is
  `ReactElement<{}>`. `React.ReactElement` is `ReactElement<unknown>`. A
  caller that stores the result as a `ReactElement` or renders it sees no
  difference. `packages/code` already used `React.ReactElement`. The old
  plans in `docs/superpowers/plans/` keep the old spelling as written.
- **`with_program` / `with_program_as` in `crates/source-edit/src/lib.rs`**
  replace `parse_checked` / `parse_checked_as`. They run a closure on the
  parsed program and return `None` when the parser found any error. A
  caller can no longer reach a recovered program by forgetting to check
  the flag. The 20 call sites now map `None` to the value they returned
  before: `.flatten()` for `Option`, `.unwrap_or_default()` for an empty
  `Vec`, `.unwrap_or(false)`, or `.unwrap_or(Err(…))` with the same lock
  reason. `parse` (tolerant, used by `locate`) is unchanged in behavior.
  `parses` and `parses_file` call `with_program` / `with_program_as` too.
  Most of the diff is re-indentation; `git diff -w` shows the real change.

## Checks

- `pnpm run build` (all 22 packages, including the `tsc` builds of every
  package touched) passes.
- `pnpm -r --filter "./packages/*" --no-bail run test` passes in every
  package, including `react` (313 tests), `components` (179), `character`
  (123), `shapes` (67), `cli` (118) and `web` (154).
- `pnpm run typecheck:examples` passes.
- `cargo test -p celesta-source-edit` passes (all 16 test binaries).
- `cargo clippy -p celesta-source-edit --all-targets -- -D warnings`,
  `cargo fmt --all -- --check` and `git diff --check` pass.
- The source-edit public API is unchanged, so its dependants were not
  rebuilt separately. No rendering code changed, so there are no
  screenshots. There is no user-facing change, so the user guide and the
  Celesta skill are untouched.

## Not done

The other section-5 items of #569 stay open: the matrix types, the three
`inspect` paths, the FFmpeg color-name tables, the `mix_audio_graph_*` and
`export_*` variants (a public API decision), the `*Def` mirror-enum macro,
`render.ts`'s `mount()`, the MCP `Args` flatten pattern, SRT/VTT and SVG
path libraries, and `#[allow(clippy::too_many_arguments)]`.
