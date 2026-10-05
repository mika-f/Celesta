# GPU blur pairs its taps (2026-10-03)

`effect.wgsl`'s separable Gaussian now reads two adjacent texels per
bilinear fetch, weighted in the ratio of their Gaussian weights, and computes
the weights by recurrence instead of `exp` per tap. At σ 48 a pass reads 145
texels per pixel instead of 289. NEBULA's GPU time on an M4 fell from
78.6 to 36.0 ms/frame, and its export from 49.4 to 37.2 s. Details and raw
data are in `docs/performance/gaussian-blur-pairing.md`.

- The layer texture bind group layout is now `filterable: true` (every bound
  texture is 8-bit unorm). `layer.wgsl` still uses `textureLoad`. The effect
  parameter bind group (group 1) gained a linear clamp-to-edge sampler at
  binding 1.
- Texels outside the canvas still count as transparent: their weight is
  zeroed before pairing, so a fetch at an edge lands exactly on the inside
  texel. Fractional shadow offsets are folded into the kernel along the
  blur, and resolved once per pixel across it. `blur_reach` is unchanged.
- The CPU renderer's `blur_pixels` is unchanged and is still the
  reference. `large_blurs_match_cpu` adds σ 13.5–64 (with an edge-crossing
  fractional shadow) and σ 0.05 / 0.3 under the same tolerance of 5. The
  measured maximum is 2.
- Not done: a reduced-resolution blur for large σ would cut reads much
  further, but needs the CPU reference changed to match.
