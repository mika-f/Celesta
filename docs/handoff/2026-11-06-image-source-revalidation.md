# Image sources follow their file and stay within a budget (2026-11-06)

Issue #698: `ImageSources::load` (`crates/renderer/src/image_source.rs`)
returned early whenever its key, the asset id, was present. It never
compared the path, never noticed a file overwritten on disk, and never
dropped an entry. A React `<Image name="logo">` whose `src` changed kept
drawing the old file until the document was reopened, as did an image
overwritten while the composition was open. Every decoded image also stayed
in memory for the whole editor document or export run. The GPU texture key
had no path either, so fixing the CPU cache alone still drew the old texture
when the new image had the same size.

## Specification

- `ImageSources` keeps one entry per asset id: the decoded source, the file
  it came from, that file's modified time and length when it was read
  (`FileStamp`), its renders for fits and SVG densities, and a revision.
- Each use (`natural_size`, `layout`, `render`) reads the file's metadata.
  If the path differs from the entry's, or the stamp does, the entry is
  dropped and the file is decoded again under a new revision. One
  `metadata` call per image per use is the only cost on a hit.
- The stamp is read before the file. A write landing during the read leaves
  a stamp that differs from the file's next one, so the next use reads the
  file again.
- A file that disappears is an error on its next use, as on the first load.
  The old pixels are not drawn.
- `DisplayImage::revision` names the decode its pixels came from. The GPU
  renderer puts it in the image texture key (`image_texture` in
  `crates/gpu-renderer/src/prepare.rs`), so a new decode gets a new texture
  and the old one is dropped by the per-frame texture eviction.
- The entries hold at most `DEFAULT_BUDGET` (1 GiB) between them, counting
  decoded pixels (every frame of an animation), renders, and an SVG's file
  length (its parsed tree is not measured). Past the budget, the least
  recently used entries are dropped first, as `PsdSources` does.
- `begin_frame` starts a frame. The entries used since then are not dropped,
  so a frame that draws more than the budget does not decode its own images
  again. It also drops what only earlier frames used. `CpuRenderer::render`
  and `GpuRenderer::prepare_draws` call it. Before the first `begin_frame`,
  only the entry being used is kept past the budget.
- Reloading an SVG does not reload the system fonts its text draws with.
  They are scanned once, on the first SVG, and shared by every SVG the
  same `ImageSources` loads after it. Fonts installed later are seen only
  by a new `ImageSources` (a reopened document, or the next export run).
  See [`2026-11-06-svg-shared-fonts.md`](2026-11-06-svg-shared-fonts.md).
- `insert_raster` pixels have no file. They are drawn for their key whatever
  path a caller names, and a later `insert_raster` for the same key is
  ignored, as before.

## Choices

- Byte budget, not per-frame eviction: a slideshow that loops would decode
  each image again whenever it came back.
- The on-disk check lives in the renderer, not in the editor's watcher. The
  editor watches only the entry and `prepare()` dependencies, and the
  exporter and MCP server use the same renderer. A paused preview does not
  redraw on its own, so an overwritten image shows from the next frame the
  preview draws (a seek, playback, or a reload).
- `PsdSources` still keys its parsed documents by asset id, with the same
  stale `src` problem. It is not changed here.

## Checks

- `cargo test -p celesta-renderer --lib image_source`
  - `decodes_an_ids_new_file_and_its_file_overwritten`: id `logo` from
    `a.png`, then from `b.png` of the same size, draws `b.png`'s pixels
    under a new revision and keeps only one of them. Overwriting `b.png`
    draws the new pixels, also for a `cover` render.
  - `evicts_the_least_recently_used_images_past_the_budget` and
    `keeps_the_images_the_current_frame_uses_past_the_budget` cover the
    budget and `begin_frame`. `counts_renders_toward_the_budget` covers the
    renders.
- `cargo test -p celesta-gpu-renderer --lib draws_an_images_new_file_and_an_overwritten_one`
  renders id `logo` from a red PNG, then a blue one of the same size, then
  the blue file overwritten green, through one `GpuRenderer`. Without the
  revision in the texture key, the second render still draws red.
