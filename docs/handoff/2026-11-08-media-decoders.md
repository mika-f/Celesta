# Media plugins and decoders, P1 (2026-11-08)

Phase P1 of the media components design
(`docs/superpowers/specs/2026-11-08-media-components-design.md`, #782;
plan `docs/superpowers/plans/2026-11-08-media-components-p1.md`). The
single `codec` role (`2026-11-05-codec-plugin.md`) becomes the `media`
role: a media plugin declares its decoders in its manifest, Celesta uses
every media plugin that may run, and each file is routed to a decoder.
Encoding is unchanged: until P2a, exports encode and mux through the first
media plugin with protocol 1's encoder messages, so exports are
byte-identical to `main`'s.

## Specification

- **Manifest.** `roles: ["media"]` with `protocol: 2`; content plugins
  stay on protocol 1, and discovery checks each plugin's `protocol`
  against its role (`celesta_plugin::protocol_for_roles`). `media.decoders`
  lists `{ id, name, extensions }`:
  - `id` follows the plugin id rules and is unique in the plugin;
  - `extensions` are read lower-case and without a leading dot;
  - a malformed decoder is skipped and reported
    (`DiscoveryError::BadComponent`, `<manifest>: a media component was
    skipped: …`) without dropping the plugin's other decoders.

  A manifest with both `content` and `media`, or the reserved id `builtin`
  (`RESERVED_PLUGIN_ID`), is invalid. An old `"codec"` role parses as
  `PluginRole::Unknown` and the plugin is ignored for media.
  `plugins/ffmpeg/plugin.json` declares one decoder, `ffmpeg`, for 26
  extensions; `scripts/plugins/build.mts` writes `protocol` per role and
  copies `media`.
- **Listing.** `InstalledPlugins::media()` returns a `MediaPlugin { id,
  version, executable, directory, origin, decoders }` for every plugin that
  may run (search order, approval, disabled list) with the `media` role and
  a program for this platform. `MediaOrigin` is `User` (`Explicit`, `User`,
  `Folder` sources), `Bundled` (`Bundled`), or `Project` (`Project`).
- **The `cargo install` sibling.** When no listed plugin has its id,
  `celesta-codec-ffmpeg` beside Celesta's executable (links resolved, never
  in a Cargo build folder) is listed as `Bundled`, from what it prints for
  `--manifest` (`MANIFEST_FLAG`: id, version, roles, media; 5 s deadline,
  1 MiB limit, no protocol started). The answer is cached by the program's
  path, size, and modification time; a program that fails, times out, or
  is not a media plugin is left out and said on stderr.
- **Registry.** `celesta_media::MediaRegistry` holds the plugins in search
  order, each with a process factory (`RegisteredPlugin`). Its decoders are
  ordered by origin (user, then bundled, then project), then search order,
  then manifest order. `set_registry` and `registry` replace `set_codec`
  and `codec`; `MediaBackend::new()` takes the registry chosen when it
  first needs one, and `MediaBackend::with_registry` the given one. The
  exporter's `media_registry`, `use_media`, and `use_installed_media`
  replace `use_codec` and `use_installed_codec`; the editor sets one on each
  discovery, `celesta-mcp` and `--inspect` at start-up.
- **Routing** (`MediaBackend`, on the first request for a path):
  1. A path that does not exist fails with `NotFound` before any plugin
     starts.
  2. Candidates are the decoders listing the path's extension
     (case-insensitively), or every decoder when none does or the path has
     no extension. No decoder at all is `MediaError::NoDecoder`.
  3. Each candidate is asked to `probe` in turn. `unsupported` (valid only
     as a `probe` answer) moves on; any other error, or a plugin whose
     process does not start, moves on and is kept; `CodecStopped` (a missed
     deadline, or the kill switch) ends routing with that error.
  4. The first probe that succeeds pins the path to that decoder, with the
     probe cached; its `decodeVideo` and `decodeAudio` go there, and their
     errors do not fall back. A decode that answers `unsupported` is a
     plain `Codec` error.
  5. When every candidate fails, the error kept is ranked `NotFound` >
     `NoVideoStream` > the first other error; with only `unsupported`
     answers it is `MediaError::Unsupported { path, tried }` (`no decoder
     could read <path> (tried <plugin>/<decoder>, …)`).

  Each backend starts one process per plugin it routes to, reopened once on
  `CodecUnavailable` as before.
- **Protocol 2.** `hello` names protocol 2 and the role `media`
  (`MediaProcess` checks both); `probe`, `decodeVideo`, and `decodeAudio`
  carry `decoder`, the component's id. An error with `code: "unsupported"`
  brings back `MediaError::Unsupported`. #775's shared-memory slot and
  `CELESTA_CODEC_SHARED_MEMORY=0` are unchanged. The encoder messages
  (`checkEncoder`, `openEncoder`, `encodeFrame`, `finishEncoder`,
  `abortEncoder`, `muxAudio`) stay as protocol 1 had them, sent to the
  registry's first plugin (`MediaRegistry::legacy_encoder`); with none,
  `MediaError::NoCodec`.
- **Exporter codes.** `no_decoder` (`MediaError::NoDecoder`, with the hint)
  and `unsupported_media` (`MediaError::Unsupported`, no hint) beside
  `no_codec`, which stays for encoding and reuses the same hint
  (`NO_DECODER_HINT`). A video export checks its encoder before it renders,
  so with no media plugin at all it reports `no_codec`; still exports, and
  video exports whose media plugins have no decoder, report `no_decoder`.
- **Editor.**
  - **No media decoder can run** replaces "No codec plugin can run" in
    Problems, with the same reasons (no program for this computer, waiting
    for approval, turned off, none found), when the registry has no
    decoder.
  - The registry records the files each project plugin's decoder read
    (`record_use`, project origin only, once per decoder and path; a
    generation counter changes with each new use). The editor checks the
    generation on each frame and reads the list only when it changed, and
    shows it as an Info-level Problems entry (`Source::MediaDecoders`,
    labelled "Project decoders"), titled `<plugin> decodes N files in this project` (or `N
    project plugins decode M files in this project`) and listing the files
    relative to the project root, with no toast.
  - The Plugins panel's row of a project plugin says `Decodes N files in
    this project`.
- **Test codec.** `celesta-codec-test` is a media plugin with one decoder,
  `celesta-test-codec/test`, listing no extensions (it reads test codec and
  WAV files of any name, and answers `unsupported` for anything else).

## Deviations from the spec

Recorded in the spec's Capability discovery, Registry and routing,
Decoders, and Phases sections:

1. `describe` and its cache move to P2a: nothing consumes them in P1.
2. A process-wide registry stays (`set_registry` / `registry`), read
   lazily by `MediaBackend::new()`; `MediaBackend::with_registry` is the
   explicit form.
3. P1's manifest parses `media.decoders` only; the encoder and muxer keys
   arrive with P2a.
4. A path that does not exist fails with `NotFound` before any plugin is
   started.
5. New exporter code `unsupported_media` for `MediaError::Unsupported`,
   beside `no_decoder`.
6. A backend's legacy encoder (the registry's first plugin) may share a
   process with that plugin's decoder; P2a's per-export components separate
   them. Today's callers use separate backends for encoding and decoding.

## Docs

The user guide's plugins page (en and ja) has a **Media plugins** section
in place of **The codec plugin**: the manifest, the decoder order and
fallback, the project-decoder notice, **No media decoder can run** with
`no_decoder` and `no_codec`, `unsupported_media`, the `cargo install`
sibling, and that an old `codec` plugin is ignored. The troubleshooting,
export, build-from-source, and installation pages say "media plugin" and
link the new section. The Agent Skill's `plugins.md`, `export.md`,
`errors.md`, `setup.md`, and `verify.md` follow.

## Checks

On macOS arm64 (Apple Silicon, Metal), with this branch's FFmpeg plugin
staged by `build.mts` on `CELESTA_PLUGIN_PATH`:

- `cargo test --workspace` (2,465 passed, 1 ignored as before),
  `cargo clippy --workspace --all-targets --locked -- -D warnings`, and
  `cargo fmt --all -- --check` (clean, as on `main`).
- With `PKG_CONFIG_PATH` at Homebrew's `ffmpeg@8`:
  `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`
  (428 passed), its clippy, and `cargo fmt --check` in `plugins/`.
- `pnpm run test:runtime` (`@celesta.studio/cli` 118 and
  `@celesta.studio/components` 179 among them, all passed),
  `pnpm --dir scripts/plugins test` (217 passed),
  `pnpm --dir packages/website test` (52 passed), and `git diff --check`.
- `celesta-codec-test`'s `plugin_folder` test asserted that the test
  codec's folder is the only media plugin found, which fails whenever
  `CELESTA_PLUGIN_PATH` holds another one, as HANDOFF.md's setup does; it
  now asserts that it is the first.
- Unit and integration tests cover the rest (Tasks 1–9): protocol per role
  and `hello`; manifests with `media`, bad and duplicate decoder ids, both
  roles, `builtin`, and the old `codec` role; the sibling's `--manifest`
  (timeouts, bad output, caching); routing by extension and origin,
  fallback, pinning, `NotFound` before any process starts, `CodecStopped`,
  and the error ranking; the exporter's codes; the editor's problem,
  notice, and Plugins row; the FFmpeg plugin's `unsupported` probe and
  decoders through its program.
- End to end, against `main` (`01b85f23`): `main`'s exporter and FFmpeg
  plugin (protocol 1, the `codec` role) built in a temporary worktree,
  this branch's exporter and plugin (protocol 2, the `media` role), release
  builds, each exporter copied alone into a scratch folder with the same
  React runtime and its own plugin folder on `CELESTA_PLUGIN_PATH`, and an
  empty `HOME`. Byte-identical (`cmp`, SHA-256):

  | Output | SHA-256 |
  | --- | --- |
  | `examples/greenscreen/composite.tsx` as MP4 | `34f34172…807baff6` |
  | a `<Video>` (the green-screen footage) + `<Audio>` (`examples/assets/voices/001.wav`) entry as MP4 | `52e07cbb…910a3558` |
  | the same with `--loop` | `dda59d64…420cc077` |
  | the same as MOV | `ad240781…9fd381d8` |

  The green-screen export took 0.64–0.77 s with `main` and 0.62–0.67 s
  with this branch (three interleaved runs each).
- The branch's exporter with `celesta-codec-ffmpeg` beside it in a plain
  folder and no plugin folder (the `cargo install` layout) exported the
  `<Video>` + `<Audio>` entry identically; the program's `--manifest`
  prints its id, version, roles, and decoder.
- The branch's exporter with `main`'s plugin (`"roles": ["codec"]`) on
  `CELESTA_PLUGIN_PATH` ignores it: a still of the `<Video>` entry fails
  with `no_decoder`.
- Without any plugin, the JSON report of a video export is `no_codec`
  (with the hint) both for the `<Video>` entry and for an entry without
  media files, since the export checks its encoder before rendering; a
  still export (`--frame 0`) of the `<Video>` entry is `no_decoder` with
  the hint, and one of the entry without media files succeeds. A `<Video>`
  whose `.mp4` holds text fails with `unsupported_media` and `no decoder
  could read ./notmedia.mp4 (tried ffmpeg/ffmpeg)`.
- Not run: the packaging scripts (`test-package.mjs` passes
  `node --check`), Linux and Windows (CI), and the editor by hand.

## Not done

- **Runtime messages still say "codec plugin"**: `MediaError::NoCodec`,
  `CodecUnavailable`, `CodecStopped`, and `EncoderUnavailable`
  (`celesta-media`), `ExportError::EncoderUnavailable` (`celesta-exporter`),
  and some doc comments, crate descriptions, packaging scripts, and the
  `preview-bench` example. They are renamed with P2a's encoder messages.
- **A decoder-only plugin found first gets the encoder requests.** Exports
  encode through the registry's first plugin in search order, whatever it
  declares, so a user-installed plugin with only decoders ahead of the
  bundled `ffmpeg` would fail exports. No such plugin exists yet; P2a's
  component selection removes it.
- **A plugin that cannot start** is tried again for each newly routed path
  (allowed by the spec; worth caching in P2a).
- The editor's manual check (project decoder notice in Problems and the
  Plugins tab) is done separately from this note's checks.
- The brief expected `no_decoder` from a video export with no plugin; it
  is `no_codec`, because the encoder check comes first. The docs say so.
