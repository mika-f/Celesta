# Freeing CPU preview frames from GPUI's atlas (2026-11-07)

Issue #684: the editor shows a CPU preview frame (`PreviewPresentation::Image`,
every frame on Linux and Windows, and on macOS for an odd-sized composition
or when the native bridge fails) as an `Arc<RenderImage>`. Painting it puts a
full-size tile in the window's sprite atlas, keyed by the image's id, and
GPUI removes a tile only through `drop_image` or an `ImageCache` eviction.
The editor never called `drop_image`, so each frame shown while playing or
scrubbing stayed in GPU memory until the window closed (about 8 MB, and its
own atlas texture, per 1920x1080 frame).

## Specification

- `EditorView::preview` is a `ShownPreview` (`crates/editor/src/preview.rs`):
  the frame on screen and the CPU images it has replaced.
- `ShownPreview::show`, called from `show_preview_result`, keeps the
  replaced image in the queue. A native surface is not in the atlas and is
  dropped as before.
- `EditorView::render` takes the queue right after `poll_background_work`
  and frees each image with `cx.drop_image(image, Some(window))`, which also
  clears it from any other window's atlas. The old image leaves the atlas
  before the new one is painted.
- `replace_contents` calls `ShownPreview::hand_over` before `*self = next`:
  the outgoing view's queue and its frame on screen move into the new view's
  queue, and the new view's first render frees them.

## Implementation notes

- The issue's queue, not an `ImageCache`: `show_preview_result` runs in
  `poll_background_work`, which has no `Window`, and a cache would need an
  entity and a keyed source for frames that are never shown twice.
- `ShownPreview` keeps its fields private so that a frame can be replaced
  only through `show`, which is what queues it.

## Validation

- `cargo test -p celesta-editor`:
  `a_shown_cpu_frame_retires_the_one_it_replaces` (two CPU frames in turn
  leave the first in the queue, and taking it empties it) and
  `a_replaced_view_hands_its_images_to_the_next_one` (the outgoing view's
  queued and on-screen images both reach the next view's queue). No window
  is needed.
- `cargo clippy -p celesta-editor --all-targets -- -D warnings` and
  `cargo fmt --check`.
