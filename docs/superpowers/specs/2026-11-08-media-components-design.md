# Media components: redesigning the codec role (2026-11-08)

Design for mika-f/Celesta#782. It replaces the single `codec` role
(`docs/handoff/2026-11-05-codec-plugin.md`) with **media components**:
decoders, video encoders, audio encoders, and muxers, each declared by a
plugin and chosen separately. v0.10 is unreleased, so nothing here keeps
compatibility with the current `codec` role, its protocol, or the
exporter's `--preset` / `--crf` / `--bit-depth` / `--color-conversion` /
`--output-format` options.

Decided with the user one question at a time. The answers to the issue's
twelve open questions are summarized in [Decisions](#decisions); the rest
of this note specifies them.

## Goals

- A plugin can add one piece (an H.264 encoder, an AAC encoder, a WebM
  muxer, a decoder for one format) without implementing the rest.
- Video from one plugin and audio from another can go into a muxer from a
  third.
- Encoder settings belong to the encoder: the plugin describes them, and the
  editor's export dialog and the CLI are built from that description.
- Users choose components and settings when they export: a dialog in the
  editor, flags in the CLI. They can save presets for later use, globally
  for themselves or in the project, so a repository and its CI share them.
- Exports are not slower than on `main` today (#645's hard requirement).
  Frames reach the encoder through shared memory, pipelined. The second
  write of the output (today's video-only intermediate, remuxed with the
  audio) goes away when the audio is known before the render ends; when it
  is not, packets are staged once instead, which costs what today's
  intermediate does ([Audio](#audio)).
- Colors are correct and tagged: BT.709 throughout.
- Stills and image sequences are components too (built in, in process).
- First plugins beyond FFmpeg: OS-native ones (AVFoundation / VideoToolbox
  on macOS, Media Foundation on Windows).

## Non-goals (for this design)

- `@celesta.studio/web` stays on Mediabunny/WebCodecs, H.264 + AAC MP4
  only.
- GPU texture sharing with hardware encoders (IOSurface, DXGI shared handles,
  Vulkan external memory): the negotiation names are reserved and nothing
  else is done.
- HDR output, more than two audio channels, localized setting labels,
  plugin-provided presets, sniffing a file's first bytes, a disk cache of
  `describe`, and user-chosen decoder priority. Each has a place reserved
  where it matters ([Deferred](#deferred)).
- The ffmpeg-CLI plugin of #645 (raw frames piped into a user-installed
  `ffmpeg`, so no GPL binary ships with Celesta). The bundled FFmpeg plugin
  already keeps Celesta's own executables MIT/Apache-2.0, and the OS-native
  plugins give a codec set with no GPL code at all. The packet model makes
  such a plugin more work but not impossible: it would parse the CLI's
  Annex B / ADTS output to build `TrackConfig` (avcC, hvcC,
  AudioSpecificConfig) itself.
- Still image *input* (PNG/JPEG/GIF/WebP layers) stays in the renderer
  (`crates/renderer/src/image_source.rs`), in process.

## Decisions

| # | Question | Decision |
| --- | --- | --- |
| 1 | Granularity | Four component kinds: decoder, video encoder, audio encoder, muxer. Encoders return compressed packets (WebCodecs-style codec strings and decoder configuration); Celesta interleaves them and hands them to the muxer. |
| 2 | Capability discovery | Two layers. The manifest declares components statically, which is enough to list them and route files without starting a plugin. A runtime `describe` is authoritative: availability with a reason, the settings schema, and accepted formats. `describe` is called on demand only (when the export dialog opens, when an export starts, `--list-components --check`). It is cached in memory, keyed by the plugin folder, the manifest's `version`, and the executable's size and modification time ([Capability discovery](#capability-discovery)), and the cache is dropped on Reload Plugins. |
| 3 | Settings schema | A small custom vocabulary shaped like `registerComponent()`'s: `select` / `number` / `boolean` / `string`, with `group`, `advanced`, and `visibleWhen`. Celesta validates settings before `open`. No codec-neutral quality/speed setting, and no plugin presets. Presets go in `export-presets.json`, in the user settings folder and, optionally, at the project root, so a project's export settings travel with its repository and CI. |
| 4 | Selection | Components are named `<plugin>/<component>`. By default the muxer comes from the output's extension, and its default codecs are served by encoders from the muxer's own plugin first, then bundled plugins, then the search order. Components from project plugins are never chosen by default; a project's `default` preset counts as naming its components, with a warning (deviation 44, P3). A preset name is looked up in `--preset-file`, then the project's presets, then the user's; a project may name a default preset. The editor has an export dialog. The CLI takes `--preset`, `--preset-file`, `--muxer`, `--video-encoder`, `--video-option`, `--audio-encoder`, `--audio-option`, `--list-components`, and `--describe`. |
| 5 | Transport | Raw frames go through a shared-memory ring of N slots, sized at export start. Requests are pipelined in order, with up to N in flight, and answers come back in order, with no request IDs. Packets and audio go through the pipe. Decoding keeps #775's single slot. |
| 6 | Pixel formats and color | Celesta offers `rgba8`, `rgba16` (straight), `yuv420p`, `yuv420p10`, `nv12`, and `p010`, and the encoder picks one. Every encoder is told `bt709 / bt709 / bt709 / limited`. The GPU conversion moves from BT.601 to BT.709, and every container is tagged. HDR fields are reserved. |
| 7 | Audio | Interleaved `f32`, mono or stereo. The encoder picks the rate and channel count, and Celesta resamples. Audio streams to the encoder once the mix is done. Encoder delay and the loop pre-roll are expressed as negative timestamps. Every muxer that takes audio hides the encoder's delay; only muxers declaring `trimsLeadingAudio` are sent the loop pre-roll, which they hide too. `--audio-encoder none`. |
| 8 | Stills and sequences | Video encoders `png` / `jpeg` / `webp` and muxers `image` / `imageSequence`, as **built-in components**: in Celesta's process, through the same component model and `resolve_output`, with no plugin process (their crates are MIT/Apache, so no licensing reason to move them out). Plugins can add more image formats. Contact sheets are composed in Celesta and written through them. |
| 9 | Decoders | Routed by declared extension, trying each candidate's `probe` and moving on when one answers `unsupported`. Order: user-installed, then bundled, then approved project plugins; a project decoder in use is shown in the UI. Decoding processes are not shared with encoding or content plugins. |
| 10 | Web | Unchanged. |
| 11 | Exporter API | `ExportOptions.output: OutputChoice`, resolved by a single function every front end uses. A component registry replaces the global `set_codec`. |
| 12 | First plugins and order | Phases P1–P6 ([Phases](#phases)): FFmpeg and the test codec ported in P1–P2 (P2 split in three, BT.709 on its own), the built-in image components in P4, macOS native in P5, Windows Media Foundation in P6. |

## Components

A **component** is one capability a plugin's process serves:

| Kind | Does | Named by |
| --- | --- | --- |
| `decoder` | probe, exact-time and sequential video frames, whole-file audio | file extensions |
| `videoEncoder` | raw frames → compressed video packets | codec families it produces |
| `audioEncoder` | `f32` PCM → compressed audio packets | codec families it produces |
| `muxer` | packets of one video and at most one audio track → an output file (or files) | file extensions, and the codec families it accepts |

A component's reference is `<plugin id>/<component id>`, such as
`ffmpeg/libx264`. **Built-in components** use the reserved plugin ID
`builtin` (`builtin/png`), and Celesta refuses a plugin manifest with that
ID.

Built-in components run in Celesta's own process, through the same Rust
traits as in-process test providers ([Registry and routing](#registry-and-routing)).
They are listed and described like plugin components (their `describe` is
answered in process) and are never approved or disabled. They are kept for
pieces with no licensing or isolation reason to live in a plugin: the
image encoders and muxers ([Stills](#stills-and-sequences-p4)). Component IDs are unique within a plugin per kind. When
two installed plugins share an ID, the earlier in search order wins, and
listings show each one's folder, except that a project's plugin never
replaces a bundled media plugin ([Decoders](#decoders), deviation 7).

**Codec families** are the four-character names of ISO-BMFF sample entries
(and WebCodecs' codec strings), used in manifests and for matching:
`avc1`, `hvc1`, `av01`, `vp09`, `ap4h` (ProRes 4444), `apch` (ProRes 422
HQ), `mp4a`, `opus`, `lpcm`, `png`, `jpeg`, `webp`. Families are compared as
strings, so a plugin can use one Celesta does not know, after one
normalization. These spellings name the same bitstream with its parameter
sets in-band or out of band, and match each other: `avc1` = `avc3`,
`hvc1` = `hev1`. Which spelling a file gets is the muxer's choice, from the
`TrackConfig` it receives. A **codec string**,
such as `avc1.640028` or `mp4a.40.2`, is the full form an encoder reports
once it knows its configuration. Its family is the part before the first
`.`.

Deviation in P4 (`docs/handoff/2026-11-09-media-stills.md`): **the
built-in plugin has no folder** (deviation 59). It is registered with an
empty `directory` and no `executable`, and listings show its origin as
`built-in` and no folder: `--list-components` text and JSON, and MCP's
`list_components`.

## Manifest

The manifest's `roles` gains `media`, which replaces `codec` (an old
`codec` manifest now reads as an unknown role and is ignored). A `media`
plugin's `protocol` is `2`. Content plugins stay on protocol 1, and the host
accepts each version only for its role ([Manifest](#manifest): a plugin has
one of the two). The new key is `media`, because
`components` already means the module's Add-menu components:

```json
{
  "id": "ffmpeg",
  "version": "0.10.0",
  "protocol": 2,
  "roles": ["media"],
  "executable": { "macos-aarch64": "bin/celesta-codec-ffmpeg" },
  "media": {
    "decoders": [
      { "id": "ffmpeg", "name": "FFmpeg",
        "extensions": ["mp4", "m4v", "mov", "webm", "mkv", "avi", "mp3", "wav", "flac", "ogg", "m4a", "aac", "opus"] }
    ],
    "videoEncoders": [
      { "id": "libx264", "name": "H.264 (x264)", "codecs": ["avc1"],
        "inputFormats": ["yuv420p", "yuv420p10", "nv12", "p010", "rgba8", "rgba16"] },
      { "id": "prores", "name": "ProRes (prores_ks)", "codecs": ["ap4h", "apch"] },
      { "id": "vp9", "name": "VP9 (libvpx)", "codecs": ["vp09"] }
    ],
    "audioEncoders": [
      { "id": "aac", "name": "AAC", "codecs": ["mp4a"] },
      { "id": "opus", "name": "Opus", "codecs": ["opus"] },
      { "id": "pcm", "name": "PCM", "codecs": ["lpcm"] }
    ],
    "muxers": [
      { "id": "mp4", "name": "MP4", "extensions": ["mp4", "m4v"],
        "video": ["avc1", "hvc1", "av01"], "audio": ["mp4a", "opus"],
        "defaultVideo": "avc1", "defaultAudio": "mp4a", "trimsLeadingAudio": true },
      { "id": "mov", "name": "QuickTime", "extensions": ["mov"],
        "video": ["avc1", "hvc1", "ap4h", "apch"], "audio": ["mp4a", "lpcm"],
        "defaultVideo": "ap4h", "defaultAudio": "lpcm", "trimsLeadingAudio": true },
      { "id": "webm", "name": "WebM", "extensions": ["webm"],
        "video": ["vp09", "av01"], "audio": ["opus"],
        "defaultVideo": "vp09", "defaultAudio": "opus" }
    ]
  }
}
```

- A muxer with no `audio` list takes no audio track. A muxer may set
  `"sequence": true` when it writes several files from one pattern, as
  `imageSequence` does.
- `name` is English display text.
- A video encoder's `inputFormats` is the upper bound of the formats it may
  take, which sizes the shared-memory ring before its process starts
  ([Transport](#transport)). Without it, any format Celesta produces is
  possible, and the ring is sized for `rgba16`.
- A plugin is either `content` or `media`. A manifest with both roles is
  reported as a plugin problem and not loaded. A plugin that does both ships
  as two plugin folders. So `hello`'s protocol version follows from the one
  role.
- The manifest is an upper bound. `describe` cannot add components, and a
  component `describe` leaves out is unavailable.
- Parsing is lenient as before: unknown keys are ignored, and a malformed
  component is reported as a plugin problem and skipped, without
  dropping the plugin's other components.

## Protocol 2

The plugin protocol's framing is unchanged (`celesta_plugin::protocol`):
one JSON header line, optionally followed by a binary payload whose length
the header names. Each message below is the header's JSON; `+ payload`
marks one with a payload. Every request can be answered by
`{"type":"error","message","code"?,…}` instead.

### Handshake

| Request | Answer |
| --- | --- |
| `{"type":"hello","protocol":2,"frames"?:{"sharedMemory":{"handle","bytes"}}}` | `{"type":"hello","protocol":2,"roles":["media"],"frames"?:"sharedMemory"}` |
| `{"type":"describe"}` | `{"type":"described","components":[ComponentDescription]}` |

`frames` is #775's negotiation, unchanged, and offered to every media
process. A decoding process uses its segment as one slot, as today. An
encoding process uses it as a ring ([Transport](#transport)).

`ComponentDescription`:

```json
{
  "kind": "videoEncoder", "id": "libx264",
  "available": true,
  "settings": [ SettingField ],
  "inputFormats": ["yuv420p", "yuv420p10", "nv12", "p010", "rgba8", "rgba16"],
  "transports": ["sharedMemory", "pipe"]
}
```

- `available: false` comes with a `reason`, for example "this FFmpeg build has no libx264". A component that is not available has nothing else.
- `inputFormats` (video encoders): every format it can take under any settings, best first.
- `sampleRates` (audio encoders): a list, or `"any"`. `channels` (audio encoders): `[1, 2]`.
- `transports` (video encoders): `sharedMemory` and `pipe` today. `ioSurface`, `dxgiShared`, and `vulkanExternal` are reserved names that Celesta does not offer yet.
- Muxers and decoders describe `settings` only. A decoder needs nothing more.

### Decoding

| Request | Answer |
| --- | --- |
| `{"type":"probe","decoder","path"}` | `{"type":"probed","probe":MediaProbe}` |
| `{"type":"decodeVideo","decoder","path","time","sequential"}` | `{"type":"videoFrame","width","height","slot"?}` + RGBA8 payload when not in the slot |
| `{"type":"decodeAudio","decoder","path","sampleRate","channels"}` | `{"type":"audio","sampleRate","channels"}` + `f32` LE payload |

The requests gain `decoder`, the component's ID; the answers are those of
protocol 1. A
`probe` error with code `unsupported` means "not a file this decoder reads"
and moves routing to the next candidate. Other errors keep today's codes
(`notFound`, `noVideoStream`, `invalidTimestamp`, …).

### Encoding

Times are integers in a track's **timescale**. Video uses the frame rate's
numerator, so a frame lasts its denominator: 30000/1001 fps is timescale
30000 and duration 1001. Audio uses the encoder's negotiated sample rate (the
one Celesta resampled to), so time counts sample frames; `delay` and
`leadingAudio` count them too.

| Request | Answer |
| --- | --- |
| `{"type":"openVideoEncoder","component","settings","width","height","frameRate","timescale","offered":[PixelFormat],"color":Color,"ring"?:{"slots","slotBytes"}}` | `{"type":"videoEncoderOpened","session","input":PixelFormat,"alpha"}` |
| `{"type":"encodeVideoFrame","session","pts","slot"?}` (+ payload when not in a slot) | `{"type":"videoFrameAccepted","packets":[Packet],"config"?:TrackConfig}` + packet bytes |
| `{"type":"finishVideoEncoder","session"}` | `{"type":"videoEncoderFinished","packets","config"?}` + packet bytes |
| `{"type":"openAudioEncoder","component","settings","sampleRate","channels"}` | `{"type":"audioEncoderOpened","session","sampleRate","channels","delay"}` |
| `{"type":"encodeAudio","session","pts","frames"}` + interleaved `f32` LE | `{"type":"audioAccepted","packets","config"?}` + packet bytes |
| `{"type":"finishAudioEncoder","session"}` | `{"type":"audioEncoderFinished","packets","config"?}` + packet bytes |
| `{"type":"abortEncoder","session"}` | `{"type":"encoderAborted"}` |

- `settings` is the validated `{ key: value }` map. Values are JSON strings, numbers, or booleans.
- `input` is one of `offered`, chosen for these settings: `p010` for 10-bit, for example. `alpha` says whether the encoded video keeps the frame's alpha (ProRes 4444 does, ProRes 422 does not).
- `videoFrameAccepted` is sent once the plugin no longer needs the frame's slot. Celesta does not write that slot again until the answer.
  - With N slots, the plugin can encode straight from the slot and answer when the frame is encoded, with no copy of its own. Rendering and encoding still overlap across the other slots. This is the expected way.
  - A plugin whose encoder API takes ownership of a buffer (ez-ffmpeg's `write_owned`, for example) copies the frame out and answers then, at its own cost.
- `Packet`: `{"pts","dts","duration","key","bytes","additional"?:[{"id","bytes"}]}`. The packets' bytes follow the header back to back, in order: each packet's main bytes, then its additional data in the order listed.
  - `additional` is per-packet side data that a muxer stores beside the frame, as Matroska's BlockAdditions. Its first use is VP9's alpha plane (`id` 1), which libvpx emits as side data and WebM stores as a BlockAddition. A video `Track` with `alpha: true` tells the muxer to declare it (WebM's `BlockAdditionMapping`).
  - Packets come out whenever the encoder produces them: on any later answer of the same session, or on `finish`. An answer may carry none.
  - Audio packet times include the encoder's `delay`, so the first packets have negative `pts`.
- `TrackConfig`: `{"codec":"avc1.640028","description"?:base64}`. `description` is the decoder configuration a muxer needs (avcC, hvcC, esds' AudioSpecificConfig, OpusHead, …), as WebCodecs' `decoderConfig`.
  - It comes with the first packets, or earlier. VideoToolbox, for example, knows it only after the first frame.
  - Celesta opens the muxer once it has every track's config.
- `delay`: the encoder's priming, in sample frames (AAC's 1024 / 2112, Opus's pre-skip).

### Muxing

| Request | Answer |
| --- | --- |
| `{"type":"openMuxer","component","settings","output","tracks":[Track],"leadingAudio"?}` | `{"type":"muxerOpened","session"}` |
| `{"type":"writePackets","session","packets":[{"track","pts","dts","duration","key","bytes","additional"?}]}` + bytes | `{"type":"packetsWritten"}` |
| `{"type":"finishMuxer","session"}` | `{"type":"muxerFinished","files":[path]}` |
| `{"type":"abortMuxer","session"}` | `{"type":"muxerAborted","files":[path]}` |

- `Track` (video): `{"kind":"video","config":TrackConfig,"timescale","width","height","frameRate","color","alpha"}`.
- `Track` (audio): `{"kind":"audio","config","timescale","sampleRate","channels","delay"}`.
- Every muxer that takes audio must hide the audio track's `delay`: the packets before time 0 (an MP4/MOV edit list, WebM's `CodecDelay` and Opus pre-skip).
- `leadingAudio`: sample frames before 0 that the muxer must hide, beyond the encoder's `delay`. This is the loop pre-roll, sent only to muxers that declare `trimsLeadingAudio`. A muxer without the flag still hides `delay`.
- `files` lists what the muxer wrote (one file, or a sequence), so Celesta can report it, or remove it on abort.
- The output is written where Celesta says. Celesta handles overwrite checks and the temporary-file-then-rename (as today's exporter does).
- A `sequence: true` muxer gets `output` as a pattern Celesta chose (`name-%06d.ext`, today's naming in `stills.rs`), and the frame numbers come with the packets' `pts`. (Superseded in P4 by deviation 53 below: the pattern is `<staging>/frame-%06d.<ext>`, each frame is published as it is written, and a failure or cancel keeps the frames already published.)
  - Celesta knows every name in advance, so it checks each one for overwrite before the export.
  - On cancel, or when the process is killed, Celesta removes the files it named itself. It does not rely on `abortMuxer`'s `files`. (Superseded by deviation 53.)

Deviations in P4 (`docs/handoff/2026-11-09-media-stills.md`):

- **A sequence's files are published one at a time, and a failure or cancel keeps those already published** (deviation 53). Celesta stages a sequence in a hidden folder beside the output (`.celesta-<random>/`, removed when the export ends) and gives the muxer the pattern `<staging>/frame-%06d.<ext>`, where `<ext>` is the output's extension as given; only the last path component is a template, so user text in the output path (`50%-check.png` in `run%d/`) is never parsed. A `sequence: true` muxer must have each frame's file whole when the `writePackets` carrying that frame's packet is answered, and Celesta publishes it to its final name (`name-000090.ext`) then, without replacing an existing file unless `overwrite`. The removal on cancel above would break the tested still behavior (separate files stop at the failed frame, keeping those before it), and under `--overwrite` could only be done by deleting files the export never wrote, so Celesta removes only its staged files. Names are still all checked for overwrite before the export. When the muxer finishes, every selected frame must have had its file published, and a one-file muxer's `finish` must list the staged file; otherwise the export fails at the muxer stage (`… wrote no file for frame N`, `… wrote no image file`). A still selection sends frames in selection order (`--frames 3,0` sends frame 3's `pts`, then frame 0's), so a sequence muxer must not require increasing `pts`. A hard kill (SIGKILL, or SIGTERM without `--stdin-cancel`) can leave an empty or partial `.celesta-*/` staging folder beside the output, as a killed video export leaves its staged file.
- **One file is staged as `.celesta-<random>.<ext>`** beside the output (ruling 4 of the handoff), keeping the output's extension rather than a `.tmp` suffix, since the built-in muxers refuse a codec that is not the extension's (below). It is published when the muxer finishes, after the last cancel check, so **a one-file still cancelled after its frame rendered writes nothing** (ruling 5); before P4 it wrote the file and then reported `cancelled`.
- **Both built-in muxers refuse, at open, a video track whose codec family is not the output's extension's** (ruling 3), `builtin/imageSequence` too: `invalidSettings` with the key `tracks`. Before the open, the still path already reports the mismatch as `incompatible_components` ([Stills](#stills-and-sequences-p4)).

### Deadlines and the window

The send window counts every request to the process, whichever session it
belongs to. Deadlines per request, measured from when it is sent:

| Request | Deadline |
| --- | --- |
| `hello`, `describe` | 30 s |
| `probe`, `decodeVideo` | 60 s |
| `decodeAudio` | 10 min |
| `openVideoEncoder`, `openAudioEncoder`, `openMuxer` | 60 s |
| `encodeVideoFrame`, `encodeAudio`, `writePackets` | 60 s, counted from when the request ahead of it in the window was answered, so a full encoder queue does not eat the deadline of requests behind it |
| `finishVideoEncoder`, `finishAudioEncoder` | 10 min |
| `finishMuxer` | 1 h (a `+faststart` rewrite of a large file) |
| `abortEncoder`, `abortMuxer` | 5 s, after which the process is killed |

As built in P2b, the deadline of `encodeVideoFrame`, `encodeAudio`, and
`writePackets` counts from the later of its send and the answer to the
request ahead of it, so one sent with nothing ahead counts from its send (`DeadlineFrom::{Sent, PreviousAnswer}`
on `PluginChild::send`).

Deviation in P2b (`docs/handoff/2026-11-08-media-pipelining.md`): **an abort with requests in flight drops the process** (deviation 22). Its stdin closes, and it is killed half a second later, instead of being sent `abortEncoder` / `abortMuxer` behind them, whose 5 s deadlines count from the send and would otherwise wait on up to `slots` encodes first. The abort messages are sent only when nothing is in flight. The staged output and the spill file are Celesta's own and are removed either way.

### Errors

New codes:

| Code | Meaning |
| --- | --- |
| `unsupported` | `probe` only: the decoder does not read this file. |
| `invalidSettings` | Carries `key` and `message`, for a combination only the plugin can judge. |
| `componentUnavailable` | Carries `component` and `reason`, for a component that turned out unusable at `open`. |

Every other error keeps protocol 1's behavior: an unknown code is
`MediaError::Codec(message)`, and the plugin keeps running.

## Capability discovery

- **Listing** (`InstalledPlugins::media()` in `celesta-plugin-host`) reads manifests only, and covers every plugin that may run under the usual search order, approval, and disabled list. It returns `MediaComponentInfo { reference, kind, name, origin (built-in / bundled / user / project), manifest fields }`. `MediaRegistry` adds the built-in components to it.
- **The `cargo install` fallback is kept.** Today `celesta-codec-ffmpeg` beside the running executable serves as the codec when no plugin folder has one (`sibling_codec`, `manifest.rs`), and `cargo install` users rely on it.
  - The program answers `celesta-codec-ffmpeg --manifest` with its manifest JSON, without starting the protocol. `media()` runs it once for the sibling and caches the answer by the cache key below.
  - This is the one case where listing starts a process: `cargo install` cannot install a data file beside the program.
  - The sibling counts as bundled.
- **A process started only to describe** is offered no shared-memory segment at `hello`.
- **`describe`** is called by `celesta_media::MediaRegistry::describe(plugin)`, which starts the plugin's process, says `hello`, asks `describe`, and stops it. It is called:
  - when the editor's export dialog opens (in the background, per plugin; the dialog shows "Checking…" on that plugin's components until it answers)
  - when an export starts (for the plugins of the chosen components)
  - for `--list-components --check` and `--describe`

  It is never called during discovery, project open, or editor start-up.
- **At export start**, a plugin whose `describe` is not cached is described by the export's own process: the ring is sized from the manifest's `inputFormats`, and the process then answers `hello`, `describe`, and the `open`s. Each plugin is started once per export, not once to describe it and again to export.
- **Cache.** The answers live in the process's memory, keyed by (plugin folder, the manifest's `version`, and the executable's size and modification time). Approval fingerprints exist only for plugins that need allowing, so bundled plugins have none.
  - A changed program misses the cache.
  - The editor's Reload Plugins clears it.
  - A `describe` that fails (the process cannot start, a missed deadline, a bad answer) is not cached. Its components count as unavailable, with the failure as their reason.
- **Mismatch.** Celesta ignores components that `describe` returns but the manifest lacks (and warns once, on stderr). Components the manifest has but `describe` leaves out count as unavailable, with the reason "the plugin did not describe it".

Deviations in P1 (`docs/handoff/2026-11-08-media-decoders.md`):

- **`describe` and its cache move to P2a** (deviation 1). Nothing consumes them in P1: decoders route by `probe`, and the export dialog and `--list-components` arrive later. P1's `MediaRegistry` holds the listing and the process factories only.
- **P1's manifest parses `media.decoders` only** (deviation 3). The encoder and muxer keys arrive with P2a; serde already ignores keys it does not know, so a manifest that has them still loads. `InstalledPlugins::media()` returns `MediaPlugin { id, version, executable, directory, origin, decoders }`, one per plugin rather than one `MediaComponentInfo` per component, and the built-in components arrive with P4.

Deviations in P2a (`docs/handoff/2026-11-08-media-encoders.md`):

- **A `describe` from the export reuses the export's process** (deviation 10). `resolve_output` takes a `describe` callback; the export's `ExportSession` answers it from the process it starts for that plugin, and keeps that process for the `open`s. `MediaRegistry::describe` (start, `hello`, `describe`, stop) serves `--list-components --check`, `--describe`, and MCP. Both write the same cache, which lives in the `MediaRegistry` and is dropped with it on Reload Plugins.
- **`InstalledPlugins::media()` stays one `MediaPlugin` per plugin** (deviation 15; P1's deviation 3, kept), now with the whole `media` manifest (decoders, video and audio encoders, muxers). `MediaRegistry::components()` gives the spec's one `MediaComponentInfo` per component.

## Settings schema

`SettingField`, in the shape of `registerComponent()`'s fields
(`crates/react-bridge/src/types.rs`, `ComponentPropertyField`) so the
editor's property controls can be reused:

```json
{ "key": "rateControl", "type": "select", "label": "Rate control", "group": "Rate control",
  "options": [ { "value": "crf", "label": "Constant quality (CRF)" },
               { "value": "vbr", "label": "Average bitrate" } ],
  "default": "crf" }
{ "key": "crf", "type": "number", "integer": true, "min": 0, "max": 51, "step": 1,
  "default": 18, "label": "CRF", "group": "Rate control",
  "visibleWhen": { "rateControl": "crf" },
  "description": "Lower is higher quality and larger." }
{ "key": "bitrate", "type": "number", "integer": true, "min": 100000, "unit": "bit/s",
  "default": 8000000, "label": "Bitrate", "group": "Rate control",
  "visibleWhen": { "rateControl": "vbr" } }
{ "key": "tune", "type": "string", "default": "", "label": "Tune", "advanced": true }
{ "key": "faststart", "type": "boolean", "default": true, "label": "Fast start" }
```

- **Types.**
  - `select`: `options` with `value` (a string) and `label`.
  - `number`: `min`, `max`, `step`, `integer`, and `unit`.
  - `boolean`.
  - `string`: free text, for advanced use.
- **Common fields.** `key`, `type`, `label`, and `default` are required. `description`, `group`, `advanced`, and `visibleWhen` are optional.
  - `visibleWhen` maps keys to a value, or to a list of values. Every listed key must match for the field to show and apply.
  - A hidden field is not sent.
- **Validation** (`celesta_media::settings::validate`) runs before any `open`, and fails with `invalid_option` naming the key. It checks:
  - every key is known and of its type
  - numbers are in range, and whole when `integer`
  - `select` values are among the options
  - fields hidden by `visibleWhen` are not given

  Missing keys take their defaults. The plugin may still refuse a combination with `invalidSettings`.
- **No codec-neutral quality/speed setting** and no plugin presets. Defaults should be good on their own, and user presets carry "my usual settings". A future `hint: "quality" | "speed"` on a field could drive a simple mode.

### Presets

Presets are kept in two places, both named `export-presets.json` and both owned by `celesta-exporter` (`presets.rs`), which the editor already depends on.

- **User presets** are in `celesta_plugin_host::user_settings_dir()`, beside (not inside) `settings.json`, which stays "what code Celesta may run". The file is written atomically, as `settings.json` is.
- **Project presets** are at the project root (`celesta_plugin_host::project_root`), so they travel with the repository and CI.
  - The editor writes them only when asked ("Save as preset… → This project").
  - The file is plain data and needs no approval. A component it names still has to be installed and, for a project plugin, approved.

The user file:

```json
{
  "version": 1,
  "presets": [
    { "name": "YouTube 1080p",
      "muxer": "ffmpeg/mp4",
      "video": { "component": "ffmpeg/libx264", "options": { "crf": 18, "preset": "slow" } },
      "audio": { "component": "ffmpeg/aac", "options": { "bitrate": 192000 } } }
  ],
  "last": { "muxer": "ffmpeg/mp4",
            "video": { "component": "ffmpeg/libx264", "options": {} },
            "audio": { "component": "ffmpeg/aac", "options": {} } }
}
```

- `audio` may be `"none"`.
- `last` is the dialog's most recent choice. It is kept in the user file only.
- The project file has the same `version` and `presets`, no `last`, and an optional `"default": "<name>"`: the preset an export of that project uses when no `--preset` and no component flags are given.
- **Name lookup** for `--preset <name>` (and MCP's and TS's `preset`):
  1. the file given by `--preset-file <path>`, when given
  2. the project's file
  3. the user's file

  The first match wins. A project preset that shadows a user preset of the same name gets a warning (and a user file that cannot be read for that check gets one too, deviation 42).
- **Stale presets.** After a plugin update, a preset may hold an unknown key, a value out of range, or a component that is gone. The response depends on who uses it:
  - The editor drops the key or resets the value to its default, falls back to the default component, and shows a warning on the preset.
  - `--preset` warns on stderr (and in the JSON report's `warnings`) and does the same.
  - A value given explicitly on the command line, in MCP, or in TS fails with `invalid_option`.

Deviations in P3 (`docs/handoff/2026-11-09-export-presets-dialog.md`):

- **A presets file that cannot be read or parsed is a new error, `invalid_presets`** (deviation 42; `ExportError::InvalidPresets { path, message }`), as is a `version` other than 1. It is raised only when a lookup has to find a name in that file: a broken user file does not fail an export that names no preset in a project without a `default`, nor one whose preset (or `default`) is found in the project file, where the shadow check warns ``your presets file <path> could not be read, so it was not checked for a preset of the same name: <message>`` instead; a missing file is skipped. A `--preset-file` (or MCP `presetFile`) that is missing or invalid stays P2a's usage error, now checked with the same parser (`PresetsFile::load`).

## Selection

### Resolving an output

`celesta_exporter::resolve_output(choice: &OutputChoice, output: &Path,
registry: &MediaRegistry) -> Result<ResolvedOutput, ExportError>` is the one
place choices become components. It runs these steps in order:

1. **Preset.** With `preset`, looked up as [Presets](#presets) says, its components and options are the base. Without `preset` and without any component given, the project's `default` preset, if it names one, is the base. A name found in no file is `unknown_preset`, with a hint when it is a libx264 preset name (`--preset ultrafast` → "use `--video-option preset=ultrafast`").
2. **Muxer.** The explicit or preset muxer, or else the first muxer whose `extensions` include the output's extension. Muxers are ranked by origin: built-in components, then bundled first-party plugins, then user-installed plugins in search order. With none, the error is `unsupported_output`, listing the extensions available.
3. **Video encoder.** The explicit or preset one, or else the first available encoder that produces the muxer's `defaultVideo`, ranked as below. An explicit one whose codecs the muxer does not accept is `incompatible_components`.
4. **Audio encoder.** The same as step 3, against `defaultAudio`. It is `none` when the choice says so or the muxer takes no audio.
5. **Availability.** Each chosen component's `describe` must say it is available. Otherwise the error is `component_unavailable`, with its reason (not installed, not approved, unavailable: "this FFmpeg build has no libx264").
6. **Options.** Explicit options are layered over the preset's and validated against each component's schema, which step 5's `describe` brought.

**Ranking** default encoder candidates:

1. the muxer's own plugin
2. bundled first-party plugins
3. user-installed plugins in search order

Approved project plugins are never chosen by default, only when named; a
project's `default` preset counts as naming its components, with a warning
(deviation 44, P3).
Among bundled plugins the order is fixed, with FFmpeg first. So on macOS,
P5's bundled `avfoundation/mp4` does not replace `ffmpeg/mp4` as the default;
it is chosen only by name or preset.

Deviations in P3 (`docs/handoff/2026-11-09-export-presets-dialog.md`):

- **`resolve_output` also takes the files presets are looked up in** (deviation 40): `resolve_output(choice, output, registry, presets: &PresetLookup, describe)`, beside P2a's `describe`, and `ResolvedOutput` gains `warnings` (the shadowing and stale-preset warnings, and the two below). A front end decides which files an export may read. `resolve_preset_output(choice, base, output, registry, describe)` runs steps 2–6 over an already-found preset, for the editor's `last` and its dialog.
- **`unknown_preset` says when the name came from the project's `default`** (deviation 43): `ExportError::UnknownPreset` gains `default_in: Option<PathBuf>`, and a `default` naming a preset found in no file reads "the default preset `x` of <path> is in no presets file", with the same code and no x264 hint.
- **A project's `default` preset counts as naming its components** (deviation 44). Step 1's default applies when the choice has no `preset`, no muxer, no video component, and audio `Encode` with no component (`--audio-encoder none` counts as a component flag; options alone layer over the default). A default that names a project plugin's component uses it (the plugin must still be approved, or it is not installed and the preset falls back as stale), and the export warns "the project's default preset `{name}` uses {reference}, from the project's plugin {plugin}", once per such component. An explicit `preset` naming one does not warn. The CLI and MCP search no project plugin folders, so the warning is reached only through a registry that has them; the editor's dialog marks them "(project)".
- **A named or preset muxer that does not list the output's extension** is used anyway (P2a's R7), now with the warning "{muxer} does not write .{ext} files; the file is written as {muxer name} anyway". A muxer chosen by the extension never warns.

Deviations in P4 (`docs/handoff/2026-11-09-media-stills.md`):

- **Built-in muxers and sequence muxers serve still exports only** (deviation 50). A video export's default muxer (step 2) is never one of the `builtin` plugin's and never a `sequence: true` muxer (`celesta_exporter::serves_video`), and a video export that names one, or whose preset does, fails before rendering with `component_unavailable` ("it writes still images; select frames to export stills"). The editor's dialog does not list them. `celesta-exporter p.json out.png` without `--frame` keeps P3's `unsupported_output` and its `--frame` hint, rather than rendering every frame into a one-frame muxer. Image sequences of a whole video are not built.
- **Stills name their default components by extension, not by ranking** (deviation 51). A muxer has one `defaultVideo`, and `builtin/image` writes three formats, so the still path names `builtin/image` (one file: one frame or a contact sheet) or `builtin/imageSequence` (several files) and `builtin/<png|jpeg|webp>` for the output's extension (`jpg` → `jpeg`), then resolves them with `resolve_preset_output` and no base. A plugin's image muxer or encoder serves a still only when named (`--muxer`, `--video-encoder`), so [Stills](#stills-and-sequences-p4)' "ranks after `builtin/image`" holds by construction. A named muxer must match the file count (a `sequence: true` muxer for several files, any other for one), else `invalid_selection`. With no muxer named, an extension no built-in muxer lists keeps P3's `invalid_selection` "still output must use the .png, .jpg, .jpeg or .webp extension", and a named encoder whose codec is not the extension's is `incompatible_components` ("builtin/image writes .png files as png, and builtin/webp writes webp") before rendering. Stills never read a presets file: no `--preset`, no project `default`, no `last`.
- **"FFmpeg first among bundled" is enforced by plugin id** (deviation 58): `celesta_plugin_host::FFMPEG_PLUGIN_ID = "ffmpeg"`, in `celesta_media::default_rank` (built-in 0, the bundled FFmpeg 1, other bundled 2, user 3, a project's none) and in the decoder order ([Decoders](#decoders)); other bundled plugins keep search order after it. For encoders, the muxer's own plugin still ranks first, then `default_rank`'s order, so built-in encoders rank before bundled ones; they write only `png`, `jpeg`, and `webp`, which no video muxer takes.
- **FFmpeg stays the default on macOS, on purpose** (decided in P5's review). With the AVFoundation plugin bundled, a macOS export could be written without FFmpeg, but the default stays `ffmpeg/*`: it gives the same output on every system and keeps the default path the one P2a and P2b checked against `main`. The choice changes no package's terms, since the GPL FFmpeg plugin ships either way and the AVFoundation plugin (MIT OR Apache-2.0, Apple's frameworks only) adds no obligation. VideoToolbox stays available by name or preset. An FFmpeg-free macOS package is a separate question and not a goal of this work.

### Editor

- Export opens a dialog that replaces today's save panel. From top to bottom:
  - Preset
  - Container (muxer)
  - Video encoder and its fields (grouped; advanced fields collapsed; `visibleWhen` applied live)
  - Audio encoder (or None) and its fields
  - the existing range and loudness controls
  - "Save as preset…"
  - Export…, which opens the save panel with the muxer's first extension
- The preset list groups project presets ("This project") above user presets. "Save as preset…" asks which file to save to.
- Components show their plugin. A project plugin's components are marked "(project)". Unavailable ones are listed but disabled, with their reason. While `describe` is running, they show "Checking…".
- The choice is remembered in `last`.
- The editor's GUI follows the GPUI Kit design guides (no gradients, no uppercase labels, radius 0, lucide icons).

### CLI (`celesta-exporter`)

```text
--preset <name>                 a preset, from --preset-file, the project, or the user
--preset-file <path>            an export-presets.json to look names up in first
--muxer <plugin/id>
--video-encoder <plugin/id>
--video-option <key=value>      repeatable; parsed by the field's type
--audio-encoder <plugin/id|none>
--audio-option <key=value>      repeatable
--list-components [--check]     manifests only; --check also runs describe
--describe <plugin/id>          availability and settings schema
```

`--json` gives `--list-components` and `--describe` machine-readable
output.

`--describe <plugin/id>` names no kind, while IDs are unique only per kind
within a plugin ([Components](#components)). As built
(`celesta_exporter::describe_component`), it looks the reference up as a
muxer, then a video encoder, then an audio encoder, then a decoder, and
the first match wins.

- Removed: `--output-format` (the extension and `--muxer` decide), `--preset <x264 name>`, `--crf`, `--bit-depth`, and `--color-conversion` ([Pixel formats](#pixel-formats-and-color)).
- `--loop` stays and means the loop pre-roll ([Audio](#audio)).
- Stills keep `--frame` and their other flags. Their format comes from the extension, routed to the built-in image components (P4).

Deviation in P2a (`docs/handoff/2026-11-08-media-encoders.md`): **`--loop` is accepted for every video output** (deviation 17). It changes nothing for a muxer without `trimsLeadingAudio` (FFmpeg's WebM), as [Audio](#audio) says, instead of today's usage error outside MP4. MCP's `loop` and `@celesta.studio/exporter`'s follow.

Deviation in P4 (`docs/handoff/2026-11-09-media-stills.md`): **stills take `--muxer`, `--video-encoder`, and `--video-option`** (deviation 55), so the built-in settings are reachable; MCP's `render_frames` and `contact_sheet` and `@celesta.studio/exporter`'s `exportStills` take `muxer` and `videoEncoder`. `--quality` stays and sets the encoder's `quality` option; giving it with `--video-option quality=` (MCP: `videoEncoder.options.quality`) is a usage error. `--preset`, `--preset-file`, and the audio flags stay video-only.

### MCP and `@celesta.studio/exporter`

Both mirror the CLI: `{ preset?, presetFile?, muxer?, videoEncoder?: { id?, options? },
audioEncoder?: { id?, options? } | "none" }`, replacing `preset` / `crf` /
`bitDepth` / `colorConversion` / `format`. They gain a
`listComponents({ check? })` and a `describeComponent(id)`; MCP gets them as
tools. `describeComponent(id)` and MCP's `describe_component` carry no kind
either and resolve `id` as `--describe` does (they run the same
`describe_component`): muxer, video encoder, audio encoder, decoder, the
first match winning.

With no codec-neutral quality setting, a non-default export has to know the
chosen encoder's own keys. Agents call `describeComponent` first. The Agent
Skill's `references/export.md` documents the bundled FFmpeg plugin's keys
as of the release it ships with, and points to `--describe` for the rest.

## Exporter API

```rust
pub struct ExportOptions {
    // … overwrite, range, render_quality, driver, subtitles, looping, loudness, plugins
    pub output: OutputChoice,
}

pub struct OutputChoice {
    pub preset: Option<String>,
    pub muxer: Option<ComponentRef>,
    pub video: EncoderChoice,
    pub audio: AudioChoice,
}

pub struct EncoderChoice {
    pub component: Option<ComponentRef>,
    pub options: BTreeMap<String, SettingValue>,
}

/// `Encode` with no component takes the muxer's default audio encoder.
pub enum AudioChoice { Encode(EncoderChoice), None }

pub enum SettingValue { String(String), Number(f64), Boolean(bool) }
```

- **Removed:** `VideoFormat`, `VideoEncoding`, `EncoderPreset`, `BitDepth`, `ColorConversion`, `MAX_CRF`, and their `Unknown*` errors. From `celesta-media`: `Container`, the old `PixelFormat`, `EncoderSettings`, `MuxRequest`, `CodecProvider`'s encoder and mux methods, `set_codec`, and `codec`.
- **`ExportedFile::Video`** carries `{ muxer, video: { component, codec }, audio: Option<{ component, codec }>, alpha, files }` instead of `format`. The JSON report's `output` has the same shape, and `alpha` now comes from the encoder's `videoEncoderOpened`.
- **Error codes** (`report.rs`): `component_unavailable`, `incompatible_components`, `invalid_option`, `unknown_preset`, and `no_decoder` (which replaces `no_codec` for reading). They replace `invalid_crf`, `encoder_unavailable`, and `no_codec`. `unsupported_output` stays, with the new meaning.
- **Progress dashboard** (`progress.rs`): labels come from component names and the settings that differ from their defaults ("H.264 (x264) · CRF 20 · slow"). This replaces the hard-coded "VP9 CRF 20" and "ProRes 4444".

Deviations in P2a (`docs/handoff/2026-11-08-media-encoders.md`):

- **Progress labels come from an `ExportProgress::Output` event** (deviation 12), sent once `resolve_output` succeeds and before rendering, since the dashboard starts before any plugin has been described. `--json-progress` gains the event `output` (`muxer`, `muxerName`, and `video` and `audio` as `{ component, name, settings }`, or `audio: null`). In P4 a still export sends it too, once its components resolve and before rendering, with `audio: null`, and the dashboard names a still's components from it.
- **`EncoderChoice.options` holds `SettingInput { Text, Value }`** (deviation 24), not `BTreeMap<String, SettingValue>`. The CLI's `key=value` text can only be typed against the component's schema, which arrives with `describe` at resolve, so it stays `Text` until then; MCP gives typed values (`Value`). `@celesta.studio/exporter` runs the CLI and passes its options as `--video-option`/`--audio-option` text, so they arrive as `Text` too: the package reads `crf: '18'` as 18, where MCP refuses a string for a number.

Deviations in P3 (`docs/handoff/2026-11-09-export-presets-dialog.md`):

- **`ExportOptions::default()` reads no presets file** (deviation 41): `ExportOptions.presets: PresetLookup` (`file`, `project`, `user`, all `None` by default). The CLI and MCP set it from the export's source (`PresetLookup::for_source`), so a library caller, and every test, is not changed by a file in the user's home. The editor's dialog reads the files itself and exports an explicit choice. Each of `ResolvedOutput`'s `warnings` (deviation 40) reaches the caller as `ExportProgress::Warning`, before `ExportProgress::Output`, and the JSON report's `warnings`.
- **Error codes** gain `invalid_presets` (deviation 42).

## Registry and routing

`celesta_media::MediaRegistry` replaces the one codec plugin `set_codec` chose. It is made
from `InstalledPlugins::media()` (or, in tests, from in-process providers)
and holds:

- the component listing and the `describe` cache
- a process factory per plugin (`MediaProcess`, the successor of `CodecProcess`)

`MediaBackend::new(registry: Arc<MediaRegistry>)` takes it explicitly.
The editor, the exporter, the MCP server, and the React bridge's probes
each hold one registry, replaced on plugin discovery. Requests can then go
to different plugins, which resolves #782's problem 9.

Deviation in P1 (deviation 2): **a process-wide registry stays.**
`celesta_media::set_registry` and `registry` replace `set_codec` and
`codec`, and `MediaBackend::new()` reads the registry lazily, on first use,
as it read the codec. A backend made before discovery (the editor's
long-lived audio worker) must get the registry chosen later.
`MediaBackend::with_registry` is the explicit form. Requests still go to
different plugins, since the registry holds them all.

The in-process provider trait splits along the component kinds, with
`MediaProvider` advertising which a provider has:

- `Decoder`: `probe`, `decode_video`, `decode_audio`, as `CodecProvider`'s are today, plus the decoder ID
- `VideoEncoder`: `open`, `encode`, `finish`, `abort`, mirroring the protocol's encoding messages
- `AudioEncoder`: the same four, for audio
- `Muxer`: `open`, `write`, `finish`, `abort`

Their exact signatures are settled in the implementation plan of the phase
that introduces them (P1 for `Decoder`, P2 for the others), following the
messages above one to one.

Deviations in P2a (`docs/handoff/2026-11-08-media-encoders.md`):

- **`MediaError`'s variant names stay** (deviation 14): `Codec`, `CodecUnavailable`, and `CodecStopped` keep their names, and only their messages say "media plugin". Renaming them would touch every crate for no change in behavior.
- **Export processes are started by the same factory as decoding ones** (deviation 16), told their purpose by a `ProviderUse`: `Describe` and `Export` processes are offered no shared memory in P2a (P2b gives `Export` the ring), and `Decode` keeps #775's slot.

`celesta_media::protocol::serve` dispatches protocol 2 to whichever the
program registers. In-process implementations (tests, benches, and
`FfmpegCodec` in its own tests) use the same traits.

## Decoders

- **Candidates** for a path are the decoders whose `extensions` include its extension (case-insensitively). When none do, or the path has no extension, every decoder is a candidate. That intentionally starts each media plugin's decoding process in turn, stopping at the first that reads the file, since such files are rare.
- **Order**:
  1. user-installed plugins (search order)
  2. bundled plugins (FFmpeg first)
  3. approved project plugins (search order)

  User-installed plugins come first because a decoder that cannot read a file only costs a fallback. Project plugins come last, so a project cannot take over decoding of files Celesta can already read (#724's problem 3).
  - Deviation in P1 (deviation 7): **a project's plugin cannot replace a bundled media plugin of the same id.** Plugin IDs are otherwise settled by search order ([Components](#components)), and project folders are searched before the bundled one, so an approved project plugin with the id `ffmpeg` would hide the bundled FFmpeg and decode every file in its place. Discovery therefore skips a project's plugin whose id a media plugin in the bundled folder has, and reports it (`DiscoveryError::Invalid`: "a project plugin cannot replace Celesta's bundled media plugin `<id>`"), which the editor shows among the skipped plugins in Problems. `InstalledPlugins::media()` likewise keeps the `cargo install` sibling over a project media plugin with its id, which it leaves out (said on stderr). The project's plugin takes no id, so a user-installed plugin with that id still comes first. Other ids keep "the earlier in search order wins".
- **Fallback.**
  - `probe` goes to each candidate in turn. An `unsupported` error moves on to the next. Any other error, or a candidate whose process cannot start, also moves on, but is remembered.
  - The first successful probe wins, and the path is pinned to that decoder in `MediaBackend`'s probe cache. A `decodeVideo` or `decodeAudio` for a path not pinned yet routes it first, with a probe. Its later `decodeVideo` and `decodeAudio` go there, and errors after a successful probe do not fall back.
  - When every candidate fails, the error is the most specific one remembered: `notFound`, or `noVideoStream` from a decoder that read the file. Otherwise it is `MediaError::Unsupported { path, tried }`. With no decoder installed at all, it is `MediaError::NoDecoder`.
    - As built in P1, the ranking is `NotFound` > `NoVideoStream` > the first other error > `Unsupported { tried }`, and a candidate stopped on purpose (`CodecStopped`: a missed deadline, or the kill switch) ends routing.
    - Deviation in P1 (deviation 4): **a path that does not exist fails with `NotFound` before any plugin is started.** Paths are absolute and local ([Components](#components)).
    - Deviation in P1 (deviation 5): **the exporter reports `MediaError::Unsupported` with its own code, `unsupported_media`**, beside `no_decoder` (with the install hint). `no_codec` stays for encoding with no media plugin.
- **Processes.** Each `MediaBackend` has one process per decoder plugin, started on first use and reopened once on `CodecUnavailable` as today. The process gets #775's single shared slot. It is not shared with encoding (an export's components) or with content plugins.
  - Deviation in P1 (deviation 6): until P2a, a backend's legacy encoder (the registry's first bundled plugin, else its first plugin in search order whose origin is not `Project`, else none, which is `NoCodec`) may share its process with that plugin's decoder. Today's callers use separate backends for encoding and decoding; P2a's per-export components separate them.
- **Visibility.** `MediaBackend` records which decoder served each path. When a project plugin's decoder serves any file, the editor's Plugins panel shows "<plugin> decodes <n> files in this project" with the files. Problems gets a matching info entry.
- **Editor problem.** "No codec plugin can run" becomes "No media decoder can run", with the same reasons as today.

## Transport

- **Processes.** For an export, Celesta starts one process per plugin among the chosen components. FFmpeg's encoder, audio encoder, and muxer share one. An AVFoundation video encoder with FFmpeg's audio and muxer makes two.
- **The ring.** Before starting the process that serves the video encoder, Celesta knows the frame size and the formats in the intersection of the encoder's `inputFormats` (from `describe`) and what the renderer can produce.
  - As built, which `inputFormats` size it depends on the `describe` cache ([Capability discovery](#capability-discovery)): when the plugin's `describe` is not cached, the export's own process is started to describe it, before the encoder is chosen, so its ring is sized from the manifest's `inputFormats` (of all the plugin's video encoders); when it is cached, the process starts at the open, and the ring is sized from the chosen encoder's described `inputFormats`.
  - It creates a segment of `slots × slotBytes`, where `slotBytes` is the largest of those formats at the frame size, plus the header. `slots` is 3 by default.
  - It offers the segment at `hello` (#775's mechanism: inherited at spawn on Unix, `DuplicateHandle` on Windows) and declares the layout in `openVideoEncoder`'s `ring`.
  - Each slot is a seqlock region exactly as #775's slot: a generation, then the bytes. Offsets and capacities are Celesta's.
  - A codec that declines the segment at `hello` gets frames as payloads (`pipe`).
- **Pipelining.** `PluginChild` gains a send window: up to `slots` requests in flight to one process.
  - Answers come in order, one per request, since a plugin serves its input in order. No request IDs are needed. An answer of the wrong kind or out of order kills the plugin as today.
  - Each request keeps its own deadline. `PluginChild::request` stays as the window of one.
  - The window is per process, not per session. When one process serves the video encoder, the audio encoder, and the muxer, a `writePackets` waits behind the `encodeVideoFrame`s sent before it. A plugin serves its requests one at a time in any case, so a per-session window would not help. Muxing is cheap next to encoding, and Celesta sends packets in batches, so this is accepted.
- **The flow.**
  1. Celesta copies the GPU readback into a free slot and sends `encodeVideoFrame{slot}` without waiting.
  2. When the window is full, it waits for the oldest answer, which frees that slot and brings packets.
  3. Packets go to the interleaver, which sends `writePackets` to the muxer through the same window.

  Rendering, the slot copy, encoding, and muxing then overlap.
- **Packets and audio** go through the pipe: they are small next to raw frames. The largest are ProRes 4444 at 4K, a few MB a frame.
- **The interleaver** (`celesta-exporter`) orders packets by decode time in seconds across the tracks.
  - It writes video packets once audio packets up to the same time are known, or when there is no audio.
  - Video packets that arrive before the mix is done are buffered in memory up to 256 MB, then spilled to a temporary file beside the output.
- **Packets twice through Celesta.** Even when an encoder and its muxer share a process, packets travel through Celesta and back. That is accepted for its small size and uniformity. A same-process shortcut can come later.
- **The escape hatch.** `CELESTA_CODEC_SHARED_MEMORY=0` turns off the encode ring too.
- **Decoding** keeps #775's single slot, unchanged.

Deviations in P2b (`docs/handoff/2026-11-08-media-pipelining.md`):

- **The plugin copies each frame out of its slot before encoding** (deviation 19), one `memcpy` through the seqlock's reader (as the decoder copies in), rather than encoding straight from the mapping. `celesta-plugin`'s rule that neither side holds a reference into the segment stands, and libavcodec copies a non-refcounted frame on `avcodec_send_frame` anyway, so encoding from the mapping would save nothing without an `AVBufferRef` over shared memory. The slot is still held until `videoFrameAccepted`, so Celesta's side is as above.
- **The ring is sized from the formats Celesta can offer at all** (deviation 20): `PixelFormat::OFFERABLE` (`yuv420p`, `yuv420p10`, `nv12`, `p010`, `rgba16`, `rgba8`, as of P2c) intersected with the encoder's `inputFormats`, not with what this export's renderer offers, since the process starts (and may `describe`) before the renderer exists. A software renderer's smaller offer costs address space, not memory. An empty intersection, or an encoder without `inputFormats`, sizes for `rgba16`, as for an unknown encoder.
- **A ring has a ceiling, `MAX_RING_BYTES = 256 MiB`** (deviation 21). `ring_layout` drops slots from `RING_SLOTS = 3` toward 1 until the segment fits, and gives no ring when one slot does not (an 8192×4320 `rgba16` frame); those frames go through the pipe, pipelined. The spec sizes the ring for any frame. A slot holds the largest offerable format's frame, but each frame writes only its own bytes, so the ring's pages are not all written: with the GPU's `yuv420p`, which libx264 chooses for 8-bit output while its `inputFormats` (`yuv420p`, `yuv420p10`, `nv12`, `p010`, `rgba8`, `rgba16`, as of P2c) size the slot for `rgba16`, a 3840×2160 frame writes 12,441,600 of a slot's 66,355,200 bytes (18.75%). On Windows the section is committed up front (`CreateFileMappingW` with `PAGE_READWRITE`), so at 4K about 199 MB is charged while about 37 MB is touched; `MAX_RING_BYTES` caps the charge. #775's open question about the decode slot's charge stays open and is not P2b's.
  - With it, **`slotBytes` is rounded up to a multiple of 8**, where the spec makes it the largest format's size. Each slot's generation is an atomic `u64` that must stay 8-aligned, so `SharedMemory::ring` refuses a ring of more than one slot whose `slotBytes` is not a multiple of 8.
- **The video encoder's `transports` gate the ring** (deviation 27). The process may start with a segment while Celesta describes its plugin, before the encoder is chosen, so the choice is made at the open: Celesta declares the ring in `openVideoEncoder` only when the chosen video encoder's described `transports` lists `sharedMemory`, and places frames in its slots only when, in addition, a frame of the input the encoder opened with fits a slot (a declared ring whose slots are too small for that input is left unused, and the frames go as payloads with the pipe's window). Otherwise it opens without `ring` and sends the frames as payloads, still pipelined (`Pipeline::decline_ring`). The spec takes the plugin's acceptance of the segment at `hello` as the whole negotiation.
- **Pipelining does not need the ring** (deviation 23). Without a segment (the plugin declined it, `CELESTA_CODEC_SHARED_MEMORY=0`, or deviation 21), or when frames do not go through the ring (none was declared, deviation 27, or the opened input's frame does not fit a slot), requests still go `PIPE_WINDOW = 3` deep, with the frames as payloads; when frames go through the ring, the window is its `slots`. The spec ties the window to `slots` and says only that the variable turns the ring off.
  - **The payload window is bounded by bytes** (deviation 28). When frames do not go through the ring (none was declared, or the opened input's frame does not fit a slot), the window is `clamp(MAX_RING_BYTES / frameBytes, 1, PIPE_WINDOW)` (`pipe_depth`), where `frameBytes` is a frame of the opened encoder's input format at the opened size: one for 8192×4320 `rgba16`, three for small frames. The frames deviation 21 sends through the pipe would otherwise wait three deep, two whole frames in the writer's queue, past the memory the ceiling bounds.

## Pixel formats and color

- **Formats** (the new `celesta_media::PixelFormat`):

  | Format | Layout |
  | --- | --- |
  | `rgba8` | 8-bit RGBA, opaque unless the encoder keeps alpha, then straight alpha (see Alpha; deviation 52, P4) |
  | `rgba16` | 16-bit LE RGBA, straight alpha (today's `Rgba16Straight`) |
  | `yuv420p` | planar I420 |
  | `yuv420p10` | planar, 10 bits in 16-bit LE |
  | `nv12` | Y plane, then interleaved UV |
  | `p010` | 16-bit LE Y, then interleaved UV, 10 bits in the high bits |

  The YUV formats are opaque.
- **Offered list.** Celesta offers, best first:
  1. the YUV formats, when the GPU can convert (`supports_yuv420p_readback`)
  2. `rgba16`
  3. `rgba8`

  The encoder answers the one it will take for these settings. This replaces `--color-conversion`: the conversion happens on the GPU whenever the encoder takes YUV and the GPU can produce it. Otherwise the encoder converts.
- **`nv12` and `p010`** come from the existing `yuv420p.wgsl` with the chroma written interleaved (one two-channel chroma target). `ReadbackFormat` gains `Nv12` and `P010`.
- **Alpha.**
  - `rgba8` is always opaque. An encoder answering `alpha: true` must have chosen `rgba16` (straight alpha); any other answer is `invalidSettings`.
  - Its frames are cleared to transparent. Otherwise frames are cleared to the default opaque background, as MP4 frames are today.
- **Order at export start.** The renderer's GPU capabilities (`supports_yuv420p_readback`, `is_software`) are known only from a renderer instance, and the encoder's answer is known only after `open`. So the order is:
  1. Construct the export renderer.
  2. Read its capabilities into the offered list.
  3. Open the encoder.
  4. Set the renderer's readback format and background from the answer.

  `GpuRenderer` gains `set_background` beside its existing `set_readback_format`.

  Alpha no longer follows from the container (`VideoFormat::has_alpha`) but from the encoder.
- **Color.** Every `openVideoEncoder` and video `Track` carries `{ "primaries": "bt709", "transfer": "bt709", "matrix": "bt709", "range": "limited" }`.
  - `matrix` and `range` describe the YUV conversion and how the encoded stream is tagged. They do not describe the input frames. `Color` binds video codecs only: an image encoder follows its format's own convention (JPEG's JFIF is full-range BT.601; PNG and WebP keep RGB).
  - RGB inputs (`rgba8`, `rgba16`) are always full-range sRGB-encoded values. An encoder converting them applies `matrix` and `range` itself.
  - YUV inputs (`yuv420p`, `yuv420p10`, `nv12`, `p010`) are always BT.709 limited range, as the GPU converts them. An encoder taking them as they are only tags them, so a host offering YUV sends the `Color` it rendered.
  - The GPU conversion changes from BT.601 to BT.709 (`yuv420p.wgsl`).
  - Encoders converting RGBA themselves must use the given matrix and range (FFmpeg's `BT709_FILTER` already does).
  - Every encoder and muxer tags its stream (H.264 VUI, `colr` atoms, WebM `Colour`).
  - **Output change:** MP4 colors change from untagged BT.601 to tagged BT.709. That fixes a color shift in players that take untagged HD as BT.709, and the file is no longer byte-identical to `main`'s.
- **HDR is reserved.** `transfer` may later be `pq` or `hlg`, and `Color` may carry `masteringDisplay` and `contentLightLevel`. Celesta renders SDR only and never sends them yet.

Deviations in P2a (`docs/handoff/2026-11-08-media-encoders.md`):

- **P2a's encoders did not apply `Color`** (deviation 8), until P2c. `openVideoEncoder` and every video `Track` carried the BT.709 limited `Color`, but FFmpeg's libx264 converted RGBA input with libswscale's default (BT.601, untagged), its ProRes and VP9 kept `main`'s BT.709 conversion and tags, and the GPU's `yuv420p` stayed BT.601. The muxers tagged MOV and WebM streams BT.709 from `Track.color` for ProRes and VP9 only. **Deviation 8 ends in P2c**, which applies `Color` everywhere at once: the GPU converts with BT.709, every FFmpeg encoder applies the given `Color`, and every muxer tags from `Track.color`.
- **A software renderer is offered no YUV format** (deviation 18), kept. The offered list above has the YUV formats whenever `supports_yuv420p_readback()`; Celesta keeps today's `ColorConversion::Auto` rule and leaves them out when `is_software()`, since lavapipe and WARP convert slower than libswscale. That keeps the encoder-side conversion on CI's software renderers. As of P2c the offered YUV formats are `yuv420p`, `yuv420p10`, `nv12`, and `p010`.

As built (P2c, `docs/handoff/2026-11-08-media-bt709.md`): the GPU converts with BT.709 limited range for all four YUV formats, `ReadbackFormat` has `Nv12` and `P010` (one two-channel chroma target), the exporter offers `yuv420p`, `yuv420p10`, `nv12`, `p010`, `rgba16`, `rgba8`, and `PixelFormat::OFFERABLE` lists all six. FFmpeg's encoders and muxers apply `Color` through libavutil's names; a name it does not know is a `MediaError::Codec`. Two deviations:

- **The test codec stores no color** (deviation 30). `celesta-codec-test`'s file has no reader but Celesta's tests, so its components accept `Color` and `Track.color` and store nothing. The exporter's `encode.rs` tests pin the `Color` the muxer receives instead.
- **The GPU reference is Celesta's formula, not libswscale's 10-bit output** (deviation 31). The "match a CPU reference conversion" and "BT.709 values hold for known colors" checks compare the readbacks with a CPU function of the BT.709 formula, within the dither's one code (exact on whole codes). Its 8-bit codes equal libswscale's for every color checked. libswscale's own 16-bit-RGB to 10-bit path, which an encoder converting `rgba16` itself takes, differs from the formula by up to 2 codes (blue's Cb 962 against 960, red's Y 251 against 250), so the encoder tests pin libswscale's codes (measured on arm64; each sample within 1 of them) and the GPU tests do not chase them.

Deviations in P4 (`docs/handoff/2026-11-09-media-stills.md`):

- **`rgba8` may carry straight alpha** (deviation 52; the question the table left to P4). An encoder answering `alpha: true` may have chosen `rgba8` or `rgba16`, and its frames are then cleared to transparent, as for `rgba16`; any other format with `alpha: true` stays `invalidSettings`. `builtin/png` and `builtin/webp` take `transparent` (default false); JPEG has no alpha. A contact sheet shows transparent pixels over its existing checkerboard and is itself opaque. Every other encoder's `rgba8` stays opaque, because it answers `alpha: false`.
- **Straight `rgba8` is narrowed on the CPU** (ruling 6 of the handoff). The renderer's 8-bit readback is premultiplied only, so an encoder keeping alpha in `rgba8` gets frames read back as `Rgba16Straight` and narrowed to 8 bits per channel with the same 8x8 ordered dither, offset, and rounding the GPU's 8-bit readback applies (by pixel position, every channel), so a transparent gradient does not band, in the still path and the video path (`ExportSession`) alike. Opaque frames and the GPU code are unchanged.

## Audio

- **Format.** Interleaved `f32` LE, 1 or 2 channels. Layout names (`mono`, `stereo`, `5.1`) are reserved for more.
- **Negotiation.**
  - `openAudioEncoder` sends the mix's rate (48 kHz by default) and channel count.
  - The answer gives the rate and channels the encoder takes: Opus, for example, always 48 kHz.
  - Celesta resamples with its shared resampler (`docs/handoff/2026-11-07-shared-resampling.md`) when they differ.
- **Flow.**
  - The mixer works on whole buffers (`mix_audio_graph*` returns one `AudioBuffer`), as do loudness normalization and the loop pre-roll (which needs the mix's end). So audio packets exist only once the mix is done. When that is depends on the entry:
    - **Projects.** The audio graph is known before rendering, so the mix runs beside the video render (`exporter.rs:153`, as today), and the muxer is opened as soon as both tracks' configs are known.
      - Video packets made before the mix is done wait in the interleaver, up to 256 MB in memory, then in a spill file.
      - When the mix finishes before the render, which is the usual case because audio is cheaper than frames, nothing is written twice.
      - When the render finishes first, the packets made since the spill began are staged once: no more I/O than today's video-only intermediate.
    - **React entries.** The audio graph is assembled from every frame's evaluation, after the render (`react_entry.rs:185-213`), and whether there is an audio track at all is decided there.
      - The muxer cannot be opened before the render ends, so every video packet is staged (memory, then the spill file), and the muxer is opened afterwards with or without an audio track.
      - That is the same I/O as today's intermediate and remux, so it is not slower, but it does not gain either.
  - Once the mix is done, Celesta streams it to the encoder in chunks of 1024 × 16 sample frames through the send window, and keeps the audio packets in memory (about 14 MB for 10 minutes at 192 kbps). The interleaver then merges them with the video packets.
  - `mux_audio` and the PCM sidecar are gone. The output is written once, except for the staging described above.
  - P2b measures how long the mix takes next to the render for the examples' projects, and records how often the spill happens.
    - As built in P2b, `CELESTA_EXPORT_TIMING=1` has an export print one line on stderr with the render's and the mix's times, the frame at which the render took the mix, the interleaver's peak staged bytes, and whether it spilled. The measurement is recorded in `docs/handoff/2026-11-08-media-pipelining.md`.
- **Delay.** The encoder's `delay` arrives with `audioEncoderOpened`, and its packets' `pts` already include it, starting negative. Every muxer that takes audio receives `delay` in the track and hides it (an MP4/MOV edit list, Opus pre-skip / WebM `CodecDelay`), whether or not it declares `trimsLeadingAudio`.
- **The loop pre-roll** replaces `lead_in_frames`.
  - With `looping`, and a muxer declaring `trimsLeadingAudio`, Celesta first feeds `LOOP_LEAD_IN_FRAMES` (16 × 1024) sample frames of the mix's end at negative `pts`, and sets `openMuxer`'s `leadingAudio` to that count.
  - The muxer hides them as it hides `delay`.
  - FFmpeg's MP4 and MOV declare it. Its WebM does not, so WebM behaves as today.
  - P5's AVAssetWriter muxers declare it and set `kCMSampleBufferAttachmentKey_TrimDurationAtStart` on the leading sample buffers. As built, that alone leaves FFmpeg reading the sound 2112 frames late, so the first audio buffer also carries two gradual-decoder-refresh attachments (deviation 60, [OS-native plugins](#os-native-plugins-p5-p6)), and PCM before 0 is cut rather than trimmed (deviation 61).
  - Whether Media Foundation's MP4 sink writes edit lists at all, for the encoder's own delay included, is verified in P6. Until then its muxers do not declare it.
- **No audio.** There is no audio track with `audio: None`, with a muxer without an `audio` list, or, as today, for a React entry whose audio graph has no clips. A project always gets the track of its mix, silent or not, as today.

Deviations in P2a (`docs/handoff/2026-11-08-media-encoders.md`):

- **Audio sample-rate conversion uses `rubato`** (deviation 9). The shared resampler named above (`docs/handoff/2026-11-07-shared-resampling.md`) resamples images, not audio. `celesta_media::resample_audio` wraps `rubato`'s sinc resampler for the rare mix whose rate the encoder refuses (Opus takes 48 kHz only). A channel count the encoder refuses is converted too: mono to stereo by copying the channel, stereo to mono by averaging.
- **The mix is encoded in one pass once it is done** (deviation 11). Its chunks of `1024 × 16` sample frames go out back to back after the frame during which the mix arrived (a project) or after the render (a React entry), rather than interleaved with later frames, and the muxer opens right after. Without an audio encoder (`--audio-encoder none`, or a muxer that takes no audio) no mix is made at all, and a loudness target warns that it has nothing to apply to.
- **`lpcm` codec strings name the sample format** (deviation 13): `lpcm.s16le`, `lpcm.s24le`, `lpcm.f32le`, so a muxer from another plugin knows the layout. The family stays `lpcm`.
- **The WebM muxer derives Opus's trailing padding from the last packet** (deviation 26). `Packet` has no skip-samples field, so FFmpeg's WebM muxer compares the last Opus packet's `duration`, which its encoder shortens to the samples left, with the sample count its TOC gives, and writes the difference as that block's discard padding, as `main`'s file has it. Another plugin's Opus encoder gets the same trailing trim only by shortening its last packet's `duration` likewise.

## Stills and sequences (P4)

- **Built-in components** in a new root-workspace crate, `crates/media-image` (`celesta-media-image`, MIT OR Apache-2.0, not a plugin). It moves `stills.rs`'s encoders behind the component traits, and `celesta-exporter` registers it in its `MediaRegistry`:
  - Video encoders `builtin/png` (the `png` crate; settings: compression), `builtin/jpeg` (the `image` crate; quality), and `builtin/webp` (the `webp` crate; quality, lossless). Each takes `rgba8`, or `rgba16` for 16-bit PNG.
  - Muxers `builtin/image` (one frame, one file; `extensions: ["png","jpg","jpeg","webp"]`, no audio) and `builtin/imageSequence` (`sequence: true`; writes Celesta's `name-000090.ext` naming as today, [Muxing](#muxing)).
- **No process or ring.** Frames go to them as borrowed slices in process, and `describe` is answered in process. A still costs no process start, and `cargo install celesta-exporter` keeps `--frame` working with nothing else installed, as today.
- **`celesta-exporter`.** `--frame` and the other still options and the contact sheet's compositing stay. Their output goes through `resolve_output` with these components, and the WebP size limit becomes the encoder's `invalidSettings`.
- **Plugins can add image formats** (GIF, APNG, AVIF, …) as ordinary components. A plugin muxer for `png` ranks after `builtin/image` by default and is used when named.

Deviations in P4 (`docs/handoff/2026-11-09-media-stills.md`):

- **Frames reach built-in encoders as an owned `Vec<u8>`** (deviation 54), as `VideoEncoder::encode_video` takes them, not as borrowed slices: one copy of each still, negligible next to encoding it.
- **Library-level quality and size errors become `invalid_option`** (deviation 56). `StillExport.quality` out of range, or given for PNG, is validated against the encoder's schema, and WebP's 16,383-pixel limit is the encoder's `invalidSettings` at open (key `size`, with P3's message), both before rendering; they were `invalid_selection`. The CLI's and MCP's own checks of `--quality` and `quality` keep their usage messages.
- **`builtin/png` also takes `depth`** (deviation 57): `"8"` or `"16"`, default `"8"`, since "`rgba16` for 16-bit PNG" needs a setting to choose it. A contact sheet is offered `rgba8` only, so `depth=16` there fails with `invalid_option` before rendering.
- **As built**, the settings are: `builtin/png` `compression` (`none`, `fastest`, `fast`, `balanced`, `high`; default `balanced`, the `png` crate's default, so default files are P3's bytes), `depth`, and `transparent` (default false); `builtin/jpeg` `quality` (1–100, default 90); `builtin/webp` `lossless` (default false), `quality` (shown only when not lossless), and `transparent`. `inputFormats` are `[rgba16, rgba8]` for PNG and `[rgba8]` for JPEG and WebP; a separate-files still offers `[rgba16, rgba8]` and a contact sheet `[rgba8]`. JPEG drops alpha rather than flattening it: it answers `alpha: false`, so its frames are rendered opaque. `describe` answers every component available, in process, with no `transports`.

## OS-native plugins (P5, P6)

- **P5, macOS** (`plugins/avfoundation`):
  - Video encoders on VideoToolbox: `h264` (`avc1`), `hevc` (`hvc1`), and `prores` (`ap4h`, `apch`). They take `nv12`, `p010`, or `rgba16` for ProRes 4444, and report `available: false` when VideoToolbox lacks the encoder. VideoToolbox's documented 16-bit inputs for ProRes are `kCVPixelFormatType_64ARGB` (big-endian) and `4444AYpCbCr16`. Whether it takes little-endian RGBA is verified in P5; if not, the plugin swizzles.
  - Audio encoder `aac` on AudioToolbox.
  - Muxers `mp4` and `mov` on AVAssetWriter, in passthrough (`outputSettings: nil`, with a source format hint built from `TrackConfig`).
  - A decoder on AVFoundation (`AVAssetReader`) for `mp4`, `mov`, and `m4a`.

  It is written in Rust with `objc2` bindings, and builds on macOS only (manifest `executable` for `macos-*`).
- **P6, Windows** (`plugins/mediafoundation`): the same component set on Media Foundation (encoder MFTs, SinkWriter with compressed input, SourceReader). It cannot be run on the macOS development machine, so it is a separate session, verified by CI and on a Windows machine.

These prove the design: an independent plugin, combined with FFmpeg's
(for example AVFoundation's HEVC with FFmpeg's Opus in FFmpeg's MP4).

As built (P5, `docs/handoff/2026-11-09-media-avfoundation.md`): `plugins/avfoundation` (`celesta-codec-avfoundation`, MIT OR Apache-2.0, `"os": ["macos"]` in `plugin.json`, which `build.mts` skips on other systems) has the decoder `avfoundation`; the video encoders `h264` (`nv12`, `rgba16`), `hevc` (`nv12`, `p010`, `rgba16`; `bitDepth` 8 or 10), and `prores` (`rgba16`; `4444` or `hq`), each describing `transports: ["sharedMemory", "pipe"]`; the audio encoder `aac` (AAC-LC, 8–48 kHz, 1 or 2 channels, 2112 frames of priming, at a constant bitrate, since AudioToolbox's default rate control coded the first second worse than the rest, the first hit of a test signal with about 12 times the error of the worst later one, which a looping export's start exposes); and the muxers `mp4` (`avc1`, `hvc1`; `mp4a`) and `mov` (also `ap4h`, `apch`; `mp4a`, `lpcm`), both with `trimsLeadingAudio`. Encoders whose codec `VTCopyVideoEncoderList` or `kAudioFormatProperty_Encoders` lacks describe `available: false` with the reason. VideoToolbox's decode timestamps are shifted back by the stream's reorder depth (HEVC's `sps_max_num_reorder_pics`, 2 with the defaults; VideoToolbox's H.264 SPS has no `bitstream_restriction`, so H.264 is shifted by the fallback, 16 frames; 0 for ProRes, `bFrames: false`, and H.264 Baseline, which never reorders), so every packet has `dts ≤ pts` and another plugin's muxer takes it. The muxers remove the empty file Celesta stages at `output` before `AVAssetWriter` opens it (and refuse anything else there), queue each input's samples while it is not ready (`writePackets` waits, up to 60 s without progress, only once more than 64 MiB is queued and no input is ready), and set the movie timescale from the video track. The decoder gives the frame shown at the requested time at the displayed size (quarter-turn `preferredTransform`s rotate) and the first audio track through `AVAssetReaderAudioMixOutput`. The macOS package bundles the plugin in `Contents/Resources/PlugIns` after FFmpeg. Verified on this Mac (Apple M4, macOS 26.6) before planning: **VideoToolbox's ProRes 4444 takes `kCVPixelFormatType_64RGBALE` (`l64r`) directly, with straight alpha, and no swizzle is needed**: a frame of it encodes byte-identical to the same frame as `64ARGB`, and decodes to BT.709 limited 12-bit codes with alpha kept; every VideoToolbox encoder takes `420v`, `x420`, `l64r`, `b64a`, `y416`, and `BGRA`, and has a software implementation beside the hardware one; and **the AAC trim recipe** of deviation 60 places a click at the same sample for FFmpeg and AVFoundation (an edit list of `delay + leadingAudio` and a `roll` group of −1). FFmpeg's MP4 and MOV muxers now write the sample entry the codec string spells (`hvc1` or `hev1`, `avc1` or `avc3`), so VideoToolbox's HEVC in FFmpeg's MP4 is `hvc1`, which QuickTime plays. Five deviations:

- **The trim needs two more attachments** (deviation 60). With only `kCMSampleBufferAttachmentKey_TrimDurationAtStart` on the leading buffers, a passthrough AAC track gets an edit list of `trim − 2112` and no `roll` sample group: AVFoundation, which assumes 2112 frames of implicit priming for such a file, plays it correctly, and FFmpeg (and any reader honouring the edit list alone) plays it 2112 frames late. The muxer therefore also sets `kCMSampleBufferAttachmentKey_GradualDecoderRefresh = 1` and `"IsGradualDecoderRefreshAuthoritative" = kCFBooleanTrue` (a key AVAssetReader's own passthrough buffers carry; no public constant) on the first audio sample buffer, every attachment with `kCMAttachmentMode_ShouldPropagate`. AVAssetWriter then writes an edit list hiding exactly `delay + leadingAudio` and a `roll` group of −1, as its own AAC encoding does, and both readers agree to the sample, for AudioToolbox's 2112 frames of priming and for FFmpeg's `aac` with 1024 alike. An AAC track whose `delay` is 0 gets no start trim, and AVAssetWriter takes it (checked).
- **Leading LPCM is cut, not trimmed** (deviation 61). PCM has no priming and every sample stands alone, so the `mov` muxer drops the sample frames before 0 (`leadingAudio`) from the packets' bytes, and no edit-list behaviour is relied on.
- **`h264` and `hevc` also take `rgba16`** (deviation 62), which VideoToolbox converts with the session's BT.709 limited matrix, for a software renderer's offer, which has no YUV (deviation 18). They refuse an odd width or height at `open` (`MediaError::Codec("H.264 needs even dimensions, not WxH")`, likewise HEVC), whatever the input, since VideoToolbox encodes 4:2:0 and crops such a frame to even dimensions without an error, so the stream would not match its `Track`; the exporter already refuses odd sizes for video. ProRes keeps odd sizes. `h264` takes no `p010`: VideoToolbox's H.264 is 8-bit only.
- **`avfoundation/mov`'s default audio is `mp4a`** (deviation 63). FFmpeg's `mov` defaults to `lpcm`; this plugin has no PCM encoder, so its `mov` takes `lpcm` from another plugin when named and defaults to its own AAC, and the plugin works alone.
- **The decoder converts colour as AVFoundation does** (deviation 64). FFmpeg's decoder tone-maps HDR to sRGB with Celesta's curve; this one asks `AVAssetReader` for 8-bit BGRA with `AVVideoColorPropertiesKey` BT.709 and takes AVFoundation's conversion. FFmpeg decodes first (FFmpeg first among bundled plugins, P4's deviation 58), so this applies only to files FFmpeg cannot read or when FFmpeg is turned off.

Planned for P6 (`docs/superpowers/plans/2026-11-08-media-components-p6.md`): `plugins/mediafoundation` (`celesta-codec-mediafoundation`, MIT OR Apache-2.0, `"os": ["windows"]`) has the decoder `mediafoundation` (`mp4`, `mov`, `m4a`) on `IMFSourceReader`; the video encoders `h264` and `hevc` on encoder MFTs, hardware first; the audio encoder `aac` on Microsoft's AAC encoder MFT; and the muxer `mp4` on `IMFSinkWriter` with compressed input, with `trimsLeadingAudio`. The plan's Task 1 verifies on Windows what Media Foundation does where its documentation is silent (below), and the last task rewrites this paragraph as built. Seven deviations, numbered from 70:

- **No ProRes and no QuickTime muxer** (deviation 70). Media Foundation has no ProRes encoder and no QuickTime sink, so "the same component set" is P5's less `prores` and `mov`. A Windows user gets ProRes and MOV from FFmpeg.
- **The `mp4` muxer ships only if the MPEG-4 sink can hide the delay** (deviation 71). Every muxer that takes audio hides the encoder's delay ([Audio](#audio)), and Media Foundation's MPEG-4 sink has no documented way to write an edit list that starts its media later. Task 1 tries what it has: negative sample times, and samples marked as a discontinuity or a preroll. It also checks that the sink writes reordered video (`dts < pts`, a `ctts` box) and colour tags. If the sink fails any of these checks, P6 ships no muxer. Its encoders then go into another plugin's muxer: `ffmpeg/mp4` by the bundled ranking (deviation 58). That is recorded as built, and a muxer of the plugin's own is deferred. **Task 1's outcome (Windows 11 build 26300): the MPEG-4 sink cannot hide the delay.** It writes no `edts`/`elst` from negative sample times, from leading samples marked `MFSampleExtension_Discontinuity`, or from `MF_SD_AUDIO_ENCODER_DELAY` on the stream's type. It drops every sample whose time is negative, while `WriteSample` and `Finalize` answer `S_OK`; that includes the IDR of a reordered H.264 stream whose decode time is −1 frame, after which no frame decodes. With times kept non-negative it writes `ctts` and `colr nclx` BT.709 limited, but the reorder delay then shows as a late start. So P6 ships no muxer, and the plugin's encoders go into `ffmpeg/mp4`.
- **RGB frames are converted in the plugin** (deviation 72). Media Foundation's H.264 and HEVC encoders take no RGB. A software renderer offers no YUV (deviation 18). So `h264` takes `rgba8`, and `hevc` takes `rgba8` or, at 10 bits, `rgba16`. The plugin converts them on the CPU to `nv12` or `p010` with BT.709 limited-range coefficients and 2×2 chroma averaging. It does not use the Video Processor MFT, whose matrix choice the documentation does not pin. This is P5's deviation 62 solved in the plugin.
- **`aac` takes 16-bit input at 44.1 or 48 kHz, at four bitrates** (deviation 73). Microsoft's AAC encoder takes 16-bit PCM at 44100 or 48000 Hz and encodes at 96, 128, 160, or 192 kbit/s. The plugin rounds the mix's `f32` to `s16` with clipping. `bitrate` is therefore a `select` of those four. The encoder reports no priming, so the delay `open` answers is a constant that Task 1 measures (a click, decoded by FFmpeg).
- **Hardware encoders first, with a `hardware` setting** (deviation 74). `MFTEnumEx` lists hardware MFTs (Intel, NVIDIA, AMD) before Microsoft's software ones. Hardware MFTs are asynchronous, and their output differs between drivers. `hardware: false` pins Microsoft's software MFT, which tests and CI use where they compare values (P5's lesson, `d2ebdd1c`). HEVC is available only where some HEVC encoder MFT is listed: a GPU driver's, or the Store's HEVC Video Extensions.
- **The decoder converts as Media Foundation does and drops alpha** (deviation 75). `IMFSourceReader` with advanced video processing gives `RGB32` (BGRX), which the plugin makes opaque RGBA. The plugin rotates by `MF_MT_VIDEO_ROTATION` itself. Audio is read as `f32` at the file's own rate, then resampled with `celesta_media::resample_audio` and mixed to the requested channels in the plugin. FFmpeg decodes first, as in deviation 64.
- **No file is held open between requests** (deviation 76). Windows cannot delete or replace a file another process has open, as P2a found when Windows CI failed to delete an open file. So the decoder opens a reader per probe or single-frame request and closes it before answering. It keeps a reader only for a sequential session, with P5's idle limits. A muxer's `abort` releases the sink writer before it answers the files to remove.

When Media Foundation cannot start (`MFStartup` fails: Windows Server without the Media Foundation feature, or an N edition without the Media Feature Pack), every component describes unavailable with that reason.

## Testing and verification

- **`celesta-plugin`**: the ring's slots read and write under the seqlock, as #775's slot; segments are sized for several slots.
- **`celesta-plugin-host`**:
  - `media` manifests parse, and an old `codec` role is ignored.
  - Components are listed with their origin, and bad components are skipped.
  - The send window keeps answers in order, enforces deadlines per request, and kills on an out-of-order answer.
- **`celesta-media`**:
  - Protocol 2 round-trips every message through a fake provider.
  - Settings validation: every rule, `visibleWhen`, and defaults.
  - Decoder routing: extension candidates, fallback on `unsupported`, pinning, ordering by origin, and the most specific error.
  - The `describe` cache and its invalidation.
  - The ring and pipelining against the test codec's program.
- **`celesta-codec-test`**: ported to the four kinds (a lossless video encoder, a PCM encoder, its own muxer, its decoder), as program and in process. The core's export tests run through it, as today.
- **`celesta-exporter`**:
  - `resolve_output`: defaults by extension, ranking, presets and overrides, stale presets, and every error code.
  - The interleaver: ordering, the spill, and audio-less runs.
  - Pre-roll only with `trimsLeadingAudio`.
  - The JSON report's shape.
  - CLI parsing of the new flags.
- **`celesta-codec-ffmpeg`**:
  - Each component through the program.
  - MP4, MOV, and WebM export and read back, and their colors are tagged BT.709.
  - Looping MP4's first hit is as clean as before (the existing lead-in test).
  - An LGPL-like build reports `libx264` unavailable with a reason.
- **GPU**: `nv12` and `p010` readbacks match a CPU reference conversion. BT.709 values hold for known colors.
- **Performance (the hard requirement).** These are measured against `main` (release builds, Apple M4, medians of several interleaved runs) and recorded in each phase's handoff note:
  - `codec-bench` encode at 1080p and 4K
  - exports of 300 frames at 1080p and 4K, with `ultrafast` and `medium`
  - a ProRes 4444 export at 4K, whose packets (several MB a frame) cross the pipe twice

  The new path must not be slower.
- **Rendering changes** (BT.709 in P2c): screenshots of the same scene before and after, attached to the PR (AGENTS.md).
- Each phase updates the user guide (`packages/website/content/en/` and `ja/`) and the Agent Skill (`skills/celesta/`) where they cover what it changes, and records its details in a `docs/handoff/<date>-<task>.md` note.

## Phases

Each phase is its own PR, stacked on the last. Each leaves `main`
working.

| Phase | Contents |
| --- | --- |
| P1: Protocol 2 and decoders | The manifest's `media` and `roles: ["media"]`, the component listing (with the sibling's `--manifest`), `MediaRegistry`, decoder routing and fallback, the decoder half of protocol 2, the project-decoder notice, and "No media decoder can run". FFmpeg's and the test codec's decoders are ported. Also everything that writes or checks today's `codec` manifest and protocol 1, or a packaged build after P1 would ship a plugin Celesta ignores: `plugins/ffmpeg/plugin.json`; `scripts/plugins/manifest.mts` (the `media` key, `protocol` per role) and its tests; `scripts/test-package.mjs`; the test codec's manifest (`crates/codec-test/src/plugin.rs`); and the protocol check in `celesta-plugin` (`PROTOCOL_VERSION`: the serve loop's `hello`, `PluginChild::hello_with`, and discovery), which becomes per role. Until P2a replaces them, protocol 2 temporarily keeps protocol 1's `checkEncoder`, `openEncoder`, `encodeFrame`, `finishEncoder`, `abortEncoder`, and `muxAudio` messages unchanged, sent to one media plugin (as built, the first bundled one, else the first in search order that is not a project's, else none), so exports keep working. As built (`docs/handoff/2026-11-08-media-decoders.md`): `describe` and its cache moved to P2a (deviation 1), the manifest parses `media.decoders` only (deviation 3), the process-wide registry stays (deviation 2), the legacy encoder may share a decoder's process (deviation 6), and a project's plugin cannot replace a bundled media plugin (deviation 7); see [Capability discovery](#capability-discovery), [Registry and routing](#registry-and-routing), and [Decoders](#decoders). |
| P2a: Encoders and muxers | `describe` and its cache, moved from P1, and the manifest's encoder and muxer keys; the settings schema and validation; the encoder and muxer halves of protocol 2 through the pipe only (`transports: ["pipe"]`, a window of one); negotiation; the interleaver and its spill; audio streaming, delay, and pre-roll; `resolve_output`; FFmpeg's and the test codec's encoders and muxers; the exporter's options, CLI, JSON report, and progress labels; MCP and `@celesta.studio/exporter`; and removal of the old types and protocol 1's encoder messages. The FFmpeg plugin moves from ez-ffmpeg's file-writing pipeline (`VideoWriter`, `FfmpegContext`) to packet-level libavcodec / libavformat through `ffmpeg-sys-next` (already a dependency), since ez-ffmpeg's packet sink takes only avcC H.264 and raw AAC. Checked against `main` by decoding: video frames and audio samples identical. Files are not byte-identical (packet-level muxing, edit lists from negative `pts`). As built (`docs/handoff/2026-11-08-media-encoders.md`): the deadlines and the manifest's `inputFormats` land here rather than in P2b, and deviations 8–18, 24, and 26 are recorded in [Capability discovery](#capability-discovery), [Exporter API](#exporter-api), [Registry and routing](#registry-and-routing), [CLI](#cli-celesta-exporter), [Pixel formats and color](#pixel-formats-and-color), and [Audio](#audio). Deviation 25: **decoded frames on the RGBA-input paths may differ from `main`'s by at most 1 code value** (16-bit codes for MOV and WebM): MOV, WebM, and MP4 on a software renderer. P2a converts with libswscale's `sws_scale` where `main`'s filter graph used the `scale` filter with the same flags, and the two entry points can round differently in ways P2a cannot match exactly; the GPU `yuv420p` path and every audio path stay identical. The end-to-end check found no difference on any path it checked (the MP4 path on a software renderer was not run). |
| P2b: Shared memory and pipelining | The ring and the send window (P2a already brought `inputFormats` in the manifest and the deadlines); the mix-against-render measurement; and the performance comparison against `main`. Checked byte-identical against P2a's output. As built (`docs/handoff/2026-11-08-media-pipelining.md`): the window is as deep as the ring's `slots`, or at most `PIPE_WINDOW = 3` without a ring, `writePackets` goes in batches (`WRITE_BATCH_PACKETS = 8` packets or `WRITE_BATCH_BYTES = 4 MiB`, and the rest at `finish`), and the FFmpeg and test-codec video encoders describe `transports: ["sharedMemory", "pipe"]`; deviations 19–23, 27, and 28 are recorded in [Transport](#transport) and [Deadlines and the window](#deadlines-and-the-window), and the measurement in [Audio](#audio). |
| P2c: BT.709 | The GPU conversion to BT.709, `nv12` and `p010`, and color tags in every container, with before/after screenshots of the same scene (AGENTS.md). Its own PR, so P2a and P2b are checked without a color change. As built (`docs/handoff/2026-11-08-media-bt709.md`): BT.709 limited range on the GPU for all four YUV readbacks, six offered formats (`PixelFormat::OFFERABLE` too), every FFmpeg encoder applying `Color` and every muxer tagging from `Track.color`, libx264 taking `nv12` directly and `p010` through a repack; deviation 8 ends, 18 is kept, and 30 and 31 are recorded in [Pixel formats and color](#pixel-formats-and-color). MP4 output changes from untagged BT.601 to tagged BT.709; MOV and WebM decode identically to P2b. |
| P3: Export dialog and presets | The editor's export dialog; user and project `export-presets.json`, the project's `default`, and the presets in the CLI, MCP, and TS (`--preset`, `--preset-file`). As built (`docs/handoff/2026-11-09-export-presets-dialog.md`): `presets.rs` holds `PresetsFile` (version 1, unknown top-level and preset keys kept on rewrite, atomic pretty-printed writes) and `PresetLookup`; `resolve_output` takes the lookup and returns warnings (deviation 40), `ExportOptions::default()` reads no file (deviation 41), a broken file the lookup reaches is `invalid_presets` (deviation 42), a missing `default` is `unknown_preset` with `default_in` (deviation 43), and a project's `default` counts as naming its components, with a warning (deviation 44); see [Presets](#presets), [Resolving an output](#resolving-an-output), and [Exporter API](#exporter-api). The plan's decisions: the project `default` applies with no preset and no muxer, video, or audio component (`--audio-encoder none` counts; options layer over it); a preset's options apply only to its own encoder; preset options are sanitized (each dropped key or value warned), explicit ones go over them, and the result is validated strictly; a named or preset muxer that does not list the output's extension warns; saved options are the visible settings that differ from their defaults (`differences`), for presets and `last` alike. In the editor, Export… opens the dialog (Preset, Container, Video encoder with Advanced collapsed, Audio encoder or None, Range, Loudness; `Save as preset…`, `Cancel`, `Export…`); loudness moves from the title bar into it; its first choice is the project's `default`, else `last`, else the `.mp4` defaults; "Save as preset…" is an inline section (This project or My presets, `Replace` for an existing name, "Save the project first" for an untitled document); `last` is saved when Export… is pressed, before the save panel; and the dialog's state is kept between openings until Reload Plugins. |
| P4: Stills | `celesta-media-image`'s built-in components; stills, sequences, and contact sheets through them. As built (`docs/handoff/2026-11-09-media-stills.md`): `MediaOrigin::BuiltIn` and `celesta_media::default_rank`, with FFmpeg first among bundled plugins by id (deviation 58); `crates/media-image` with the encoders `builtin/png`, `builtin/jpeg`, and `builtin/webp` and the muxers `builtin/image` and `builtin/imageSequence`, both refusing a codec that is not the output extension's; `celesta_exporter::with_builtin` registering them in every exporter registry, and `serves_video` keeping them, and every sequence muxer, out of video exports and the editor's dialog (deviation 50); `still_output.rs`, through which stills, sequences, and contact sheets resolve and write, naming their components by extension (deviation 51); straight-alpha `rgba8` for `transparent` PNG and WebP (deviation 52); sequences published frame by frame from a staging folder (deviation 53); owned frames (deviation 54); `--muxer`, `--video-encoder`, and `--video-option` for stills, and MCP's and the TS package's `muxer` and `videoEncoder` (deviation 55); quality and size errors as `invalid_option` (deviation 56); PNG's `depth` (deviation 57); and the built-in plugin listed without a folder (deviation 59); see [Components](#components), [Muxing](#muxing), [Resolving an output](#resolving-an-output), [CLI](#cli-celesta-exporter), [Pixel formats and color](#pixel-formats-and-color), and [Stills](#stills-and-sequences-p4). With default settings, every still export P3 wrote is byte-identical. |
| P5: macOS native | `plugins/avfoundation`. As built (`docs/handoff/2026-11-09-media-avfoundation.md`): the decoder, VideoToolbox's `h264`, `hevc`, and `prores`, AudioToolbox's constant-bitrate `aac`, and AVAssetWriter's `mp4` and `mov`, in Rust through `objc2`; decode timestamps shifted by the reorder depth; the `os` key in `plugin.json` and `build.mts`; FFmpeg's muxers keeping the `hvc1` and `avc1` sample entries; and the plugin in the macOS package's `Contents/Resources/PlugIns`. ProRes takes little-endian RGBA without a swizzle. Deviations 60–64 are recorded in [OS-native plugins](#os-native-plugins-p5-p6) and [Audio](#audio). FFmpeg stays the default through P4's FFmpeg-first ranking (deviation 58), so the branch lands after P4. |
| P6: Windows native | `plugins/mediafoundation`, in its own session on a Windows machine (`docs/superpowers/plans/2026-11-08-media-components-p6.md`). It provides the decoder, MFT `h264`, `hevc`, and `aac`, and the SinkWriter's `mp4` if Task 1 finds that its sink can hide the delay. Deviations 70–76 are planned in [OS-native plugins](#os-native-plugins-p5-p6). |

`--preset` is parsed from P2a. Until P3 adds `export-presets.json` it
fails with `unknown_preset` and the libx264 hint, so the old meaning never
silently survives.

## Deferred

| Item | Where it would go |
| --- | --- |
| GPU texture sharing | `transports`: `ioSurface`, `dxgiShared`, `vulkanExternal`. |
| HDR | `Color.transfer` `pq`/`hlg`, `masteringDisplay`, `contentLightLevel`. |
| More than 2 channels | Channel layout names in `openAudioEncoder`. |
| Out-of-order answers | Request IDs in protocol 2's headers, if a plugin ever needs to answer out of order. |
| A streaming mix | Audio packets beside the video when no loudness target and no loop are set, so a project's mix never makes the render wait; needs a chunked `mix_audio_graph`. |
| React audio known earlier | The bridge reporting, before rendering, whether the entry can have audio, so a React export can open its muxer at once. |
| Disk cache of `describe` | Keyed as the memory cache is, if plugin start-up turns out slow. |
| Sniffing | `magic` patterns in a decoder's manifest entry. |
| User decoder priority | A list in `settings.json`, ahead of the origin order. |
| Plugin presets, quality/speed hints, localized labels | `presets` in `describe`, `hint` on fields, `labels: { ja }`. |
| Same-process packet shortcut | An `openMuxer` that names an encoder session of the same process. |
| Web on the same model | WebCodecs-backed components in `@celesta.studio/web`. |
