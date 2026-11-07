# Plugin performance (2026-11-08)

The v0.10.0 review of the plugin system left three performance questions
open: what the codec plugin's pipe costs (`2026-11-05-codec-plugin.md`,
Not done), the scene layer texture made every frame
(`2026-11-03-plugin-scene-content.md`, Per-frame GPU resources), and the
MToon shader compiled when the first avatar opens
(`2026-11-03-plugin-vrm.md`, Not done). Each was measured first; two
changes came out of it, and the third needs none for now.

## Measurements

All on an Apple M4 (macOS, Metal), release builds. The machine was shared
with other builds, so every comparison runs the old and new binaries
interleaved in random order and gives medians (minimums where noted);
the tables below are from the quietest runs (load average 12–40). Times
are per frame.

### Codec frames across the pipe

`plugins/ffmpeg/examples/codec-bench.rs` (new) decodes a video's frames in
order, as playback and export do, and encodes RGBA frames to H.264
(`ultrafast`), once with FFmpeg in the bench's own process and once
through the codec plugin's program. Clips: `testsrc2`, H.264 yuv420p,
30 fps, 1920x1080 and 3840x2160; 240 frames a run.

| | in process | plugin, before | plugin, after |
| --- | --- | --- | --- |
| decode 1080p | 0.78 ms | 2.19 ms | 1.69 ms |
| decode 4K | 3.33 ms | 8.98 ms | 8.01 ms |
| encode 1080p | 1.75 ms | 3.02 ms | 2.83 ms |
| encode 4K | 7.40 ms | 13.75 ms | 14.15 ms |

The CPU time the plugin's process spent decoding dropped from 7.4 to
6.9 ms (1080p) and from 28.7 to 26.7 ms (4K) a frame; the host's
reading, from 1.08 to 0.77 ms (1080p). In a run under heavy load, the
shared answer alone saved the plugin 2.0 ms (1080p) and 6.5 ms (4K) of
CPU a frame. The encode rows did not change beyond the noise. They time
the whole run of frames, copying each one included (four runs a side): a
first version of the bench timed only the `encode_frame` calls, which
left out what an encoder in the bench's process does on its own threads
meanwhile, and so made in-process encoding look cheaper (1.28 and
4.93 ms) and costlier in CPU through the plugin than it is. Counted
whole, encoding costs about the same CPU either way (about 61 ms a 4K
frame in process, 55 + 5 ms through the plugin); what the plugin adds is
about 1.1 ms (1080p) and 6.6 ms (4K) of wall-clock time, the frame's way
through the pipe.

What the pipe itself costs: `dd` through a pipe moves 5.8–8.5 GiB/s here,
about 1.0–1.3 ms for a 1080p RGBA frame (8.3 MB) and 4–5 ms for a 4K one
(33 MB). After the change, decoding through the plugin costs 0.9 ms
(1080p) and 4.7 ms (4K) more than in process, which is about that.

End to end, `crates/exporter/examples/preview-bench.rs` (the editor's
preview path: the React bridge, the plugin's sequential decode, the GPU
render) on an entry showing the clip full frame, 150 frames, `SEQUENTIAL=1`:

| | before | after |
| --- | --- | --- |
| 1080p | 6.8 ms | 5.9 ms |
| 4K | 27.0 ms | 25.6 ms (minimums 20.7 and 21.4: no difference) |

At 4K the decode is about 8 ms of the 25: the rest is uploading and
drawing the 4K frame, so the pipe no longer decides 4K playback.
Exporting the same entries (`celesta-exporter --react`, libx264 `medium`)
took about 15 ms (1080p) and 60 ms (4K) a frame, too noisy under the
load to tell before from after; the encoder dominates. Before and after
wrote byte-identical MP4s.

### The scene layer texture

`plugins/vrm/examples/render-bench.rs` (new) draws the synthetic avatar
(`SyntheticVrm::avatar`) through `render_preview`, the VRM provider in the
same process, a layer as large as the frame, 240 frames at successive
times:

| | before | after |
| --- | --- | --- |
| 1080p, MToon | 1.36 ms | 1.15 ms |
| 1080p, unlit | 1.44 ms | 1.21 ms |
| 4K, MToon | 9.50 ms | 5.89 ms |
| 4K, unlit | 9.07 ms | 6.58 ms |

A throwaway test timed the texture alone: making a 1920x1080 layer
texture, clearing it, and waiting cost 0.69 ms more than clearing a kept
one; 0.96 ms at 3840x2160.

### The MToon shader

The avatar's first frame (opening the model, compiling its shaders and
pipelines, drawing) with Metal's shader cache warm: about 14 ms with MToon
and 15–19 ms with the same avatar's materials made plain glTF (unlit). The
first run of a newly built binary, whose Metal cache is cold: 743–817 ms
for the first renderer in the process (Celesta's own pipelines and the
unlit ones) and 468–481 ms for a second renderer opening the MToon avatar.
So MToon costs about 450 ms once, when Metal has not compiled it before
(the first avatar after installing or updating Celesta), and nothing
noticeable after. Not changed: compiling it before an avatar opens would
need the plugin's WGSL before the plugin sends it, and moving it to a
background thread would leave the first frame without the avatar instead.

## Changes

- **A codec writes the frame it keeps without copying it.** The FFmpeg
  codec keeps the last frame it decoded (to answer a repeat request or
  hold past the end), so `Arc::try_unwrap` on the frame always failed and
  the codec role copied every frame into the answer before writing it.
  `celesta_plugin::protocol::Answer` is what a plugin's main loop answers
  with: a `Message`, or `Answer::Shared { header, payload: Arc<Vec<u8>> }`,
  written from the share. `serve_requests_on` and `serve_requests` take a
  closure returning anything `Into<Answer>`, so closures returning
  `Message` still compile; the codec role answers `decodeVideo` with
  `Answer::Shared`. The bytes on the wire are unchanged.
- **A message part is read into a buffer of its size.** `read_body` still
  starts with at most 1 MiB (`PREALLOCATED`), so a bogus length reserves at
  most twice the bytes that came, but now doubles by itself, capped at the
  part's length, instead of letting `read_to_end` grow the buffer past it
  (an 8.3 MB frame went 1 → 2 → 4 → 8 → 16 MiB; a 33 MB one ended in a
  64 MiB buffer). Both sides read this way: the host the decoded frames,
  the codec the frames it encodes.
- **A scene layer's texture is kept from frame to frame.** Each
  `SceneTargets` (the canvases of one size) now keeps the layer textures
  its frames were converted into, and a scene frame draws into one that
  nothing else holds (`LayerTexture::unshared`: its `Arc` has no other
  handle), or makes one. Two scene layers of one size in a frame each get
  their own, and a texture something still draws with is never drawn into.
  The convert pass clears and covers the texture, so nothing of an earlier
  frame shows; the layer textures go with their size's canvases (dropped
  when a frame does not draw that size, or draws no scene), and count in
  `SceneFrameResources::allocations`. A frame whose scene draws nothing
  still returns a new, transparent texture. Output is unchanged.

## Checks

- New tests: `celesta-plugin`'s `a_shared_answer_is_written_as_its_message`,
  `a_large_payload_arriving_in_pieces_is_read_whole_into_its_size` (a
  5 MiB payload read 64 KiB at a time ends in a buffer of its size, and the
  next message follows intact), `a_payload_the_stream_ends_inside_is_an_error`;
  `celesta-media`'s `a_frame_the_codec_keeps_is_sent_whole_and_stays_its_own`;
  `celesta-gpu-renderer`'s
  `scene_layers_of_one_size_each_draw_into_their_own_texture` (two layers
  of one model at different times, side by side, each match their frame
  drawn alone, and the second frame allocates nothing), which fails when
  the pool hands out its first texture regardless. The existing
  `scene_resources_*` tests, which compare `allocations` across frames, now
  cover the layer textures too.
- Before/after screenshots of the synthetic avatar (MToon and unlit,
  1920x1080, `RENDER_BENCH_PNG`): byte-identical.
- `cargo test --workspace` (2,385 passed, 1 ignored as before; the codec
  plugin on `CELESTA_PLUGIN_PATH`), `cargo clippy --workspace --all-targets
  --locked -- -D warnings`, `cargo fmt --all -- --check`,
  `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`
  (412 passed; the VRM sample tests pass saying they skipped, without
  `CELESTA_VRM_SAMPLES_DIR`), the plugins workspace's clippy and
  `cargo fmt --check`, `pnpm --dir scripts/plugins test` (213 passed), and
  `git diff --check`, on macOS arm64 (Metal).

## Not done

- **Shared memory for codec frames.** What is left of the transport is
  the pipe's own copy: 0.9 ms (1080p) and 4.7 ms (4K) a decoded frame,
  1.1 and 6.6 ms an encoded one. Frames in a shared mapping (a ring of slots
  the host creates and the plugin writes, the answer naming the slot)
  would remove it, but the renderer keeps decoded frames past the next
  request, so the host would still copy out of a slot, or slots would need
  a lifetime protocol; it needs a design decision before it is built.
- **Uploading a 4K video frame** is now most of 4K preview's time (about
  17 of 25 ms); not looked into here.
- **Mesh (2D) plugin layers** still make their layer texture every frame
  (`mesh.rs`'s `plugin_canvas`); only scene layers keep theirs.
- No real VRM sample (`CELESTA_VRM_SAMPLES_DIR`) was available; the avatar
  timings use the synthetic one, whose texture and vertex counts are tiny,
  so they measure Celesta's fixed per-frame costs rather than a real
  model's drawing.
