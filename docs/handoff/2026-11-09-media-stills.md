# Stills through the built-in image components, P4 (2026-11-09)

Phase P4 of the media components design
(`docs/superpowers/specs/2026-11-08-media-components-design.md`, #782;
plan `docs/superpowers/plans/2026-11-08-media-components-p4.md`), on top
of P3 (`docs/handoff/2026-11-09-export-presets-dialog.md`). Still frames,
image sequences, and contact sheets stop encoding PNG, JPEG, and WebP in
`stills.rs` and write through media components: a new crate,
`celesta-media-image`, gives the built-in video encoders `builtin/png`,
`builtin/jpeg`, and `builtin/webp` and the muxers `builtin/image` and
`builtin/imageSequence`, which run in Celesta's own process and need no
media plugin. Stills can now keep a transparent background (PNG, WebP),
write 16-bit PNG, choose PNG's compression, and write lossless WebP, and a
plugin's image components serve a still when named. With default settings
every still export P3 wrote is byte-identical (see Checks).

## Specification

- **Origins and ranking** (`celesta-plugin-host`, `celesta-media`).
  `MediaOrigin::BuiltIn` joins `Bundled`, `User`, and `Project`, labelled
  `built-in`. `celesta_plugin_host::FFMPEG_PLUGIN_ID = "ffmpeg"`.
  `celesta_media::default_rank(&RegisteredPlugin) -> Option<u8>`: built-in
  0, the bundled FFmpeg 1, other bundled 2, user 3, a project's `None`
  (never a default). `default_muxer` sorts by it (stable, so search order
  and manifest order hold within a rank), and `default_encoder` ranks the
  muxer's own plugin first, then `default_rank`. Decoders are tried user,
  the bundled FFmpeg, other bundled, then a project's (`decoder_order`);
  built-in components have no decoder. The editor's dialog ranks with
  `default_rank` too.
- **The built-in plugin** (`crates/media-image`, `celesta-media-image`, MIT
  OR Apache-2.0, a root-workspace member). `registered()` gives id
  `celesta_plugin_host::RESERVED_PLUGIN_ID` (`builtin`), version
  `CELESTA_VERSION`, origin `BuiltIn`, `directory: PathBuf::new()`, no
  `executable`, and a factory returning an in-process `ImageComponents`.
  Its manifest:
  - Video encoders `png` "PNG", `jpeg` "JPEG", `webp` "WebP", each with
    `codecs: [<id>]`; no `description`.
  - Muxers `image` "Image" and `imageSequence` "Image sequence", both with
    `extensions: ["png", "jpg", "jpeg", "webp"]`, `video: ["png", "jpeg",
    "webp"]`, `defaultVideo: "png"`, no `audio`, `trimsLeadingAudio:
    false`; `imageSequence` has `sequence: true`.
  - `describe` answers every component available, in process, with no
    `transports`. `inputFormats` are `[rgba16, rgba8]` for PNG and
    `[rgba8]` for JPEG and WebP. Muxers have no settings.
- **Settings**, exactly:
  - `builtin/png`: `compression` select `none` "None", `fastest`
    "Fastest", `fast` "Fast", `balanced` "Balanced", `high` "High"
    (label "Compression", default `balanced`, description "Every level is
    lossless; smaller files take longer to write."); `depth` select `8`
    "8 bits per channel", `16` "16 bits per channel" (label "Bit depth",
    default `8`); `transparent` boolean (label "Transparent background",
    default false, description "Leaves the background transparent instead
    of filling it.").
  - `builtin/jpeg`: `quality` number, integer, 1–100, step 1, default 90
    (label "Quality", description "Higher is sharper and larger.").
  - `builtin/webp`: `lossless` boolean (label "Lossless", default false);
    `quality` as JPEG's, `visibleWhen: { "lossless": false }`;
    `transparent` as PNG's.
- **Encoding.** PNG writes 8-bit or 16-bit RGBA with the `png` crate;
  `balanced` calls no `set_compression`, so a default file is P3's.
  16-bit input is little-endian `rgba16`, swapped to PNG's big-endian.
  JPEG writes RGB8 through `JpegEncoder::new_with_quality` (JFIF), dropping
  alpha. WebP writes `webp::Encoder::from_rgba(..).encode_simple(false,
  quality)`, or `encode_simple(true, 75.0)` when lossless (what
  `encode_lossless` runs, without its panic). Each frame is one packet:
  `pts`, `dts = pts`, `duration` the frame rate's denominator, `key`, the
  whole file; the first carries `TrackConfig { codec: "<png|jpeg|webp>" }`.
  `alpha` is `transparent` for PNG and WebP and false for JPEG. Open-time
  refusals (`MediaError::InvalidSettings`): `depth=16` without `rgba16`
  offered (key `depth`), 8-bit without `rgba8` offered (key `input`), and
  WebP over 16,383 pixels on a side (key `size`, P3's message "a {w}x{h}
  image is too large for WebP, which holds at most 16383 pixels on each
  side (write PNG or JPEG, or make the contact sheet smaller)").
- **Muxing.** Both muxers take exactly one video track whose codec family
  is the output extension's (`codec_for_extension`, ignoring case;
  `jpg` → `jpeg`); anything else is `InvalidSettings { key: "tracks" }`.
  `builtin/image` writes its one packet to `output` and refuses a second;
  `finish` with no packet is an error. `builtin/imageSequence` takes
  `output` as a pattern whose last path component holds exactly one
  `%06d` (`sequence_path`; the parent is kept byte for byte, and nothing
  else is parsed), writes each packet whole to frame `pts / denominator`
  before `writePackets` returns, and refuses a repeated or negative frame.
  `abortMuxer` lists the files written and removes none.
- **Exporter registries** (`celesta_exporter::with_builtin`). The
  exporter's registries (`media_registry`, which the CLI, MCP, and the
  editor use through `use_media`, and the `current()` fallback) put the
  built-in plugin first, once; a still export whose registry lacks it
  adds it. `serves_video`
  (public) is false for a `builtin` muxer and any `sequence: true` muxer.
  A video export never defaults to one, and a named or preset one is
  `component_unavailable` with "it writes still images; select frames to
  export stills" before rendering. The editor's Container list leaves them
  out, and its encoder lists follow the muxer, so the image encoders never
  show for a video container.
- **Stills** (`crates/exporter/src/still_output.rs`, `stills.rs`).
  `still_choice` turns the export's `OutputChoice` and `StillExport.quality`
  into a still's choice: a preset or any audio component or option is
  `invalid_selection` ("presets apply to video output, not still frames",
  "audio applies to video output, not still frames"); `quality` becomes
  the encoder's `quality` option, and both is "quality is given twice"; a
  named muxer must match the file count; with no muxer named it names
  `builtin/image` (one frame or a contact sheet) or `builtin/imageSequence`
  (several frames); with a built-in muxer, an extension it does not write
  is `invalid_selection` "still output must use the .png, .jpg, .jpeg or
  .webp extension", the encoder defaults to `builtin/<codec>`, and a named
  encoder of another codec is `incompatible_components` ("builtin/image
  writes .png files as png, and builtin/webp writes webp").
  `StillOutput::open` resolves through `resolve_preset_output` with no base
  (no presets file is ever read), describes the built-in in process and a
  named plugin through one lazily started export process, opens the
  encoder offering `[rgba16, rgba8]` (separate files) or `[rgba8]` (a
  contact sheet), checks the answer with the video path's `check_opened`,
  and only then stages the output, so a refusal at open (WebP too large,
  `depth=16` on a contact sheet) creates no folder. Errors from a still's
  encoder are filed under the stage `image encoding` (`IMAGE_STAGE`), the
  muxer's under `muxing`. The renderer is set from the answer as video's is
  (`render::configure_renderer`): `readback_for(opened)` and a transparent
  background when `alpha`, else the default background.
- **Staging and publishing.** One file is staged beside the output as
  `.celesta-<random>.<ext>` and published (no-clobber unless `overwrite`)
  when the muxer finishes. A sequence is staged in a hidden
  `.celesta-<random>/` folder beside the output with the pattern
  `frame-%06d.<ext>`; each frame whose packet the muxer has written is
  published to its final name (`name-000090.ext`) at once, so the
  `ExportProgress::Wrote` events follow the packets. Every final name is
  still checked for overwrite before the export. A failure or cancel ends
  the sessions, keeps the frames already published, and removes the staged
  folder; a one-file still writes nothing. A flush or write whose packets
  carry several frames publishes them one at a time and reports each one
  published (`Wrote`) even when a later one fails. A hard kill (SIGKILL,
  or SIGTERM without `--stdin-cancel`) can leave an empty or partial
  `.celesta-*/` staging folder beside a sequence, as a killed video export
  leaves its staged file. A selection like `--frames 3,0` sends the frames'
  `pts` in selection order, so a sequence muxer must not require increasing
  `pts`.
- **Straight `rgba8`.** An encoder answering `alpha: true` with `rgba8`
  gets straight alpha: the renderer reads back `Rgba16Straight` and
  `encoder_pixels` narrows each channel to 8 bits with the same 8x8 ordered
  dither the renderer's 8-bit readback uses (`narrow_rgba16`, a CPU copy of
  `dither.wgsl`'s `dither` and `code`: every channel, alpha too, gets the
  Bayer offset at the pixel's position in the frame, and a value within
  1/8 of a code keeps that code), in the still path and in `ExportSession`,
  so a transparent gradient does not band.
  Opaque frames keep `render()`'s RGBA8 readback, so their bytes are P3's.
  A contact sheet places straight tiles over its checkerboard as before.
- **Front ends.** CLI stills accept `--muxer`, `--video-encoder`, and
  `--video-option` (`STILL_COMPONENT_FLAGS`); `--preset`,
  `--preset-file`, `--audio-encoder`, and `--audio-option` stay "… applies
  to video output, not still frames"; `--quality` with `--video-option
  quality=` is a usage error, and `--quality` on a `.png` is refused only
  without `--video-encoder`. MCP's `render_frames` and `contact_sheet`
  take `muxer` and `videoEncoder` (`{ id?, options? }`, typed values;
  `quality` with `videoEncoder.options.quality` is refused), and
  `@celesta.studio/exporter`'s `exportStills` takes `muxer` and
  `videoEncoder`. `--list-components` and MCP's `list_components` show the
  built-ins as `built-in` with no folder; `--describe builtin/png` gives
  their settings.

## Implementation

| Commit | Task |
| --- | --- |
| `03a09586` | `MediaOrigin::BuiltIn`, `FFMPEG_PLUGIN_ID`, `default_rank`, the decoder order, and the editor's rank. |
| `c2ca205d` | `crates/media-image`: the encoders, the muxers, `codec_for_extension`, `sequence_path`. |
| `caab95e2` | `with_builtin`, `serves_video`, the video refusal, `check_opened` shared, the editor's container filter, the listings. |
| `b422a2d7` | `still_output.rs`; `stills.rs` loses its encoders; `celesta_remote::publish_path`; `png`, `image`, and `webp` move to the exporter's dev-dependencies. |
| `f558b847` | Straight `rgba8` (`readback_for`, `encoder_pixels`), and sequences publishing the frames of the packets written. |
| `a2a79905` | The CLI's, MCP's, and the TS package's still component options. |

## Deviations from the spec

Numbered from 50 (P2a used 8–18 and 24–26, P2b 19–23, 27, 28, P2c 30–31,
P3 40–44); recorded in the spec's Components, Muxing, Resolving an
output, CLI, Pixel formats and color, and Stills sections, the `rgba8` row,
and the Phases row's "As built".

50. **Built-in muxers and sequence muxers serve still exports only.** A
    video export's default muxer is never one of the `builtin` plugin's and
    never a `sequence: true` muxer, and a video export that names one fails
    before rendering with `component_unavailable` ("it writes still images;
    select frames to export stills"). The editor's dialog does not list
    them. `celesta-exporter p.json out.png` without `--frame` keeps P3's
    `unsupported_output` and its `--frame` hint, rather than rendering every
    frame into a one-frame muxer. Image sequences of a whole video are not
    built.
51. **Stills name their default components by extension, not by
    ranking.** A muxer has one `defaultVideo`, and `builtin/image` writes
    three formats, so the still path names `builtin/image` (one file) or
    `builtin/imageSequence` (several) and `builtin/<png|jpeg|webp>` for the
    output's extension, then resolves them with `resolve_preset_output`. A
    plugin's image muxer or encoder serves a still only when named, so the
    spec's "ranks after `builtin/image`" holds by construction.
52. **`rgba8` may carry straight alpha** (the question the spec left to
    P4). An encoder answering `alpha: true` may have chosen `rgba8` or
    `rgba16`; frames are then cleared to transparent, as for `rgba16`.
    `builtin/png` and `builtin/webp` take `transparent` (default false);
    JPEG has no alpha. A contact sheet shows transparent pixels over its
    existing checkerboard and is itself opaque. Every other encoder's
    `rgba8` stays opaque because it answers `alpha: false`.
53. **A sequence's files are published one at a time, and a failure or
    cancel keeps those already published.** Celesta stages a sequence in a
    hidden folder beside the output (`.celesta-<random>/`, removed when the
    export ends) and gives the muxer the pattern
    `<staging>/frame-%06d.<ext>`; only the last path component is a
    template, so user text in the output path is never parsed. A
    `sequence: true` muxer must have each frame's file whole when the
    `writePackets` carrying that frame's packet is answered, and Celesta
    publishes it to its final name then (no-clobber unless `overwrite`).
    The spec has Celesta remove the files it named on cancel; that would
    break the tested still behavior ("separate files stop at the failed
    frame, keeping those before it") and, under `--overwrite`, could only
    be done by deleting files the export never wrote. Celesta removes only
    its staged files.
54. **Frames reach built-in encoders as an owned `Vec<u8>`**, as
    `VideoEncoder::encode_video` takes them, not as borrowed slices: one
    copy of each still, negligible next to encoding it.
55. **Stills take `--muxer`, `--video-encoder`, and `--video-option`**
    (MCP's and `@celesta.studio/exporter`'s still calls take `muxer` and
    `videoEncoder`), so the built-in settings are reachable. `--quality`
    stays and sets the encoder's `quality` option; giving both is a usage
    error. `--preset`, `--preset-file`, and the audio flags stay
    video-only.
56. **Library-level quality and size errors become `invalid_option`.**
    `StillExport.quality` out of range or for PNG is validated against the
    encoder's schema, and WebP's 16,383-pixel limit is the encoder's
    `invalidSettings` at open (key `size`), both before rendering; they
    were `invalid_selection`. The CLI's and MCP's own checks of
    `--quality` / `quality` keep their usage messages.
57. **`builtin/png` also takes `depth`** (`"8"` or `"16"`, default `"8"`),
    since "rgba16 for 16-bit PNG" needs a setting to choose it. A contact
    sheet is offered `rgba8` only, so `depth=16` there fails with
    `invalid_option` before rendering.
58. **"FFmpeg first among bundled" is enforced by plugin id**
    (`celesta_plugin_host::FFMPEG_PLUGIN_ID = "ffmpeg"`), in
    `default_rank` and the decoder order; other bundled plugins keep search
    order after it.
59. **The built-in plugin has no folder.** Listings show its origin as
    `built-in` and no folder (`--list-components` text and JSON, MCP).

## Decisions

The spec left these to the plan:

- **Which muxer a still names:** one output file (one frame, or a contact
  sheet) → `builtin/image`; several → `builtin/imageSequence`. A named
  muxer must match: a `sequence: true` muxer for several files, any other
  for one; otherwise `invalid_selection` ("`{muxer}` writes one image;
  several frames need a sequence muxer such as builtin/imageSequence" /
  "`{muxer}` writes image sequences; one image needs a muxer such as
  builtin/image").
- **Extension checks:** an extension no built-in muxer lists keeps P3's
  `invalid_selection` text, built from the built-in muxers' extensions,
  with no muxer named or a built-in one named (see Rulings below). A
  built-in muxer with an encoder whose codec is not the extension's is
  `incompatible_components` before rendering; the muxer refuses the same
  mismatch at open as a backstop.
- **Formats offered to a still encoder:** `[rgba16, rgba8]` for separate
  files, `[rgba8]` for a contact sheet. No YUV: image encoders keep RGB.
- **The renderer for stills** is set from the encoder's answer as video's
  is, through `render::configure_renderer`, shared with
  `open_video_output`.
- **JPEG drops alpha** rather than flattening: it answers `alpha: false`,
  so its frames are rendered opaque and dropping the channel gives P3's
  bytes.
- **Built-in settings and open-time refusals** as in Specification above.
- **Stills never read a presets file:** the still path never calls
  `resolve_output`, only `resolve_preset_output` with no base.

## Rulings

From the session's ledger; each with what it costs if it is wrong.

1. `StillOutput::open` takes the registry first and the export's output
   path besides the plan's parameters; the exporter runs `ensure_media`,
   `current()`, and `with_builtin`. If wrong, a signature change in one
   crate-private type.
2. The still path reuses the video path's `check_opened` (the input among
   those offered; alpha only with `rgba8` or `rgba16`), extracted from
   `encode.rs`, rather than a copy. If wrong, a small refactor.
3. **Both built-in muxers refuse a codec that is not the output
   extension's**, `builtin/imageSequence` too (the plan called it a
   backstop for `builtin/image`). If wrong, a plugin-free sequence of
   another codec cannot be written, which nothing needs.
4. **A single still's staged file keeps the output's extension**
   (`.celesta-XXXX.png`) instead of the plan's `.tmp` suffix, since the
   muxers refuse a codec/extension mismatch (ruling 3); an output with no
   extension stages as `.tmp`. If wrong, only the hidden name differs.
5. **A one-file still cancelled after its frame rendered writes nothing**:
   it is published at finish, after the cancel check. P3 wrote the file and
   then reported `cancelled`. A cancel that leaves no output is the
   documented intent ("a failed or cancelled export leaves no partial
   file"). If wrong, the file must be published before the cancel check.
6. **Straight `rgba8` is narrowed on the CPU**: an encoder keeping alpha in
   `rgba8` gets frames read back as `Rgba16Straight` and narrowed with the
   8-bit readback's ordered dither (same matrix, offset, tolerance, and
   rounding as `dither.wgsl`, by pixel position), in the still and video
   paths alike, because the renderer's
   8-bit readback is premultiplied only. The GPU code and opaque output are
   untouched. If wrong, a GPU straight 8-bit readback replaces the
   narrowing later, saving some CPU.

Also accepted from the task reviews: the extension check applies to a
named built-in muxer too, so `--muxer builtin/image out.gif` fails before
rendering; a preset-chosen still muxer is refused after `resolve_output`
has run the in-process built-in `describe` (no process starts); and
`--quality` with a named encoder reaches that encoder's options, whatever
it is.

## Docs

- The user guide's export page (en and ja): "Still frames and contact
  sheets" names the built-in components and says stills need no media
  plugin, a table of the built-in settings (`compression`, `depth`,
  `transparent`, `quality`, `lossless`) with `--describe builtin/png`,
  `--quality` against `--video-option quality=`, transparent stills
  (straight alpha, JPEG has none, the contact sheet's checkerboard), that a
  WebP over 16383 pixels fails with `invalid_option`, naming a plugin's
  image components with `--muxer` and `--video-encoder` and the one-image
  and sequence muxers, that stills take no preset or audio flags, that
  separate files already written stay on failure or cancel while a
  cancelled single image writes nothing, and MCP's and TypeScript's
  `muxer` and `videoEncoder`; a "Export a transparent PNG" example in the
  existing tab group (the website's tab counts are unchanged); and
  "Choosing encoders and settings" says `--list-components` lists the
  built-ins as `built-in`.
- The Agent Skill: `references/export.md` (the built-in components per
  extension, their settings' keys and defaults, `--describe builtin/png`,
  transparent, 16-bit, and lossless examples, plugin image components,
  no presets for stills, failure and cancel, MCP's and TypeScript's
  `muxer` and `videoEncoder`, and the `--muxer`, `--video-option`, and
  `--quality` rows) and `references/errors.md` (the WebP size message is
  `invalid_option`).
- HANDOFF.md: a `celesta-media-image` row, the `celesta-exporter` and
  `@celesta.studio/exporter` rows, and a stills bullet in Renderer and
  export constraints.
- The spec: deviations 50–59 and rulings 3–6 under Components, Muxing,
  Resolving an output, CLI, Pixel formats and color (and the `rgba8` row),
  and Stills; the Phases row's "As built".
- Examples: unchanged. No example exports stills with settings P4 adds,
  and none is superseded.

## Checks

On macOS 26.6.2, Apple M4, Metal, with `PKG_CONFIG_PATH` at Homebrew's
`ffmpeg@8`, at `a2a79905` plus this note's commit. A GPU adapter was
available, so the live still tests ran.

**Against P3, byte for byte.** P3's exporter was built in a detached
worktree of `8c0ad423` (`claude/media-components-p3`) in the session's
scratch folder (`git worktree add --detach <scratch>/p3 8c0ad423`, then
`cargo build --release -p celesta-exporter`, and `pnpm install && pnpm
run build:runtime` there for its own `packages/cli/dist`), and this
branch's with the same command. Both ran from this worktree on the same
sources, into separate folders, with `--json`:

```sh
# S is the source; OUT the binary's own folder
celesta-exporter --json [--react] $S --frame 0 $OUT/a.png
celesta-exporter --json [--react] $S --frames 0,12,599 $OUT/b.png     # React: --frames 0,12
celesta-exporter --json [--react] $S --every 15 $OUT/c.jpg --quality 70
celesta-exporter --json [--react] $S --every 30 $OUT/d.webp
celesta-exporter --json [--react] $S --every 10 --contact-sheet $OUT/e.png
celesta-exporter --json [--react] $S --every 10 --contact-sheet $OUT/f.webp
```

- `examples/editor-demo.celesta.json` (JSON, 1280×720, 60 fps, 600
  frames): 68 files from each binary (`a.png`, 3 `b-*.png`, 41 `c-*.jpg`,
  21 `d-*.webp`, `e.png`, `f.webp`), the same names, and `cmp` finds all
  68 identical.
- `examples/path-motion/film.tsx` (React, 1920×1080, 30 fps, 180 frames):
  25 files from each (`a.png`, 2 `b-*.png`, 13 `c-*.jpg`, 7 `d-*.webp`,
  `e.png`, `f.webp`), the same names, and `cmp` finds all 25 identical.
- The 12 `--json` reports are identical once each binary's output folder
  is replaced by one name and `stats` (elapsed time, render rate) is
  removed: the same `outputs` in the same order, `composition`, and
  `warnings`.
- The brief's `--frames 0,12,-1` is not a valid selection on either
  branch: both reject it identically (`usage`, "invalid value '-1' for
  '--frame <N,...>': invalid digit found in string"), so the JSON project
  used its last frame, 599.

The P3 worktree was removed afterwards (`git worktree remove`).

**Full verification.** The JS packages were built first (`pnpm install`, `pnpm
run build:runtime`), so `packages/cli/dist` was whole and the React and
MCP live tests ran; a GPU adapter was available, so none of the GPU tests
returned early.

- `cargo build -p celesta-codec-test`, then `cargo test --workspace
  --no-fail-fast`: 2,757 passed, 0 failed, 1 ignored as before
  (`builtin_effect::write_web_effect_fixtures`). MCP's
  `server::init_project_creates_a_project`, which failed in Tasks 4 and 5
  on a partly built `packages/cli/dist`, passes with the runtime built.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` and
  `cargo fmt --all -- --check`: clean.
- `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`
  (468 passed, 0 failed) and `cargo clippy --manifest-path
  plugins/Cargo.toml --all-targets --locked -- -D warnings` (clean). The
  first run failed under `--locked`: `plugins/ffmpeg` depends on
  `celesta-exporter` by path, and `plugins/Cargo.lock` still listed the
  exporter's old `image`, `png`, and `webp` dependencies and lacked
  `celesta-media-image` (Tasks 2 and 4 updated only the root
  `Cargo.lock`). The fix round regenerated it offline (`cargo metadata
  --manifest-path plugins/Cargo.toml --offline`, a 15-line diff) and
  committed it.
- `pnpm run test:runtime` (17 packages, 1,046 tests, all passed;
  `@celesta.studio/cli` 119, `@celesta.studio/components` 179),
  `pnpm --dir packages/exporter test` (25 passed), `pnpm --dir
  packages/website test` (52 passed), and `git diff --check`: clean. No
  test was skipped.

This changes no rendering of an opaque export (the comparison above is
byte-identical), so there is no before/after screenshot pair (AGENTS.md);
transparent and 16-bit stills are new output, not a changed scene.

Unit and integration tests cover the rest (Tasks 1–5): the ranking and the
decoder order (`ffmpeg_ranks_first_among_bundled`,
`ffmpeg_is_the_bundled_default`, `built_in_encoders_rank_before_bundled`);
the crate's 14 tests (`default_png_is_todays_png`,
`default_jpeg_and_webp_are_todays`, `png_takes_sixteen_bits_and_compression`,
`transparent_keeps_alpha`, `webp_lossless_is_vp8l`,
`webp_refuses_images_too_large`, `image_writes_one_frame`,
`the_sequence_writes_each_frame_as_it_comes`,
`the_pattern_is_only_the_last_component`, and more); the video refusals
(`a_video_never_defaults_to_an_image_muxer`,
`a_video_naming_a_still_muxer_is_refused`, `still_muxers_are_not_containers`,
`stills_need_no_media_plugin`, `built_ins_are_listed_without_a_folder`);
`still_output`'s unit tests (`names_the_built_ins_by_extension`,
`quality_becomes_the_encoders_option`, `presets_and_audio_do_not_apply`,
`a_named_muxer_must_match_the_file_count`,
`a_mismatched_built_in_codec_is_incompatible`,
`png_quality_is_an_invalid_option`, `webp_too_large_fails_before_rendering`,
`sixteen_bit_png_is_not_for_contact_sheets`, `jpeg_stills_are_opaque`,
`a_sequence_publishes_each_frame_as_it_is_written`,
`an_aborted_sequence_keeps_published_frames_and_no_staging`,
`a_sequence_frame_never_clobbers_unless_asked`,
`a_held_back_frame_is_published_under_its_own_name`,
`a_refusal_at_open_creates_no_folder`,
`a_still_encoder_error_names_image_encoding`,
`a_flush_failing_partway_keeps_the_frames_it_published`); straight `rgba8`
(`straight_rgba16_narrows_to_rgba8_keeping_whole_codes`,
`narrowing_uses_the_renderers_8x8_bayer_dither`,
`a_narrowed_ramp_does_not_band`,
`an_rgba8_encoder_keeping_alpha_gets_straight_8_bit_frames`); the live
stills (`a_project_default_preset_never_reaches_stills`,
`transparent_png_and_webp_keep_alpha`, `sixteen_bit_png`,
`a_failed_sequence_keeps_written_files_and_no_staging`,
`a_cancelled_sequence_leaves_later_names_alone`,
`percent_signs_in_the_output_name_are_literal`); and the front ends
(`stills_take_video_options`, MCP's `still_tools_take_components`, and the
TS package's stills arguments).

## Not done

- **P2c's rebase.** When P2c (BT.709) lands in the stack, P3 and then this
  branch are rebased onto it (never merged); P4 touches neither
  `readback_of` nor `PixelFormat::OFFERABLE`, so expect conflicts only in
  docs and the spec's deviation lists.
- **The built-in crate.**
  - `imageSequence`'s frame is `pts / duration`, truncating; that `pts` is
    a multiple of the frame rate's denominator is assumed, not documented.
  - A sequence frame is recorded as written before its write succeeds.
  - No test for `writePackets` with a track other than 0, or a zero-sized
    frame.
- **Ranking.** `decoder_order` and `default_rank` repeat the origin and
  FFmpeg-id match; no editor model test pins the new rank order.
- **Video refusals.** `built_ins_are_listed_without_a_folder` checks the
  JSON listing only, not the text line; no unit test for the preset-chosen
  still-muxer refusal.
- **Stills.**
  - A named plugin muxer's process may first start at the first write,
    after rendering, when its `describe` is cached.
  - One-file publish and finish errors no longer carry `.at_frame(frame)`;
    the one-file finish reports frame 0 when nothing was muxed
    (unreachable), and a negative `pts` maps to frame 0 in `mux`.
  - Error precedence changed: `output_exists` now comes before the
    extension message for `check.gif`.
  - The test `rejects_empty_duplicate_and_non_png_selections` no longer
    tests a non-PNG output.
  - `--frames 3,0` sends decreasing `pts` to the encoder and muxer; the
    built-ins accept it, and the spec (deviation 53) now tells a plugin's
    sequence muxer to; nothing checks a plugin does.
  - The one-file staged name `.celesta-XXXX.png` matches `*.png` globs
    during the render (`staged.rs` documents `.tmp` for that reason).
  - "renderer read back no frame" is filed under the image stage's `codec`
    rather than `render`.
  - The video-path alpha test calls `encode_readback`, not `encode_frame`.
- **Front ends.** `component_flags_need_a_video_output` narrows by string munging; no test
  for `--quality` with `--video-encoder` on a `.png`, MCP's named-encoder
  PNG relaxation, or `--preset-file` for stills; MCP repeats
  `encoder.id.is_some()` in both still jobs.
- **Not built:** image sequences of a whole video (deviation 50), and a GPU
  straight 8-bit readback (ruling 6).
