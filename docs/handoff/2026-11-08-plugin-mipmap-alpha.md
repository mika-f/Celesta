# Plugin scene mipmaps weigh color by alpha (2026-11-08)

Closes the "Not done" entry in
[`2026-11-03-plugin-scene-content.md`](2026-11-03-plugin-scene-content.md):
straight-alpha mipmaps fringe cut-outs at distance. A cut-out or blended
texture whose transparent texels are black (or any other color) went dark
or tinted at its edges once a plugin's 3D scene showed it small, because
each mip level averaged those texels' straight colors with the covered
ones.

## Where mips are built

Three paths build mip levels from straight-alpha RGBA. Only the scene path
averaged straight color:

- **2D plugin meshes** (`mesh.rs`, `premultiplied_mips`) premultiply
  before averaging, and the mesh shader samples premultiplied texels.
  Unchanged.
- **Layer images** (`texture.rs`, `downsample`) weight color by alpha
  (`box_downscale(.., Coverage::Alpha)`), and `layer.wgsl` premultiplies
  each texel before it filters. Unchanged.
- **3D plugin scenes** (`scene.rs`, `straight_mips`) averaged every
  channel on its own, sRGB in linear light. The texture is uploaded with
  straight alpha and sampled by the hardware sampler, so both the mip
  levels and the bilinear filter within a level blend straight colors.
  This is the one fixed here.

## Specification

- `straight_mips` takes a `Coverage` (`celesta_renderer::resample`).
  - `Coverage::Straight` is the old average: each channel on its own,
    sRGB colors in linear light.
  - `Coverage::Alpha` weights each texel's (linear) color by its alpha, so
    a mip texel takes no color from the fully transparent texels it
    averages. Alpha is the plain average in both modes, so a mask's
    coverage at distance is what it was; the mask entry point's alpha to
    coverage still sharpens it around the cutoff.
  - With `Coverage::Alpha`, `fill_transparent` then gives every fully
    transparent texel (alpha byte 0), on every level including the image
    itself, the color of the texel under it on the next level, coarsest
    level first. That is the alpha-weighted color of the smallest block
    around it that covers something. The hardware filter blends a
    transparent texel's color into its covered neighbours' (at level 0 as
    much as at a mip), so the transparent texels must carry their
    surroundings' color, not merely be left out of the average.
  - An image with no texel above alpha 0 is averaged straight, and left as
    it is, whatever coverage was asked for.
- `LoadedScene::load` picks the coverage per texture: `Coverage::Alpha`
  for an sRGB texture that no opaque material shows (an unlit material's
  base color texture, or any slot of a custom material), else
  `Coverage::Straight`.
  - An opaque material ignores alpha and shows a texel's color whatever its
    alpha says (for example an albedo with smoothness packed in alpha), so
    its textures keep the plain average, and their transparent texels'
    colors are not replaced.
  - Linear textures are data (normals, masks): their alpha need not be
    coverage, so they are averaged straight as before.
  - A texture both an opaque and a cut-out or blended material show keeps
    the straight average, and fringes as before.
- Textures without mipmaps (`Sampler::mipmaps` false) are uploaded as the
  plugin sent them: no levels are built, so there is nothing to fill.
- The turntable sample's leaf now has transparent black clear texels
  instead of padding them with the leaf's green: Celesta fills them, and
  the sample's comment says so.

## Checks

- New GPU tests in `crates/gpu-renderer/src/tests/scene.rs` draw a 64x64
  texture on a square 4x4 pixels big (mip level 4). Its transparent texels
  are black or magenta (`[255, 0, 255, 0]`). Both failed before the fix:
  - `far_blended_cut_outs_keep_their_color`: a checkerboard, blended. The
    pixels are half covered and pure green unpremultiplied. Before:
    `[0, 187, 0, 128]` with black holes.
  - `far_masked_cut_outs_keep_their_color_and_coverage`: one hole in each
    2x2 block, masked at 0.5. The pixels are covered and pure green.
    Before: `[0, 225, 0, 255]`.
  - `far_opaque_textures_average_their_transparent_texels_too` keeps the
    opaque case as it was: green and magenta average to `[188, 188, 188]`.
    It passed before the fix and after it.
- New unit tests in `scene.rs`: alpha-weighted against straight averages
  next to a transparent magenta texel, partial alpha weighting
  (`[191, 0, 64, 170]` in linear), the fill spreading a covered texel's
  color to every transparent texel on every level, and a wholly
  transparent image left as it is. `srgb_mips_average_in_linear_light`
  keeps its bytes with `Coverage::Straight`.
- Before and after screenshots of the turntable sample (with its leaf's
  clear texels black) at four sizes on a light ground are in the pull
  request.
- `cargo test -p celesta-gpu-renderer`, `cargo test -p
  celesta-plugin-turntable`, `cargo clippy --workspace --all-targets -- -D
  warnings`, and `cargo fmt --check` pass.

## Not done

- **Coverage-preserving mips for masks.** Alpha averages plainly, so
  thin cut-out features (hair strands, fences) thin out at distance, as
  before. Scaling each level's alpha to keep the coverage at the cutoff
  needs the cutoff, which belongs to the material, not the texture.
- **Shared opaque and cut-out textures** keep the straight average (see
  above). Uploading two versions of such a texture would fix them at twice
  the memory.
