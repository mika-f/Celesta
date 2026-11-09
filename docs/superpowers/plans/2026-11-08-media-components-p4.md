# Media components P4: stills — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Still exports (`--frame`, `--every`, contact sheets, and the
image sequences several frames make) write through media components:
built-in, in-process video encoders `builtin/png`, `builtin/jpeg`,
`builtin/webp` and muxers `builtin/image`, `builtin/imageSequence` in a new
crate `celesta-media-image`, chosen and validated through the same
`resolve_preset_output` as video, with byte-identical files for every
still export the spec does not change, and `cargo install
celesta-exporter` still exporting stills with nothing else installed.

**Architecture:**
- **Origins and ranking** (`celesta-plugin-host`, `celesta-media`).
  `MediaOrigin::BuiltIn`, and one ranking function for defaults
  (`default_rank`: built-in, then bundled with FFmpeg first, then user,
  never project) that `resolve_output`'s defaults and the editor's dialog
  share; decoders keep user → bundled → project, with FFmpeg first among
  bundled.
- **The built-in components** (new `crates/media-image`). One in-process
  `MediaProvider` (`ImageComponents`) serving three video encoders and two
  muxers, its manifest, and `registered()`, the `RegisteredPlugin` with the
  reserved id `builtin`. Encoders return one key packet per frame (the
  whole image file); `image` writes one packet to one file;
  `imageSequence` writes each packet to the file its pattern names for
  the packet's frame.
- **Registering them** (`celesta-exporter`). Every registry the exporter
  makes starts with the built-in plugin. Video exports never use a
  built-in or sequence muxer (deviation 50); the editor's dialog leaves
  them out.
- **Stills through components** (`celesta-exporter`, new
  `still_output.rs`). `stills.rs` keeps the frame selection, the render
  loop, and the contact sheet; its `Encoder` is replaced by a
  `StillOutput` that names the built-in components by the output's
  extension (or takes the named ones), resolves them with
  `resolve_preset_output` (no preset is ever read), opens the encoder
  before rendering, configures the renderer from the encoder's answer,
  and stages and publishes every file.
- **Front ends.** The CLI, MCP, and `@celesta.studio/exporter` let stills
  name a muxer and a video encoder and set its options, so PNG
  compression, 16-bit PNG, transparency, and lossless WebP are reachable.

**Tech stack:** Rust 2024 (MSRV 1.89); `png 0.18.1`, `image 0.25.8`
(`jpeg` feature only), `webp 0.3.1` (no default features), all three moved
from `celesta-exporter` to `celesta-media-image`; `tempfile 3.23`;
TypeScript with Vitest for `packages/exporter`.

**Spec:** `docs/superpowers/specs/2026-11-08-media-components-design.md`
(Phases → P4; Stills and sequences (P4); Components — built-in components
and the reserved `builtin` id; Muxing — sequence muxers, naming, overwrite
checks, cleanup; Selection — ranking; Pixel formats and color — Alpha and
"an image encoder follows its format's own convention"; Testing). Read it
with this plan. P2a's and P3's plans and handoff notes
(`docs/handoff/2026-11-08-media-encoders.md`,
`docs/handoff/2026-11-09-export-presets-dialog.md`) define
`resolve_preset_output`, `OutputChoice`, `SettingInput`, the provider
traits, and the error codes used here.

## Global constraints

- This PR stacks on P3's (`claude/media-components-p3`, mika-f/Celesta#794,
  tip `8c0ad423`). P2c (BT.709) is on `claude/media-components-p2c`; when
  it lands in the stack, P3 and then this branch are **rebased** onto it
  (never merged). P2c makes `nv12`/`p010` offerable and changes
  `readback_of` and `PixelFormat::OFFERABLE`; this plan touches neither,
  so expect conflicts only in docs and the spec's deviation lists.
- A separate session plans P5 (macOS AVFoundation) on its own branch. P5
  relies on Task 1's ranking (FFmpeg first among bundled); keep
  `default_rank`'s signature as written here so P5 can use it.
- Every name P2a–P3 define is used as spelled at `8c0ad423`; where this
  plan changes one, it says so.
- The built-in plugin: id `celesta_plugin_host::RESERVED_PLUGIN_ID`
  (`"builtin"`, already refused for plugin manifests), version
  `celesta_plugin_host::CELESTA_VERSION`, origin `MediaOrigin::BuiltIn`,
  `directory: PathBuf::new()`, `executable: None`.
- Component ids and names, exactly: video encoders `png` "PNG", `jpeg`
  "JPEG", `webp` "WebP"; muxers `image` "Image" and `imageSequence`
  "Image sequence", both with `extensions: ["png", "jpg", "jpeg", "webp"]`,
  `video: ["png", "jpeg", "webp"]`, `defaultVideo: "png"`, no `audio`,
  `trimsLeadingAudio: false`; `imageSequence` has `sequence: true`.
  Codec strings (and families) `png`, `jpeg`, `webp`; no `description`.
- Built-in encoders follow their format's convention, not `Color`
  (spec, Pixel formats and color): PNG and WebP keep sRGB RGB(A); JPEG
  is JFIF (full-range BT.601, as the `image` crate writes it).
- **Byte-identical output.** With default settings, every still export
  that worked on P3 writes the same bytes: 8-bit RGBA PNG at
  `png::Compression::Balanced` (the crate default, today's), JPEG through
  `JpegEncoder::new_with_quality` from RGB8, WebP through
  `webp::Encoder::from_rgba(..).encode_simple(false, quality)`. The same
  names (`name-000090.ext`, the output itself for one frame), the same
  `ExportProgress::Wrote` events in selection order, and the same JSON
  report.
- Stills never read a presets file: no `--preset`, no project `default`,
  no `last` (the CLI already passes `PresetLookup::default()` for stills;
  the still path never calls `resolve_output`, only
  `resolve_preset_output` with no base).
- JS/TS tests use Vitest in TypeScript; Rust tests stay in Rust (AGENTS.md).
- Never amend commits; each task ends in a new commit. Stage files
  explicitly; never commit `.superpowers/`, `plugins/dist`, or scratch
  files. Commits are signed through 1Password; if signing fails with
  "failed to fill whole buffer", stop and ask the user to unlock
  1Password — never bypass signing. **Every commit message ends with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`**, whatever
  model wrote it.
- Builds: `export PATH="/Users/natsuneko/Library/Application Support/mbx/bin:$HOME/.cargo/bin:$HOME/.local/share/mise/installs/node/26/bin:$HOME/.local/share/mise/installs/pnpm/12/bin:$PATH"`
  and `export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"`
  (Homebrew's default `ffmpeg` is 9 and breaks the build). Workspace tests
  need `cargo build -p celesta-codec-test` first.
- **Builds between tasks.** Task 1 adds an enum variant and fixes every
  exhaustive match in the same commit. Task 2 is a new crate nothing uses
  yet. Task 3 registers it (still exports keep their own encoders until
  Task 4). Task 4 removes the exporter's `png`/`image`/`webp`
  dependencies (they stay as dev-dependencies where tests decode images).

## Deviations from the spec (recorded in Task 6's spec edit)

Numbered from 50 (P2a used 8–18 and 24–26, P2b 19–23, 27, 28, P2c 30–31,
P3 40–44).

50. **Built-in muxers and sequence muxers serve still exports only.** A
    video export's default muxer is never one of the `builtin` plugin's
    and never a `sequence: true` muxer, and a video export that names one
    fails before rendering with `component_unavailable` ("it writes still
    images; select frames to export stills"). The editor's dialog does not
    list them. `celesta-exporter p.json out.png` without `--frame` keeps
    today's `unsupported_output` and its `--frame` hint, rather than
    rendering every frame into a one-frame muxer. Image sequences of a
    whole video are not built.
51. **Stills name their default components by extension, not by
    ranking.** A muxer has one `defaultVideo`, and `builtin/image` writes
    three formats, so the still path names `builtin/image` (one file) or
    `builtin/imageSequence` (several) and `builtin/<png|jpeg|webp>` for
    the output's extension (`jpg` → `jpeg`), then resolves them with
    `resolve_preset_output`. A plugin's image muxer or encoder serves a
    still only when named (`--muxer`, `--video-encoder`), so the spec's
    "ranks after `builtin/image`" holds by construction.
52. **`rgba8` may carry straight alpha** (the question the spec left to
    P4). An encoder answering `alpha: true` may have chosen `rgba8` or
    `rgba16`; frames are then cleared to transparent, as for `rgba16`.
    `builtin/png` and `builtin/webp` take `transparent` (default false);
    JPEG has no alpha. A contact sheet shows transparent pixels over its
    existing checkerboard and is itself opaque. Every other encoder's
    `rgba8` stays opaque because it answers `alpha: false`.
53. **A sequence's files are published one at a time, and a failure or
    cancel keeps those already published.** Celesta stages a sequence in
    a hidden folder beside the output (`.celesta-<random>/`, removed when
    the export ends) and gives the muxer the pattern
    `<staging>/frame-%06d.<ext>`; only the last path component is a
    template, so user text in the output path is never parsed. A
    `sequence: true` muxer must have each frame's file whole when the
    `writePackets` carrying that frame's packet is answered, and Celesta
    publishes it to its final name then (no-clobber unless `overwrite`).
    The spec has Celesta remove the files it named on cancel; that would
    break today's tested still behavior ("separate files stop at the
    failed frame, keeping those before it") and, under `--overwrite`,
    could only be done by deleting files the export never wrote. Celesta
    removes only its staged files.
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
    `StillExport.quality` out of range or for PNG is validated against
    the encoder's schema, and WebP's 16,383-pixel limit is the encoder's
    `invalidSettings` at open (key `size`), both before rendering; they
    were `invalid_selection`. The CLI's and MCP's own checks of
    `--quality` / `quality` keep their usage messages.
57. **`builtin/png` also takes `depth`** (`"8"` or `"16"`, default
    `"8"`), since "rgba16 for 16-bit PNG" needs a setting to choose it. A
    contact sheet is offered `rgba8` only, so `depth=16` there fails with
    `invalid_option` before rendering.
58. **"FFmpeg first among bundled" is enforced by plugin id**
    (`celesta_plugin_host::FFMPEG_PLUGIN_ID = "ffmpeg"`), in
    `default_rank` and the decoder order; other bundled plugins keep
    search order after it.
59. **The built-in plugin has no folder.** Listings show its origin as
    `built-in` and no folder (`--list-components` text and JSON, MCP).

## Decisions the spec left to this plan

- **Which muxer a still names:** one output file (one frame, or a contact
  sheet) → `builtin/image`; several → `builtin/imageSequence`. A named
  muxer must match: a `sequence: true` muxer for several files, any other
  for one; otherwise `invalid_selection`
  ("`{muxer}` writes one image; several frames need a sequence muxer
  such as builtin/imageSequence" / "`{muxer}` writes image sequences;
  one image needs a muxer such as builtin/image").
- **Extension checks:** with no muxer named, an extension no built-in
  muxer lists is today's `invalid_selection` "still output must use the
  .png, .jpg, .jpeg or .webp extension" (built from the built-in muxers'
  extensions, so the text stays exactly that). A built-in muxer with an
  encoder whose codec is not the extension's (`--video-encoder
  builtin/webp out.png`) is `incompatible_components` ("builtin/image
  writes .png files as png, and builtin/webp writes webp") before
  rendering; the muxer refuses the same mismatch at open as a backstop.
- **Formats offered to a still encoder:** `[rgba16, rgba8]` for separate
  files, `[rgba8]` for a contact sheet. No YUV: image encoders keep RGB.
- **The renderer for stills** is set from the encoder's answer as video's
  is: `readback_of(input)` and a transparent background when `alpha`,
  else `GpuRenderOptions::default().background`
  (`render::configure_renderer`, shared with `open_video_output`).
- **JPEG drops alpha** rather than flattening: an encoder answering
  `alpha: false` gets opaque frames (the renderer clears to the opaque
  background), so dropping it gives today's bytes.
- **Built-in settings**, exactly (keys, types, defaults):
  - `builtin/png`: `compression` select `none` "None", `fastest`
    "Fastest", `fast` "Fast", `balanced` "Balanced", `high` "High"
    (default `balanced`; description "Every level is lossless; smaller
    files take longer to write."); `depth` select `8` "8 bits per
    channel", `16` "16 bits per channel" (default `8`, label "Bit
    depth"); `transparent` boolean, label "Transparent background",
    default false, description "Leaves the background transparent instead
    of filling it."
  - `builtin/jpeg`: `quality` number, `integer`, min 1, max 100, step 1,
    default 90, label "Quality", description "Higher is sharper and
    larger."
  - `builtin/webp`: `lossless` boolean, label "Lossless", default false;
    `quality` as JPEG's with `visibleWhen: { "lossless": false }`;
    `transparent` as PNG's. Lossless encodes with
    `webp::Encoder::encode_lossless`.
  - `describe`: every component available; `inputFormats` `[rgba16,
    rgba8]` for PNG, `[rgba8]` for JPEG and WebP; no `transports` (in
    process); muxers and encoders describe their `settings` only (muxers
    have none).
- **Open-time refusals** (`MediaError::InvalidSettings`): PNG `depth=16`
  without `rgba16` offered (key `depth`, "16-bit PNG needs rgba16 input,
  which this export does not offer"), any encoder without `rgba8` offered
  for 8-bit (key `input`), WebP wider or taller than 16,383 (key `size`,
  today's text "a {w}x{h} image is too large for WebP, which holds at most
  16383 pixels on each side (write PNG or JPEG, or make the contact sheet
  smaller)").

## Review focus

The tests that pin each of these are in the task named.

1. **A project whose `export-presets.json` has a `default`** (say, MP4)
   still writes PNG for `--frame 0 out.png`, with no preset warning
   (Task 4, `a_project_default_preset_never_reaches_stills`).
2. **A sequence that fails or is cancelled part-way** keeps the files
   already written, leaves no `.celesta-*` folder or file beside the
   output, and under `--overwrite` leaves a pre-existing file at a later,
   unwritten name untouched (Task 4,
   `a_failed_sequence_keeps_written_files_and_no_staging`,
   `a_cancelled_sequence_leaves_later_names_alone`).
3. **Output paths with `%` or a non-UTF-8 parent** (`50%-check.png` in
   `run%d/`) write the same names as before (Task 2,
   `the_pattern_is_only_the_last_component`; Task 4,
   `percent_signs_in_the_output_name_are_literal`).
4. **A video export to an image extension** (`out.png`, no `--frame`)
   fails before rendering with `unsupported_output` and the `--frame`
   hint, and `--muxer builtin/image` for a video with
   `component_unavailable` (Task 3,
   `a_video_never_defaults_to_an_image_muxer`,
   `a_video_naming_a_still_muxer_is_refused`).
5. **No media plugin at all** (`cargo install`, an empty plugin path):
   stills export and `--list-components` lists the built-ins as
   `built-in`; and a bundled plugin sorted before `ffmpeg` by path does
   not take `.mp4`'s default (Task 3,
   `stills_need_no_media_plugin`, `built_ins_are_listed_without_a_folder`;
   Task 1, `ffmpeg_ranks_first_among_bundled`).

---

### Task 1: Built-in origin and one default ranking

**Files:**
- Modify: `crates/plugin-host/src/media.rs` (`MediaOrigin::BuiltIn`, `FFMPEG_PLUGIN_ID`), `crates/plugin-host/src/lib.rs` (re-export)
- Modify: `crates/media/src/registry.rs` (`default_rank`, decoder order), `crates/media/src/lib.rs` (re-export)
- Modify: `crates/exporter/src/resolve.rs` (`default_muxer`, `default_encoder` use `default_rank`), `crates/exporter/src/components.rs` (`origin_label`), `crates/editor/src/export_dialog/model.rs` (`rank`)
- Test: `crates/media/src/registry.rs` tests, `crates/exporter/src/resolve.rs` tests, `crates/editor/src/export_dialog/model.rs` tests

**Interfaces:**
- Produces:
  ```rust
  // celesta_plugin_host
  pub enum MediaOrigin { User, Bundled, Project, /// Celesta's own, in its process (`builtin/png`).
      BuiltIn }                                  // appended: the derived order is not used for ranking any more
  pub const FFMPEG_PLUGIN_ID: &str = "ffmpeg";

  // celesta_media (registry.rs, re-exported)
  /// Where `plugin`'s components rank among defaults, lowest first: built-in 0, the bundled
  /// FFmpeg 1, other bundled 2, user 3; `None` for a project's, which is never a default.
  pub fn default_rank(plugin: &RegisteredPlugin) -> Option<u8>;
  ```
- Decoders are tried user (search order) → bundled (FFmpeg, then search
  order) → project (search order); built-in plugins have none. Replace
  `decoders.sort_by_key(origin)` with an explicit stable key.
- `default_muxer`: candidates ranked by `default_rank` (stable within a
  rank). `default_encoder`: the muxer's own plugin 0, else
  `default_rank + 1`, never a project's unless it is the muxer's.
- `origin_label(BuiltIn) == "built-in"`. Editor `rank`: `default_rank`,
  with a project's last (4).

- [ ] **Step 1: Write failing tests.**
  - registry: `ffmpeg_ranks_first_among_bundled` — bundled `avfoundation` then bundled `ffmpeg` (search order), user `u`, project `p`, built-in `builtin`: `default_rank` gives `ffmpeg` 1, `avfoundation` 2, `u` 3, `builtin` 0, `p` None; `decoder_candidates` for `a.mp4` (all four declare `mp4`, built-in has none) is `u`, `ffmpeg`, `avfoundation`, `p`.
  - resolve: `ffmpeg_is_the_bundled_default` — bundled `avfoundation` (muxer `mp4` for `mp4`, encoder `h264` writing `avc1`) registered before bundled `ffmpeg` (same): `.mp4` resolves `ffmpeg/mp4` and `ffmpeg/...`'s encoder; naming `avfoundation/mp4` still works.
  - resolve: `built_in_encoders_rank_before_bundled` — a user muxer `u/img` taking `png` with the built-in `png` encoder and a bundled one both writing `png`: the default video encoder is the built-in one (the muxer's own plugin has none).
  - components: `origin_label(MediaOrigin::BuiltIn) == "built-in"`.
- [ ] **Step 2:** `cargo test -p celesta-media registry` and `cargo test -p celesta-exporter resolve` fail.
- [ ] **Step 3: Implement.** Every exhaustive `match` on `MediaOrigin` (resolve, components, editor model, plugin-host's `From<PluginSource>` needs none) handles `BuiltIn`.
- [ ] **Step 4:** `cargo test -p celesta-media -p celesta-exporter -p celesta-editor` and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.
- [ ] **Step 5: Commit:** `feat(media): a built-in origin, and FFmpeg first among bundled defaults`.

### Task 2: `celesta-media-image`, the built-in image components

**Files:**
- Create: `crates/media-image/Cargo.toml` (`celesta-media-image`, description "Celesta's built-in image components: PNG, JPEG and WebP encoders and the image and image-sequence muxers.", license and version from the workspace), `crates/media-image/src/lib.rs`, `crates/media-image/src/encoders.rs`, `crates/media-image/src/muxers.rs`
- Modify: `Cargo.toml` (workspace `members`, `[workspace.dependencies] celesta-media-image = { path = "crates/media-image", version = "=0.9.0" }`), `Cargo.lock`
- Test: unit tests in each new file

**Interfaces:**
- Consumes: Task 1's `MediaOrigin::BuiltIn`; `celesta_media`'s `MediaProvider`, `VideoEncoder`, `Muxer`, `RegisteredPlugin`, `ComponentDescription`, `SettingField`, `Settings`, `PixelFormat`, `Packet`, `Encoded`, `TrackConfig`, `Track`, `TrackPacket`, `SessionId`, `MediaError`.
- Produces, in `celesta_media_image`:
  ```rust
  pub fn manifest() -> MediaManifest;              // Global constraints' components
  pub fn registered() -> RegisteredPlugin;         // id builtin, factory → Box::new(ImageComponents::default()), ignores the kill switch and use
  #[derive(Default)] pub struct ImageComponents { /* one session counter for every kind */ }
  impl MediaProvider for ImageComponents { describe, video_encoder, muxer }   // decoder/audio/pipeline stay None
  impl VideoEncoder for ImageComponents; impl Muxer for ImageComponents;
  /// The built-in encoder (and codec family) for `extension`, ignoring case: png, jpg|jpeg → jpeg, webp.
  pub fn codec_for_extension(extension: &str) -> Option<&'static str>;
  /// The file `pattern` names for `frame`: its last component's one `%06d` replaced by the
  /// zero-padded frame number; the rest of the path is literal. MediaError::Codec when the last
  /// component is not UTF-8 or does not hold exactly one `%06d`.
  pub fn sequence_path(pattern: &Path, frame: u64) -> Result<PathBuf, MediaError>;
  ```
- Encoders: `open_video` gets settings Celesta already validated; it
  chooses the input (Decisions), answers `alpha` = `transparent` (false
  for JPEG), and refuses as in Decisions. `encode_video(session, pts,
  frame)` returns `Encoded { packets: [Packet { pts, dts: pts, duration:
  frame_rate.denominator, key: true, bytes: <the whole file>, additional:
  [] }], config: Some(TrackConfig { codec: "<family>", description: None })
  }` on the first frame, `config: None` after; `finish_video` returns no
  packets. 16-bit PNG swaps each little-endian `u16` to big-endian.
- Muxers: `open_muxer` takes exactly one video track whose family is
  `png`, `jpeg`, or `webp` (anything else, an audio track, or for `image`
  a codec that is not `codec_for_extension(output's extension)`:
  `MediaError::InvalidSettings { key: "tracks", … }`). `image`: the first
  packet's bytes become `output` (`File::create`, `write_all`,
  `sync_all`); a second packet is `MediaError::Codec("builtin/image writes
  one frame")`; `finish_muxer` with no packet written is
  `Codec("builtin/image was given no frame")`, else `Ok(vec![output])`.
  `imageSequence`: each packet's frame is `pts / frame_rate.denominator`
  (the video track's), written whole to `sequence_path(output, frame)`
  before `write_packets` returns (deviation 53); a frame written twice is
  `Codec`; `finish_muxer` returns the files in the order written.
  `abort_muxer` returns the files written so far and removes none.

- [ ] **Step 1: Write failing tests** (`#[cfg(test)]`, small synthetic frames):
  - `describes_every_component_with_its_settings`: `describe()` lists `png`, `jpeg`, `webp` (video encoders) and `image`, `imageSequence` (muxers), all available, with the Decisions' keys, defaults, and `inputFormats`; every field passes `celesta_media::settings::validate(&schema, &Settings::new())` defaults round trip.
  - `default_png_is_todays_png`: a 7×5 opaque RGBA8 frame through `builtin/png` with default settings gives bytes equal to `png::Encoder::new(..)` with `ColorType::Rgba`, `BitDepth::Eight`, no `set_compression` call (today's `stills.rs` code, copied into the test).
  - `default_jpeg_and_webp_are_todays`: the same frame with alpha 255 through `builtin/jpeg` at quality 90 equals `JpegEncoder::new_with_quality(.., 90)` of the RGB bytes; `builtin/webp` at quality 90 equals `webp::Encoder::from_rgba(..).encode_simple(false, 90.0)`.
  - `png_takes_sixteen_bits_and_compression`: `depth=16` with `[rgba16, rgba8]` offered opens with input `rgba16`; the file decodes (with `png::Decoder`) as 16-bit RGBA equal to the input's values; `compression=high` decodes to the same pixels as `none`; `depth=16` with only `[rgba8]` offered is `InvalidSettings { key: "depth", .. }`.
  - `transparent_keeps_alpha`: PNG and WebP with `transparent=true` answer `alpha: true`; a pixel of alpha 0 decodes with alpha 0 (WebP lossless for an exact check); JPEG answers `alpha: false`.
  - `webp_lossless_is_vp8l`: `lossless=true` writes a `VP8L` chunk and decodes equal to the input; the default writes `VP8 `.
  - `webp_refuses_images_too_large`: 16,383×16,383 opens; 16,384×9 is `InvalidSettings { key: "size", message }` with today's message.
  - `image_writes_one_frame`: open `image` on `<tmp>/a.png` with a `png` track, write one packet, finish → `[<tmp>/a.png]` holding those bytes; a second packet errors; finish with none errors; a `webp` track for `a.png` is refused at open.
  - `the_sequence_writes_each_frame_as_it_comes`: `imageSequence` with pattern `<tmp>/frame-%06d.png`, frame rate 30000/1001: packets at pts `3*1001`, `0` write `frame-000003.png` then `frame-000000.png`, each existing right after its `write_packets`; `finish_muxer` lists them in that order; pts `3*1001` again errors.
  - `the_pattern_is_only_the_last_component`: `sequence_path("run%d/50%-x/frame-%06d.png", 7)` is `run%d/50%-x/frame-000007.png`; a last component without `%06d`, with two, or not UTF-8 (Unix, `OsStr::from_bytes`) is an error; a non-UTF-8 parent (Unix) is kept byte for byte.
  - `codec_for_extension_follows_the_extension`: `PNG` → `png`, `jpg`/`JPEG` → `jpeg`, `webp` → `webp`, `gif` and `` → None.
- [ ] **Step 2:** `cargo test -p celesta-media-image` fails (crate missing).
- [ ] **Step 3: Implement.** Encode code moves from `crates/exporter/src/stills.rs` (`Encoder::encode`); `stills.rs` is left as it is until Task 4.
- [ ] **Step 4:** `cargo test -p celesta-media-image` and `cargo clippy -p celesta-media-image --all-targets --locked -- -D warnings` pass.
- [ ] **Step 5: Commit:** `feat(media-image): built-in PNG, JPEG and WebP encoders and image muxers`.

### Task 3: Register the built-ins; video stays off them; `rgba8` alpha

**Files:**
- Modify: `crates/exporter/Cargo.toml` (`celesta-media-image.workspace = true`), `crates/exporter/src/media.rs` (`with_builtin`, `media_registry`, `current`), `crates/exporter/src/resolve.rs` (`serves_video`, `default_muxer` skips still-only muxers), `crates/exporter/src/encode.rs` (`ExportSession::resolve` refuses a still-only muxer; the alpha rule), `crates/exporter/src/components.rs` (no folder for built-ins), `crates/exporter/src/lib.rs` (re-exports), `crates/media/src/provider.rs` (`PixelFormat::Rgba8` doc), `crates/editor/src/export_dialog/model.rs` (leave still-only muxers out), `crates/mcp/src/server.rs` only if its listing prints folders
- Test: `crates/exporter/src/media.rs`, `resolve.rs`, `encode.rs`, `components.rs` tests; `crates/editor/src/export_dialog/model.rs` tests; `crates/exporter/tests/integration/` (a CLI case)

**Interfaces:**
- Consumes: Task 2's `celesta_media_image::registered()`.
- Produces, in `celesta_exporter`:
  ```rust
  /// `plugins` after the built-in components, unless one of them already has the reserved id.
  pub fn with_builtin(plugins: Vec<RegisteredPlugin>) -> Arc<MediaRegistry>;
  /// Whether a video export may write with `muxer`: not the built-in plugin's, not a sequence muxer (deviation 50).
  pub fn serves_video(muxer: &MediaComponentInfo) -> bool;
  ```
- `media_registry(plugins)` = `with_builtin(<today's list>)`; `current()`'s
  fallback is `with_builtin(Vec::new())`.
- `ExportSession::resolve` (video exports only): after `resolve_output`,
  a muxer that is not `serves_video` → `ExportError::ComponentUnavailable
  { component, reason: "it writes still images; select frames to export
  stills" }`, before any process starts for it.
- `default_muxer` skips muxers that are not `serves_video` (so the
  candidates, and `unsupported_output`'s extension list, are today's).
- `try_open_video`'s alpha check: `opened.alpha` with an input other than
  `rgba8` or `rgba16` → `InvalidSettings { key: "alpha", message:
  "keeping alpha needs rgba8 or rgba16 as the input, not {input}" }`.
- Listings: a built-in component's text line has no folder, and its JSON
  object no `directory` key.

- [ ] **Step 1: Write failing tests.**
  - media: `registry_lists_media_plugins_in_order` updated — index 0 is `builtin` (origin `BuiltIn`), then today's plugins; `with_builtin` of a list that already has a `builtin` plugin adds none.
  - resolve: `a_video_never_defaults_to_an_image_muxer` — a registry of `with_builtin` plus a bundled mp4 plugin: `out.png` is `UnsupportedOutput` whose `extensions` are the mp4 plugin's only; a user plugin's `sequence: true` muxer for `.tif` is not a default either.
  - encode: `a_video_naming_a_still_muxer_is_refused` — `ExportSession::resolve` with `muxer: builtin/image` is `ComponentUnavailable` with the reason above, and no factory ran for the built-in plugin.
  - encode: `an_encoder_may_keep_alpha_in_rgba8` — the fake encoder answering `alpha: true` with `rgba8` opens; with `yuv420p` it fails with key `alpha`.
  - components: `built_ins_are_listed_without_a_folder` — `--list-components`'s text and JSON for `with_builtin(Vec::new())` list `builtin/png` … `builtin/imageSequence` as `built-in`, without `directory`; `describe_component(builtin/webp)` gives the three settings.
  - editor model: `still_muxers_are_not_containers` — a registry with the built-ins and a bundled mp4: the container list holds mp4 only.
  - integration (CLI): `stills_need_no_media_plugin` — `celesta-exporter --list-components --json` with `CELESTA_PLUGIN_PATH` set to an empty folder and the test binary's folder free of `celesta-codec-ffmpeg` (skip with a message when the sibling exists) lists the five built-ins.
- [ ] **Step 2:** the tests fail.
- [ ] **Step 3: Implement.** Fix every test that counted plugin indices of a `media_registry`.
- [ ] **Step 4:** `cargo build -p celesta-codec-test`, `cargo test -p celesta-exporter -p celesta-editor -p celesta-mcp`, clippy pass.
- [ ] **Step 5: Commit:** `feat(exporter): register the built-in image components, never for video`.

### Task 4: Stills through components

**Files:**
- Create: `crates/exporter/src/still_output.rs`
- Modify: `crates/exporter/src/stills.rs` (`Destination` uses `StillOutput`; `Encoder` and `flatten` go; `StillExport.quality`'s doc), `crates/exporter/src/render.rs` (`configure_renderer`, used by `open_video_output`), `crates/remote/src/staged.rs` (`publish_path`), `crates/exporter/src/lib.rs`, `crates/exporter/Cargo.toml` (drop `png`, `webp`, `image` from `[dependencies]`; keep what tests decode with under `[dev-dependencies]`)
- Test: `crates/exporter/src/still_output.rs` and `stills.rs` tests, `crates/remote/src/staged.rs` tests, `crates/exporter/tests/integration/stills.rs`

**Interfaces:**
- Consumes: Task 2's `codec_for_extension`; Task 3's `with_builtin`; P3's `resolve_preset_output`, `ResolvedOutput`; `ExportOptions.output` (now read by stills too).
- Produces:
  ```rust
  // celesta_remote::staged
  /// Renames the file at `staged` to `destination` as StagedFile::publish does (no-clobber
  /// unless `overwrite`, AlreadyExists when it is there).
  pub fn publish_path(staged: &Path, destination: &Path, overwrite: bool) -> io::Result<()>;

  // celesta_exporter::render (pub(crate))
  /// Sets `renderer` to `opened`: its readback format, and a transparent background when it keeps alpha.
  pub(crate) fn configure_renderer(renderer: &mut GpuRenderer, opened: &VideoEncoderOpened) -> Result<(), ExportError>;

  // celesta_exporter::still_output (pub(crate))
  pub(crate) enum StillFiles { One(PathBuf), Sequence(Vec<(u64, PathBuf)>) }   // (frame, final name), selection order
  pub(crate) struct StillOutput { /* registry, kill, providers by plugin, resolved, sessions, staging */ }
  impl StillOutput {
      /// Names the components (Decisions), resolves them, starts their providers
      /// (ProviderUse::Export { ring: None }; built-ins in process), opens the encoder at `size`,
      /// offering [rgba16, rgba8], or [rgba8] when `sheet`. Before anything renders.
      pub(crate) fn open(choice: &OutputChoice, quality: Option<u8>, files: StillFiles, sheet: bool,
          size: (u32, u32), frame_rate: Rational, overwrite: bool, cancellation: &ExportCancellation,
      ) -> Result<(Self, Vec<String>), ExportError>;               // the resolution's warnings
      pub(crate) fn opened(&self) -> &VideoEncoderOpened;
      /// Encodes composition frame `frame` (pts = frame × denominator) and writes it: a sequence's
      /// file is published now and returned; one file stays staged until `finish`.
      pub(crate) fn write(&mut self, frame: u64, pixels: Vec<u8>) -> Result<Option<PathBuf>, ExportError>;
      /// Flushes the encoder, finishes the muxer, and publishes a single file (returned).
      pub(crate) fn finish(self) -> Result<Option<PathBuf>, ExportError>;
      /// Ends open sessions; staged files go, published ones stay.
      pub(crate) fn abort(self);
  }
  ```
- The registry is `media::current()` after `media::ensure_media(plugins)`;
  one without the built-in plugin is replaced by `with_builtin(its
  plugins)`. Describing: the built-in through `registry.describe`, a
  plugin through `describe_with` on the provider started for the export
  (one start per plugin, as `ExportSession` does).
- Naming (`fn still_choice(choice, quality, files, output, registry) ->
  Result<OutputChoice, ExportError>`): `choice.preset` → `invalid_selection`
  "presets apply to video output, not still frames"; an
  `AudioChoice::Encode` with a component or options → `invalid_selection`
  "audio applies to video output, not still frames"; the returned choice
  has `AudioChoice::None`, the muxer and encoder of Decisions, and
  `quality` as `SettingInput::Value(Number)` under `quality` (with a
  `quality` option already given: `invalid_selection` "quality is given
  twice"). Then the sequence-ness and codec checks of Decisions.
- Staging: one file through `output::stage(path, ".tmp", …)` (today's),
  the muxer's `output` being the staged path, published by
  `output::publish`; a sequence in `tempfile::Builder::new().prefix(".celesta-").tempdir_in(parent)`
  with pattern `<staging>/frame-%06d.<the output's extension as given>`,
  each frame's file published with `publish_path` to its final name right
  after its `write_packets` (`ExportError::OutputExists` on a clash, as
  today). A frame whose file the muxer did not write is
  `ExportError::Codec { stage: MUXER_STAGE, MediaError::Codec("{muxer}
  wrote no file for frame {frame}") }`.
- `stills.rs`: `Destination::prepare` keeps `select`, `validate` (minus
  its extension check, now `still_choice`'s), the sheet layout, and the
  overwrite checks of every name (`outputs`), then opens `StillOutput`;
  `export_*_stills_with` call `configure_renderer` after
  `export_renderer`, report the warnings as `ExportProgress::Warning`, and
  `render_stills` passes `rendered.into_pixels()` (separate files) or the
  sheet's pixels (`finish`) through it. Any error after open calls
  `abort`. A contact sheet's frames render over the background the
  encoder chose.

- [ ] **Step 1: Write failing tests.**
  - staged: `publish_path_never_clobbers_unless_asked` — an existing destination stays and is `AlreadyExists` without `overwrite`, is replaced with it; the staged file is gone after a successful publish.
  - still_output (with a `with_builtin(Vec::new())` registry and in-memory RGBA frames): `names_the_built_ins_by_extension` (`a.JPG` one file → `builtin/image` + `builtin/jpeg`; two files → `builtin/imageSequence`); `quality_becomes_the_encoders_option` (quality 40 → settings `quality: 40`; with `--video-option quality=40` too → `InvalidSelection`); `presets_and_audio_do_not_apply` (both errors above); `a_named_muxer_must_match_the_file_count`; `a_mismatched_built_in_codec_is_incompatible` (`builtin/webp` for `a.png`); `png_quality_is_an_invalid_option` (code `invalid_option`); `webp_too_large_fails_before_rendering` (code `invalid_option`, today's message); `sixteen_bit_png_is_not_for_contact_sheets`.
  - stills (unit): today's `validates_first_last_and_out_of_range_frames`, `rejects_empty_duplicate_and_non_png_selections`, `separate_files_keep_the_output_extension`, `checks_every_output_before_rendering` pass unchanged (move or adapt only where they called `Encoder`); `quality_applies_to_jpeg_and_webp_only` and `webp_rejects_images_wider_than_it_can_hold` move into still_output's tests above; `jpeg_flattens_transparency_onto_the_export_background` becomes `jpeg_stills_are_opaque` (integration below covers it).
  - integration (`tests/integration/stills.rs`, existing tests unchanged in their assertions): every existing test passes as is, `json_report_lists_every_written_file_or_a_coded_error` included (selection order 3, 0; `invalid_selection` for frame 4), and `separate_stills_and_videos_name_the_frame_that_failed` (outputs keep frame 0). New:
    - `a_project_default_preset_never_reaches_stills`: a project folder with `export-presets.json` `{"version":1,"presets":[{"name":"v","muxer":"celesta-test-codec/…"}],"default":"v"}` (any muxer): `--frame 0 out.png` exits 0, writes a PNG, and the JSON report has no `warnings`.
    - `transparent_png_and_webp_keep_alpha`: a project whose background layer is absent (`colored_project` without its full-frame rect, or a half-covered frame): `--frame 0 --video-option transparent=true out.png` decodes with alpha 0 outside the shape; without it, alpha 255 everywhere (today's `stills_are_opaque`); the same for `.webp` with `lossless=true`.
    - `sixteen_bit_png`: `--video-option depth=16 out.png` decodes as 16-bit RGBA whose high bytes equal the 8-bit export's pixels within 1.
    - `a_failed_sequence_keeps_written_files_and_no_staging`: the React bad-frame entry with `--frames 0,6,7`: `check-000000.png` exists, `check-000006.png` and `-000007` do not, and the folder holds no `.celesta-*` entry (skip like its neighbor without `packages/cli/dist`).
    - `a_cancelled_sequence_leaves_later_names_alone`: library-level, `--overwrite`, a pre-existing `out-000002.png` holding `keep`, an `ExportCancellation` cancelled from the progress callback after the first `Wrote`: `out-000000.png` is the new frame, `out-000002.png` still holds `keep`, no `.celesta-*` entry.
    - `percent_signs_in_the_output_name_are_literal`: `--frames 0,1` to `<dir>/50%d/c%06d.png` writes `c%06d-000000.png` and `c%06d-000001.png` there.
- [ ] **Step 2:** `cargo test -p celesta-remote staged`, `cargo test -p celesta-exporter still` fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** `cargo build -p celesta-codec-test`, `cargo test -p celesta-exporter -p celesta-remote -p celesta-mcp`, clippy, and `cargo fmt --all -- --check` pass.
- [ ] **Step 5: Commit:** `feat(exporter): stills write through the built-in image components`.

### Task 5: Front ends — component choices for stills

**Files:**
- Modify: `crates/exporter/src/main.rs` (`output_kind`, `job`, the stills help paragraph, `--quality`'s doc), `crates/mcp/src/args.rs` (`RenderFramesArgs`, `ContactSheetArgs`), `crates/mcp/src/docs.rs` if it documents the still tools' arguments, `packages/exporter/src/types.ts` (`ExportStillsOptions`), `packages/exporter/src/args.ts`
- Test: `crates/exporter/src/main.rs` tests (CLI parsing), `crates/mcp/src/args.rs` tests, `packages/exporter/test/args.test.mts`

**Interfaces:**
- Consumes: Task 4's reading of `ExportOptions.output` for stills.
- CLI: for stills, only `--preset`, `--preset-file`, `--audio-encoder`,
  `--audio-option` are rejected ("{flag} applies to video output, not
  still frames"); `job` sets `options.output = cli.output_choice()` for
  stills too. `--quality` with a `--video-option quality=…` is the usage
  error "--quality and --video-option quality= both set the quality;
  give one". The PNG `--quality` usage error stays when no
  `--video-encoder` is given.
- MCP: both still tools gain `muxer?: string` and `videoEncoder?: { id?,
  options? }` (the video tool's `EncoderArg`, parsed and checked the same
  way) into `ExportOptions.output`; `quality` stays.
- TS: `ExportStillsOptions` gains `muxer?: string` and `videoEncoder?:
  EncoderOptions`, passed with the existing `encoderFlags('video', …)` and
  `--muxer`.
- Help text (stills paragraph), added sentence: "--video-option sets the
  image encoder's settings: compression=none|fastest|fast|balanced|high,
  depth=16 and transparent=true for PNG; lossless=true and
  transparent=true for WebP (--describe builtin/png lists them).
  --muxer and --video-encoder name a plugin's image components."

- [ ] **Step 1: Write failing tests.**
  - CLI: `stills_take_video_options`: `--frame 0 --video-option transparent=true out.png` parses to `OutputKind::Stills` with `options.output.video.options["transparent"] == Text("true")`; `--frame 0 --preset web out.png` and `--frame 0 --audio-encoder none out.png` are usage errors naming the flag; `--frame 0 --quality 50 --video-option quality=60 out.jpg` is the usage error above; `--frame 0 --muxer foo/avif out.avif` parses.
  - MCP: `still_tools_take_components`: `render_frames` with `videoEncoder: { options: { lossless: true } }` builds an `ExportOptions.output` with that typed value; an invalid `muxer` reference is refused as the video tool refuses it.
  - TS: `exportStills` with `videoEncoder: { options: { depth: 16 } }` and `muxer: 'builtin/image'` emits `--muxer builtin/image --video-option depth=16`.
- [ ] **Step 2:** the tests fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** `cargo test -p celesta-exporter -p celesta-mcp`, `pnpm --dir packages/exporter test`, clippy pass.
- [ ] **Step 5: Commit:** `feat(exporter): stills take a muxer, a video encoder, and its options`.

### Task 6: Docs, spec, and verification against P3

**Files:**
- Modify: `docs/superpowers/specs/2026-11-08-media-components-design.md` (deviations 50–59 under the sections they change: Components, Muxing, Selection, Pixel formats and color, Stills and sequences (P4), and Phases → P4's "As built"; `rgba8`'s row in the format table), `HANDOFF.md` (the crate table gains `celesta-media-image`; `celesta-exporter`'s row; the stills behavior), `packages/website/content/en/docs/02-(guides)/09-export.md` and its `ja` counterpart (still settings, transparency, 16-bit PNG, lossless WebP, naming a plugin's image components, that stills ignore presets), `skills/celesta/references/export.md` (the built-in components' keys, `--describe builtin/png`)
- Create: `docs/handoff/2026-11-09-media-stills.md` (spec, implementation, deviations 50–59, the verification below, Not done)

- [ ] **Step 1: Write the docs** above. Sentence case, no new terms beyond the spec's.
- [ ] **Step 2: Compare against P3.** Build P3's exporter in a separate worktree (`git worktree add <scratch>/p3 claude/media-components-p3`, `cargo build --release -p celesta-exporter` there) and this branch's; for `examples/` projects (one JSON project, one React entry) export with both: `--frame 0 a.png`, `--frames 0,12,-1` (React: `--frames 0,12`) `b.png`, `--every 15 c.jpg --quality 70`, `--every 30 d.webp`, `--every 10 --contact-sheet e.png`, `--every 10 --contact-sheet f.webp`. Every file must be byte-identical (`cmp`); record the commands and results in the handoff note. Remove the worktree after.
- [ ] **Step 3: Full verification.** `cargo build -p celesta-codec-test`; `cargo test --workspace`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `cargo fmt --all -- --check`; the plugins workspace's tests and clippy (`cargo test --manifest-path plugins/Cargo.toml`, `cargo clippy --manifest-path plugins/Cargo.toml --all-targets --locked -- -D warnings`); `pnpm run test:runtime`; `pnpm --dir packages/exporter test`; the website tests (`pnpm --dir packages/website test`). Record skipped tests (GPU, `packages/cli/dist`) in the handoff note.
- [ ] **Step 4: Commit:** `docs: built-in image components and stills through them`.
