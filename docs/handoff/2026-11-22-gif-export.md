# Animated GIF export (2026-11-22)

`Celesta-export … out.gif`, the editor's export page (**GIF · Built-in**),
MCP's `export_video`, and `@celesta.studio/exporter`'s `exportVideo` write
an animated GIF, for short loops in READMEs and posts. The GIF is written
by a new built-in encoder, `builtin/gif`, and muxer, `builtin/gifFile`, in Celesta's own
process; it needs no media plugin.

## Decisions

- **Where the encoder lives: built in, pure Rust** (`celesta-media-image`,
  beside `builtin/png`, `jpeg`, and `webp`), not FFmpeg's `gif` encoder in
  `plugins/ffmpeg`. FFmpeg's encoder writes good GIFs only behind its
  `palettegen`/`paletteuse` filter graph, which the plugin does not build,
  and a GIF should not need a media plugin, nor the `ffmpeg-x264` download.
  The crates are MIT/Apache-2.0: `gif` 0.14 (LZW and the file format,
  already in the lockfile through `image`'s tree) and `color_quant` 1.1
  (NeuQuant). `libimagequant` was not used: it is GPL-3.
- **Built-in muxers may now serve video, but only `builtin/gifFile`**
  (`celesta_media_image::muxer_writes_video`, used by the exporter's
  `muxer_serves_video`). This narrows deviation 50 of the media components
  spec: `builtin/image` and `builtin/imageSequence` still serve stills only,
  and every `sequence: true` muxer is still refused for video. `default_muxer`
  ranks built-ins first, so `.gif` resolves to `builtin/gifFile` even if a plugin
  someday lists `gif`; naming a plugin's muxer still wins.
- **Animated WebP and APNG: not built.** Both would fit the same path (an
  encoder giving its header as the track config and a frame per packet, a
  muxer appending them): the `png` crate writes APNG and libwebp has
  `WebPAnimEncoder`. They are left out because their extensions are the
  still formats': `out.png` and `out.webp` without a frame selection are
  today `unsupported_output` with the `--frame` hint (deviation 50), and
  making them animations changes what a missing `--frame` does. Adding them
  later means new encoders and muxers (`builtin/apng`, `builtin/webpAnim`)
  listed by `muxer_writes_video`, and deciding whether `.png` defaults to
  APNG or keeps the hint (an `.apng` extension avoids the question).
- **Frame rate, scale, and palette are encoder settings**, so presets, the
  export page's fields, `--video-option`, MCP, and the TS package get them
  with no new flag. Rendering still happens at the composition's size and
  rate; the encoder drops and scales (see Not done).
- **`--loop`** is accepted and changes nothing: a GIF has no sound, and its
  repeats are `plays` (forever by default, as GIFs are expected to loop).

## Specification

- **Manifest.** Video encoder `gif` "GIF" (`codecs: ["gif"]`,
  `inputFormats: ["rgba8"]`); muxer `gifFile` "GIF" (`extensions: ["gif"]`,
  `video: ["gif"]`, `defaultVideo: "gif"`, no audio, not a sequence).
  `describe` gives the encoder's settings; the muxer has none.
- **Settings** (`builtin/gif`), in order:
  - `frameRate` select: `source` "The composition's (at most 50)" (default),
    `50`, `30`, `25`, `24`, `20`, `15`, `12`, `10`, `5`. `source` keeps every
    `ceil(fps / 50)`th frame (60 fps → 30). A fixed rate keeps the first
    frame of each 1/rate second.
  - `scale` number 1–100, integer, unit `%`, default 100. Each side is
    `round(side × scale / 100)`, at least 1, scaled with a triangle filter
    (`image::imageops::resize`) after rendering. Over 65535 a side is
    `invalidSettings` key `scale`.
  - `palette` select `global` "One for the whole GIF" (default), `frame`
    "One for each frame".
  - `colors` number 2–256, integer, default 256; one entry is always the
    transparent index, so a palette holds `colors - 1` colours.
  - `dither` boolean, default true: Floyd–Steinberg error diffusion within
    each frame's rectangle (no error crosses an undrawn pixel).
  - `plays` number 0–65536, integer, default 0: 0 writes the NETSCAPE2.0
    loop extension with 0 (forever), 1 writes none (plays once), n writes
    n − 1.
  - `transparent` boolean, default false, PNG's label and description.
- **Timing.** A frame's time is its pts in hundredths of a second, rounded;
  a kept frame's delay is the next kept frame's time minus its own, and the
  last frame's runs to the end of the last frame given (at least 2). A frame
  less than 2 hundredths after the frame before is dropped (browsers play
  0–1 as 10), so 30 fps alternates 3, 4, 3 and the GIF keeps the
  composition's length. A frame identical (after scaling and alpha
  thresholding) to the one before is folded into it. Packets carry
  `pts` = the kept frame's pts, `duration` = until the next kept frame,
  `key` only for frames drawn whole.
- **Palettes.** When the frames' opaque colours fit (`colors - 1`), the
  palette is exactly those colours, sorted, and nothing is dithered;
  otherwise NeuQuant (sampling factor 3) learns from the opaque pixels,
  sampled evenly to at most 2^20 pixels for a global palette. Lookup is
  exact for palette colours, else the entry nearest the centre of the
  colour's 6-bit cell (cached per palette), so output is deterministic.
  `global` holds every kept frame (rgba8, after scaling) until
  `finish_video`, then writes the header with a global colour table and
  every frame; `frame` writes the header at open and each frame with a
  local table as soon as the next kept frame gives its delay.
- **Drawing.** Opaque GIFs with a global palette: the first frame is
  drawn whole; later frames draw only the bounding rectangle of the pixels
  that changed, with unchanged pixels as the transparent index and
  disposal "keep". With a palette per frame every frame is drawn whole
  (disposal "keep"): a pixel left undrawn would keep the colour the palette
  before chose for it, which the new palette may show better.
  `transparent=true`: scaling premultiplies alpha first and divides it out
  of the pixels kept after, so transparent black does not darken edges;
  then alpha under 128 is transparent, the rest opaque
  (`alpha: true`, so the export clears the background and reads back
  straight `rgba8`); each frame is drawn whole with disposal "restore to
  background".
- **Track config.** `codec: "gif"`, `description`: the GIF header (GIF89a,
  logical screen, global colour table or `gif`'s empty one, loop
  extension). With a global palette it arrives with the packets at
  `finish_video`; the export opens the muxer then, as for any encoder whose
  config comes late.
- **Muxer.** Takes exactly one video track of the `gif` family with a
  header; else `invalidSettings` key `tracks`. It writes any extension:
  only `.gif` picks it, and a named `--muxer builtin/gifFile` (or preset)
  to another extension writes a GIF there with Celesta's mismatch warning. Creates the file at open and writes the header, appends each
  packet's bytes, and writes the trailer (`;`) at finish; abort names the
  file for Celesta to remove.
- **Cancel.** The built-in provider keeps the export's plugin kill switch,
  which cancel throws; a global palette's final encode checks it before
  learning the palette and before each frame, and fails with
  `codecStopped`, which the export reports as `cancelled` and cleans up.
- **No media plugin.** When `.gif` is the only extension a muxer writes,
  `unsupported_output`'s hint is "write an animated GIF to a .gif path; for
  other formats, install a media plugin …".
- **Warnings** (`celesta_media_image::gif_warnings`, from
  `ExportSession::output_warnings` once the export's span is known, for the
  JSON project and React paths): "a {s} s GIF of {n} {w}x{h} frames will be
  large and slow to write; …" when the span is over 30 s or the kept frames
  hold over 100 million pixels (`gif_estimate`'s upper bound), and
  "palette=global holds every frame in memory until the end, about {x} GB
  here; …" over 1 GiB of held frames.
- **Stills.** `--frame`/`--frames`/`--every`/`--contact-sheet` to `.gif`
  keep the still extension message; `--muxer builtin/gifFile` with a frame
  selection is `invalid_selection` "builtin/gifFile writes an animation, not
  still frames; …".
- **Editor.** The Container list now shows **GIF · Built-in**; built-in
  muxers sort after the plugins' containers, so it is last, and the default
  outputs (`export.mp4`, then each container's first extension) try `.gif`
  last. Its encoder fields come from the schema; the Audio section says no
  audio is exported, since the muxer takes none.

## Implementation

- `crates/media-image/src/animated_gif.rs`: settings, `EncoderSession`,
  `MuxerSession`, `Palette`, `Lookup`, `Diffusion`, `estimate`/`warnings`,
  and its tests. `lib.rs` adds the manifest and `describe` entries, the
  `GIF` constant, `muxer_writes_video`, and the session maps;
  `encoders.rs` and `muxers.rs` dispatch `gif` sessions.
- `crates/exporter/src/resolve.rs` (`muxer_serves_video`),
  `still_output.rs` (the still refusal), `encode.rs` (`output_warnings`),
  `exporter.rs` and `react_entry.rs` (reporting them), `main.rs` (help).
- `crates/editor/src/export_page/model.rs`: the built-in muxer sort.
- `crates/mcp/src/args.rs`, `packages/exporter/src/{index,types}.ts`: doc
  comments name `.gif`.
- Docs: the user guide's export page (en, ja: Container, the extension
  list, a new "Animated GIF" section), the Skill's `references/export.md`
  (title, options, a new "Animated GIF" section) and `SKILL.md`.

## Checks

On macOS 26 (Apple silicon, Metal), with a GPU adapter, so the live GIF
tests ran. Nine sessions were building at once, and macOS stalled every
freshly copied or built executable at launch (see below).

- `cargo test -p celesta-media-image`: 24 passed (11 new GIF tests: the
  delays and the folded frame, palette per frame, frame rate and the
  2-hundredths floor, `source_step`, `plays`, `scale`, one-bit
  transparency, NeuQuant with and without dithering, the estimate and
  warnings, and the refusals; the listing tests updated).
- `cargo test -p celesta-exporter --lib`: 192 passed, 1 failed:
  `the_test_codecs_program_writes_what_it_writes_in_process`, whose test
  codec program "did not answer `hello` within 30 s" (it passes alone).
  New: `a_gif_resolves_to_the_built_in_and_warns_when_large`,
  `a_gif_is_the_built_in_animation`, and the GIF cases in
  `a_video_never_defaults_to_an_image_muxer` and
  `a_named_muxer_must_match_the_file_count`.
- `cargo test -p celesta-exporter --test integration --no-fail-fast`: 66
  passed, 15 failed. The new `exports_an_animated_gif_with_the_built_in_components`,
  `a_transparent_gif_keeps_one_bit_alpha_at_its_scale`,
  `the_built_in_gif_is_listed_and_described`, and
  `a_gif_still_and_a_still_gif_muxer_are_refused` pass. The 15 failures are
  tests that start a freshly linked program (the test codec's, the
  exporter's, or a plugin's): on this machine such a program took 47 s to
  start (a copied exporter about 24 minutes), so each fails its 30 s
  `hello` deadline or reports the test codec unavailable. Another session
  recorded the same stall at `_dyld_start` that day. They need a re-run on
  a machine without it (CI).
- `cargo test -p celesta-editor export_page`: 68 passed
  (`still_muxers_are_not_containers` now expects **GIF · Built-in** last
  and `export.gif` last among the default outputs).
- `cargo test -p celesta-mcp --bins args`: 22 passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo fmt --all -- --check`, `git diff --check`: clean.
- `pnpm --dir packages/website test`: 52 passed.
- The sample: `celesta-exporter --json --react examples/perfect-loop/film.tsx
  --from 0 --to 4 --video-option scale=33 --video-option frameRate=25
  perfect-loop.gif` with `CELESTA_PLUGIN_PATH` set to an empty folder
  (no media plugin): 356x446, 100 frames, 1.5 MB, in 67 s (debug build),
  `"muxer": "builtin/gifFile"`, `"video": {"component": "builtin/gif",
  "codec": "gif"}`, no warnings. Attached to the pull request.

No rendering changed, so there is no before/after screenshot pair; the GIF
is new output.

## Not done

- Frames the frame rate drops are still rendered, and scaling happens after
  rendering at full size; a GIF export of a 60 fps composition renders
  twice the frames it keeps. Rendering only kept frames, at the scaled
  size, would need the exporter to know the encoder's frame selection.
- A global palette holds every kept frame in memory (warned over 1 GiB);
  it is not spilled to disk.
- GIF transparency is one bit with a fixed 128 threshold; no matte colour
  for soft edges.
- No audio warning: a composition's soundtrack is silently left out
  (documented), except that `--loudness` warns there is no audio.
- Animated WebP and APNG (see Decisions).
