# Plugin profiling: 4K preview and the codec's encode (2026-11-08)

`2026-11-08-codec-shared-memory-design.md` recommended profiling 4K
preview's upload and draw cost, and the codec plugin's encode overhead,
before building shared memory; the user chose to profile first. This note
is that profile and the fix it led to: video frames upload through kept
staging memory and textures.

## Profile

Apple M4, Metal, release builds, `preview-bench` on an entry showing a
3840x2160 H.264 clip full frame (`SEQUENTIAL=1`, the codec plugin from
`plugins/dist`), sampled with macOS `sample` for 4 s (about 2,400 samples
of the main thread, 26 ms a frame):

| where the main thread was | share | per frame |
| --- | --- | --- |
| waiting for the codec plugin's frame (`PluginChild::request`) | 36% | ≈ 9 ms |
| waiting for the GPU (`Device::poll`, the native preview bridge waiting for its submission) | 32% | ≈ 8 ms |
| `Queue::write_texture` uploading the frame, nearly all of it `memmove` into its staging buffer | 26% | ≈ 7 ms |
| the React bridge, the rest | 6% | |

A 33 MB `memcpy` takes about 0.9 ms here, so the upload's 7 ms was not the
copy. `write_texture` makes a staging buffer for each call (wgpu-core's
`queue_write_texture`), and `upload_texture` made a 4K texture for each
frame: both fresh memory the system maps and clears as the copy first
touches it. A throwaway test uploading a frame and waiting for the GPU,
120 times a variant:

| per frame | 1920x1080 | 3840x2160 |
| --- | --- | --- |
| new texture, `write_texture` (as before) | 1.83 ms (0.93 ms CPU) | 6.66 ms (3.61 ms CPU) |
| kept texture, `write_texture` | 1.55 ms | 5.18 ms |
| new texture, kept staging buffer | 0.83 ms | 2.51 ms |
| kept texture, kept staging buffer | 0.49 ms (0.15 ms CPU) | 1.49 ms (0.79 ms CPU) |

**The encode overhead was a bench error.** The CPU the codec plugin seemed
to spend encoding beyond the same encode in process (about 20 ms a 4K
frame) came from `codec-bench` timing only the `encode_frame` calls, which
left out what an encoder in the bench's own process does on its threads
meanwhile. Timed whole (fixed on #769's branch), encoding costs about the
same CPU either way, and the plugin adds 1.1 ms (1080p) and 6.6 ms (4K) of
wall-clock time: the frame's way through the pipe. Nothing to fix there
but the transport.

**Would pipelining `encodeFrame` hide it?** In an export
(`crates/exporter/src/render.rs`), Node evaluates the next frame on its own
thread and `GpuRenderer::submit` leaves the GPU working while the render
thread goes on, but `write_frame` sends each frame to the codec and waits
for `frameEncoded` before the render thread prepares the next one. Not
waiting would let the transfer overlap preparing the next frame, so it
could hide at most the transfer: about 6.6 ms a 4K frame with
`ultrafast`, and less with the default `medium`, where libx264 (about
55–60 ms a 4K frame here) is the bottleneck and the plugin's encoder
threads are still busy with earlier frames while the next one crosses
the pipe. It would also change the protocol: today `PluginChild` has one
request in flight and pairs each answer with it, so a frame sent without
waiting needs either several requests in flight with ordered answers or an
`encodeFrame` that is not answered. Not built: it changes the protocol for
a gain that only fast presets see, so it is the user's call.

## Change

`VideoUploads` (`crates/gpu-renderer/src/texture.rs`), the renderer's
`video_uploads`, uploads video frames in place of `upload_texture`:

- **Textures are kept** by size and mip level count. A frame is written
  into a kept texture nothing else holds (`LayerTexture::unshared`, from
  `2026-11-08-plugin-performance.md`), or a new one. Two video layers of
  one size in a render each get their own, and a texture something still
  draws with is never written. Every texel of every level is written, so
  nothing of an earlier frame shows.
- **Staging buffers are kept** by size (`MAP_WRITE | COPY_SRC`). The
  frame's levels are copied into a kept buffer the GPU has finished with,
  each level at an offset aligned to and with rows padded to 256 bytes as
  `copy_buffer_to_texture` needs, then copied to the texture in a
  submission ahead of the draws. The buffer is mapped again with
  `map_async`, whose callback marks it ready when the device is next
  polled (every render polls). A buffer still in use is passed over and a
  new one made. A buffer that cannot be mapped is dropped and its frame
  goes through `write_texture`.
- **A render lets go of what the render before it did not use**
  (`VideoUploads::begin_render`, at the start of `prepare_draws`), as the
  scene resources do, so a project that stops showing a video, or shows
  another size, does not keep the old memory.
- Images, text, and the other textures still go through `upload_texture`
  (`write_texture`): they are uploaded once and cached.

## Measurements

`preview-bench` (the editor's preview path), five runs of 150 frames each,
interleaved with the build without this change (#769's), load average
about 8:

| per frame | before | after |
| --- | --- | --- |
| 1080p | 6.2 ms | 5.3 ms |
| 4K | 24.6 ms | 21.2 ms |

Exporting the same entries (`celesta-exporter --react`, libx264
`medium`), three runs: 14.6 → 13.5 ms (1080p) and 57.4 → 54.2 ms (4K),
within the noise; the encoder dominates. The MP4s, and a still of frame 45
(`--frame 45`, below), are byte-identical before and after.

What a 4K preview frame now spends, by the profile less the upload: about
9 ms waiting for the plugin's frame (3.3 ms of it decoding, the rest the
pipe), about 8 ms waiting for the GPU, about 1.5 ms uploading.

## Checks

- New tests (`crates/gpu-renderer/src/tests.rs`):
  `video_frames_reuse_their_texture_and_staging_memory` (a 37x21 frame,
  whose 148-byte rows need padding, drawn exactly at two times and the
  first again without allocating; what a render without video leaves goes
  as the next begins), `video_layers_of_one_size_each_draw_into_their_own_texture`
  (fails when the pool reuses a texture regardless of `unshared`), and
  `a_mipmapped_video_frame_draws_as_the_same_image_does` (a final render
  at a quarter size, so with mipmaps at padded, aligned offsets, equals the
  same pixels drawn as an image through `write_texture`). The existing
  video tests pass unchanged.
- `cargo test --workspace` (2,388 passed, 1 ignored as before; the codec
  plugin on `CELESTA_PLUGIN_PATH`), `cargo clippy --workspace --all-targets
  --locked -- -D warnings`, `cargo fmt --all -- --check`,
  `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`
  (412 passed; its codec tests export video through the renderer) and its
  clippy, and `git diff --check`, on macOS arm64 (Metal).

## Not done

- **The plugin's frame** (about 9 ms at 4K) is now the largest wait. The
  plugin could decode a sequential session's next frame as soon as it has
  answered, while the host renders, so that only the transfer is left on
  the next request; shared memory (`2026-11-08-codec-shared-memory-design.md`)
  would then cut the transfer.
- **The GPU's 8 ms** at 4K (compositing on a `Rgba16Float` canvas, the
  native preview's NV12 conversion) was not looked into; it needs a Metal
  frame capture.
- Exports' readback (`render()`), and other per-frame uploads (CPU-raster
  path fallbacks), still allocate as before.
