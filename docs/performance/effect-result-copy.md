# Lone effect results are not copied

A layer with effects is drawn onto a canvas of its own, filtered, and the
result is drawn onto its parent. The last step in `Compositor::draw_group`
(`crates/gpu-renderer/src/compositor.rs`) used to draw every filtered texture
(shadow, glow, then the layers or their blur) onto one more canvas through the
group's inner instance, even when there was only one texture to draw. The
inner instance is the identity at full opacity with the `Normal` blend, and
the canvas starts transparent, so for one texture that pass only copied it:
one clear and one full-region draw per layer for nothing.

Now, when a layer has no shadow and no glow, the blurred canvas (or, without
a blur, the canvas its filters ended on) is returned as the result. A layer
with a shadow or a glow still composites its textures as before.

This covers the common cases: a layer with only a blur (NEBULA's six blobs),
and a layer with only custom shaders or built-in color filters (a grade over
an image).

## Measurements

Ubuntu 22.04, 2 vCPUs, Mesa lavapipe (llvmpipe, a CPU Vulkan driver), release
builds of `celesta-bench` at 1280x720, 6 frames after 2 warmup frames, three
rounds alternating before and after. Before = `main` at `424b8b1`. Medians:

| Workload | Before | After | Ratio |
| --- | ---: | ---: | ---: |
| `blur` | 324 ms/frame | 268 ms/frame | 0.83 |
| `shaders` | 195 ms/frame | 147 ms/frame | 0.75 |
| `shader-chain` | 197 ms/frame | 145 ms/frame | 0.74 |
| `color-filters` | 204 ms/frame | 156 ms/frame | 0.76 |
| `masks` | 108 ms/frame | 95 ms/frame | 0.88 |
| `nebula` | 540 ms/frame | 459 ms/frame | 0.85 |

These are not real-GPU measurements. llvmpipe pays for a clear and a copy in
proportion to the pixels, like any other pass; a discrete GPU clears almost
for free and copies far faster than it filters, so the saving there is
expected to be smaller. The skipped pass is still one full-region render pass
and one canvas fewer per such layer.

The last frame of `blur`, `shaders`, `shader-chain`, `shader-nest`,
`color-filters`, `masks` and `shapes` is pixel-identical before and after.
