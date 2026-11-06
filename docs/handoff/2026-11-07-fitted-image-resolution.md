# Fitted raster images keep their own resolution (2026-11-07)

Issue #699: with `fit`, `ImageSources::render`
(`crates/renderer/src/image_source.rs`) drew a raster into a buffer of the
box's own size, one pixel per layout unit, picking the nearest source texel
for each. The GPU renderer then filtered and mipmapped that point-sampled
copy. A photo shrunk into a small box with `fit="cover"` showed moiré and
shimmered as it moved. Under a parent scale it went blurry, since the
texture held only as many texels as the box had units. Without `fit`, the
same image kept its full pixels and looked right, and
`@celesta.studio/web` draws the whole image under the final transform
either way.

## Specification

- A raster or animated image with `fit` is drawn from the part of the image
  the box shows, at the image's own resolution:
  - `cover`: the image cropped to the box's aspect ratio, centered, in
    source texels.
  - `contain`: the whole image, centered in transparency that pads it to
    the box's aspect ratio.
- `fitted_rect` gives that part as a `SourceRect` in source texels. With
  `cover` it lies inside the image. With `contain` it reaches past the
  image on two sides. The output is `round(rect)` pixels, resampled from
  the rect by `resample_rgba`, so it may start between texels and still
  land exactly on the box. A crop that starts on a whole texel at 1:1 copies
  the texels unchanged.
- A box with the image's own aspect ratio draws the source pixels
  themselves, shared like an unfitted image's, with no render cached.
- `resample_rgba` (`crates/renderer/src/resample.rs`) is `resize_rgba`
  over a rect: the same alpha-weighted triangle filter. Inside the image its taps read the texels just outside
  the rect, and stop at the image's edges as before. Past the image they
  read transparency, so a `contain` edge is filtered against its padding.
  `resize_rgba` is `resample_rgba` over the whole image, unchanged.
- Padding a long image to a square could hold far more texels than the
  image does. A fitted output past 16384 pixels on a side, or 64 million in
  all, is shrunk to fit. It used to be an error past those limits for the
  box's size. A raster with `fit` now has no error for a large box, as
  without `fit`.
- The GPU renderer is not changed. It takes the fitted image as it takes an
  unfitted one: capped at `max_texture_dimension` by `fit_within`, with
  mipmaps, filtered under the final transform. The CPU reference renderer
  still picks the nearest texel for every image, fitted or not.
- SVGs are unchanged: rasterized at the box's size times the density.

## Checks

- `cargo test -p celesta-renderer --lib image_source`:
  `fits_a_raster_at_its_own_resolution` (crop, padding, shared pixels),
  `fits_a_long_raster_within_the_rasterization_limits`,
  and the earlier fit tests; `cargo test -p celesta-renderer --lib resample`:
  `resamples_a_rect_starting_between_texels` and the earlier resize tests.
- `cargo test -p celesta-gpu-renderer --lib`:
  `filters_a_fitted_image_shrunk_into_its_box` (a one-texel checkerboard
  with `cover` in a box a quarter of its cropped size comes out mid grey)
  and `enlarges_a_fitted_image_as_sharply_as_an_unfitted_one` (under a 5x
  parent scale, a `cover` crop draws the same pixels as an unfitted image
  of the texels it shows). Both fail with the old nearest-texel fit.
- Before and after screenshots, and the same scene drawn by
  `@celesta.studio/web`'s `SceneCanvas`: a 1920x1080 poster with
  `fit="cover"` in a 180x180 box, and the same layer under a 3x parent
  scale. They are attached to the pull request.
