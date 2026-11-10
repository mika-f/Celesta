# Media components P1: protocol 2 and decoders — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the `codec` role with the `media` role on protocol 2 and
route every probe and decode to decoder components. Encoding keeps working
through protocol 1's encoder messages, carried inside protocol 2 until P2a.

**Architecture:**
- **Manifest.** `celesta-plugin-host` parses a media plugin's manifest
  (`roles: ["media"]`, `protocol: 2`, `media.decoders`) and lists media
  plugins with their origin. `celesta-media` turns that list into a
  `MediaRegistry`.
- **Routing.** `MediaBackend` holds one process per plugin. For each path it
  probes the candidate decoders in origin order, moves on when one answers
  `unsupported`, and pins the path to the decoder that read it.
- **Encoding.** The legacy encoder calls go to the first media plugin in
  search order, as `InstalledPlugins::codec()` chose before.

**Tech stack:** Rust 2024 (MSRV 1.89), serde/serde_json, Vitest (TypeScript)
for `scripts/plugins`.

**Spec:** `docs/superpowers/specs/2026-11-08-media-components-design.md`
(sections Manifest, Protocol 2 → Handshake and Decoding, Decoders, and
Phases → P1). Read it with this plan.

## Global constraints

- No Celesta crate except `celesta-codec-ffmpeg` links FFmpeg. The core's
  tests use `celesta-codec-test` (HANDOFF.md).
- Media plugins speak protocol **2**; content plugins stay on protocol **1**.
  A manifest with both `content` and `media` is invalid.
- The plugin ID `builtin` is reserved, and a manifest with it is invalid.
- Decoder order by origin: **user** (`Explicit`, `User`, `Folder` sources) →
  **bundled** (`Bundled` source, and the `cargo install` sibling) →
  **project** (`Project` source). Within an origin, search order; within a
  plugin, manifest order.
- An error answer with `code: "unsupported"` means "not a file this decoder
  reads" and is valid only for `probe`.
- `CELESTA_CODEC_SHARED_MEMORY=0` still turns the decoded-frame slot off.
- JS/TS tests use Vitest in TypeScript, and Rust tests stay in Rust
  (AGENTS.md).
- Never amend commits; each task ends in a new commit. Commits are signed
  through 1Password. If signing fails with "failed to fill whole buffer",
  ask the user to unlock 1Password. Do not bypass signing.
- Building or testing `plugins/` needs FFmpeg 8:
  `export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"`
  (Homebrew's default `ffmpeg` is 9 and breaks the build).
- **Builds between tasks.** Tasks 3–7 rename `CodecProvider`, `CodecProcess`, and `set_codec`. Until Task 7 updates their callers, `celesta-exporter`, `celesta-mcp`, and `celesta-editor` do not compile, so each of those tasks builds and tests only the crates it names. From Task 8 on, the whole workspace compiles.
- Each commit message ends with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Deviations from the spec (recorded in Task 10's spec edit)

1. **`describe` and its cache move to P2a.** Nothing consumes them in P1:
   decoders route by `probe`, and the export dialog and `--list-components`
   arrive later.
2. **A process-wide registry stays.** `set_registry` and `registry` replace
   `set_codec` and `codec`, and `MediaBackend::new()` reads the registry
   lazily, as it does today. A backend made before discovery (the editor's
   long-lived audio worker) must get the registry chosen later.
   `MediaBackend::with_registry` is the explicit form.
3. **P1's manifest parses `media.decoders` only.** The encoder and muxer
   keys arrive with P2a, and serde already ignores keys it does not know.
4. **A path that does not exist fails with `NotFound` before any plugin is
   started.** Paths are absolute and local (spec, Components).
5. **New exporter code `unsupported_media`** for `MediaError::Unsupported`,
   beside `no_decoder`.

## Review focus

The tests that pin each of these are in the task named.

1. **An old `roles: ["codec"]` plugin** that the user installed earlier is
   ignored, not fatal. The editor's "No media decoder can run" problem names
   no codec plugin as the reason (Tasks 2 and 8).
2. **A missing file** starts no plugin process (Task 4,
   `missing_file_starts_no_process`).
3. **An export cancelled during routing** (the kill switch) stops at once.
   It does not go on to try the next decoder (Task 4,
   `a_stopped_decoder_ends_routing`).
4. **A decoder process that dies on `probe`** is restarted once and asked
   again, then routing moves to the next candidate (Task 4,
   `an_unavailable_decoder_falls_through`).
5. **A `cargo install` sibling whose `--manifest` hangs or prints garbage**
   is left out within 5 s. Discovery still returns (Task 2,
   `a_sibling_that_hangs_is_left_out`).

---

### Task 1: Protocol version per role (`celesta-plugin`, `celesta-plugin-host` child)

**Files:**
- Modify: `crates/plugin/src/protocol.rs` (constants near `:26`, serve loop `:1042-1080`)
- Modify: `crates/plugin/src/lib.rs:16` (re-exports)
- Modify: `crates/plugin-host/src/child.rs:185-207` (`hello`, `hello_with`)
- Test: `crates/plugin/src/protocol.rs` (its `tests` module), `crates/plugin-host/src/child.rs` tests

**Interfaces:**
- Produces:
  - `pub const MEDIA_PROTOCOL_VERSION: u32 = 2;`
  - `pub fn protocol_for_roles(roles: &[&str]) -> u32`, which returns 2 when `roles` contains `"media"`, else `PROTOCOL_VERSION` (1).
  - `PluginChild::hello_as(&mut self, protocol: u32, offer: Map<String, Value>) -> Result<Message, RequestError>`.
  - `hello_with(offer)` stays and becomes `hello_as(PROTOCOL_VERSION, offer)`.

- [ ] **Step 1: Write failing tests** in `protocol.rs`'s tests:
  - `media_roles_answer_protocol_2`: a `serve_requests_with_hello_on("p", "1.0.0", &["media"], …)` loop answers `{"type":"hello","protocol":2}` with `protocol == 2` and `roles == ["media"]`.
  - `media_roles_refuse_protocol_1`: the same loop answers `protocol: 1` with an `error` whose message contains `"speaks protocol 2"`.
  - `content_roles_still_answer_protocol_1`: the existing behaviour, unchanged.

  In `child.rs`'s tests: `hello_as_checks_the_answered_protocol`. A fake plugin that answers `protocol: 1` to `hello_as(2, …)` gives `RequestError::Refused`.
- [ ] **Step 2:** `cargo test -p celesta-plugin -p celesta-plugin-host hello` fails (`protocol_for_roles` and `hello_as` are undefined).
- [ ] **Step 3: Implement.** The serve loop compares and answers with `protocol_for_roles(roles)` instead of `PROTOCOL_VERSION`. `hello_as` sends `protocol` and checks the answer against it.
- [ ] **Step 4:** `cargo test -p celesta-plugin -p celesta-plugin-host` passes.
- [ ] **Step 5: Commit:** `feat(plugin): answer hello with the protocol of the plugin's role`.

### Task 2: The media manifest and the media plugin listing (`celesta-plugin-host`)

**Files:**
- Modify: `crates/plugin-host/src/manifest.rs`
  - `Manifest` `:53-97`: add the `media` field, `PluginRole::Media`, and remove `Codec`
  - `invalid_reason` `:440`
  - `read_manifest`'s protocol check `:574`
  - `CodecPlugin` and `sibling_codec` `:318-350`
  - `InstalledPlugins::codec` `:846`
  - tests `:1530-1572`, `:1800-1816`
- Create: `crates/plugin-host/src/media.rs`, holding the media manifest types, `MediaOrigin`, `MediaPlugin`, `InstalledPlugins::media`, and the sibling's `--manifest`
- Modify: `crates/plugin-host/src/lib.rs` (module and re-exports)
- Test: `crates/plugin-host/src/media.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces, in `celesta_plugin_host`:
  ```rust
  pub enum PluginRole { Content, Media, Unknown }          // "codec" now parses as Unknown
  pub struct MediaManifest { pub decoders: Vec<MediaDecoderEntry> }   // Manifest.media: Option<MediaManifest>
  pub struct MediaDecoderEntry { pub id: String, pub name: String, pub extensions: Vec<String> }
  pub enum MediaOrigin { User, Bundled, Project }          // Ord: User < Bundled < Project
  pub struct MediaPlugin { pub id: String, pub version: String, pub executable: PathBuf,
                           pub directory: PathBuf, pub origin: MediaOrigin, pub decoders: Vec<MediaDecoderEntry> }
  impl InstalledPlugins { pub fn media(&self) -> Vec<MediaPlugin> }   // search order; replaces codec()
  pub const RESERVED_PLUGIN_ID: &str = "builtin";
  pub const CODEC_PROGRAM: &str = "celesta-codec-ffmpeg";              // kept, still the sibling's name
  pub const MANIFEST_FLAG: &str = "--manifest";
  ```
- Also produced: `DiscoveryError::BadComponent { path: PathBuf, reason: String }`, a non-fatal entry in `find`'s error list. Its text is `"{path}: a media component was skipped: {reason}"`.
- `CodecPlugin` and `InstalledPlugins::codec` are removed.

- [ ] **Step 1: Write failing tests** in `media.rs`. Use the manifest-writing helpers the existing `manifest.rs` tests use (`write(…)`, `COMPATIBLE`).
  - `reads_media_decoders`: a manifest with `"protocol":2, "roles":["media"], "media":{"decoders":[{"id":"ffmpeg","name":"FFmpeg","extensions":["mp4","MOV"]}]}` lists one `MediaPlugin` with that decoder. Extensions are lower-cased (`["mp4","mov"]`).
  - `a_codec_role_is_ignored`: `"roles":["codec"]`, protocol 1, discovered without error, appears in no `media()` result.
  - `content_and_media_together_are_invalid`: a `DiscoveryError::Invalid` whose reason contains `"both content and media"`.
  - `builtin_is_reserved`: id `builtin` gives `Invalid`, with a reason containing `"reserved"`.
  - `media_needs_protocol_2`: a media manifest with `"protocol":1` is `Invalid`. A content manifest with `"protocol":2` is `Invalid`.
  - `a_bad_decoder_is_skipped_alone`: `decoders: [{"id":"ok","name":"OK","extensions":[]}, {"id":3}]` keeps `ok` and adds one `BadComponent` to `find`'s errors.
  - `origins_follow_sources`: plugins under `Explicit`, `Project`, `User`, and `Bundled` search paths get `User`, `Project`, `User`, and `Bundled`.
  - `media_lists_in_search_order`: two media plugins under two explicit paths come back in that order.
  - `the_sibling_is_listed_once`: a fake program beside a fake executable prints `{"id":"ffmpeg","version":"0.10.0","roles":["media"],"media":{"decoders":[…]}}` for `--manifest`. It is listed with origin `Bundled` when no discovered plugin has id `ffmpeg`, and not when one does. The fake program is a shell script on Unix (`#[cfg(unix)]`), as `child.rs`'s tests make fakes.
  - `a_sibling_that_hangs_is_left_out`: a sibling that sleeps 30 s is left out, and `media()` returns within 6 s.
  - `a_sibling_in_a_build_folder_is_ignored`: the existing `.fingerprint` / `.cargo-lock` rule, carried over from `sibling_codec`.
- [ ] **Step 2:** `cargo test -p celesta-plugin-host media` fails.
- [ ] **Step 3: Implement.**
  - **Manifest parsing.** `Manifest.media` is a lenient `Option<MediaManifest>`. Each decoder entry is deserialized from a `serde_json::Value` on its own, so a bad one becomes a `BadComponent` and does not fail the manifest. `read_manifest` returns those beside the entry, and `discover_in_with` appends them to its error list.
  - **Protocol check.** The check in `read_manifest` uses `celesta_plugin::protocol::protocol_for_roles` on the manifest's role names.
  - **The sibling.** `sibling_media(executable: &Path) -> Option<MediaPlugin>` replaces `sibling_codec` and keeps its build-folder rule. It runs `<program> --manifest` with a 5 s deadline (kill on timeout) and parses stdout. The result is cached in a `static OnceLock<Mutex<HashMap<(PathBuf, u64 size, SystemTime mtime), Option<MediaPlugin>>>>`.
  - **`media()`.** It returns every entry in `self.plugins` with `PluginRole::Media` and an executable for this platform, in order. The sibling is appended unless a listed entry has the same id as the sibling's printed manifest (`ffmpeg`).
- [ ] **Step 4:** Update the tests that used `codec()` (`reads_roles_and_picks_the_first_codec` and the sibling tests) to `media()`, then run `cargo test -p celesta-plugin-host`, which passes.
- [ ] **Step 5: Commit:** `feat(plugin-host): media manifests and the media plugin listing`.

### Task 3: Protocol 2 decoding in `celesta-media`

**Files:**
- Modify: `crates/media/src/codec.rs`, whose trait is renamed: `CodecProvider` → `MediaProvider`. The file stays `codec.rs` until P2a, to keep the diff readable.
- Modify: `crates/media/src/codec/protocol.rs`
  - `ROLES` `:26`
  - `answer` `:144-254`
  - `encode_error` / `decode_error` `:259-358`
  - the request builders `:370-470`
- Modify: `crates/media/src/codec/process.rs`, where `CodecProcess` → `MediaProcess`, with the role check `:113-121` and `hello_as(2, …)`.
- Modify: `crates/media/src/error.rs`
- Modify: `crates/media/src/lib.rs` re-exports
- Test: `crates/media/src/codec/protocol.rs` tests (its fakes at `:564`, `:738`, `:853`, `:921`) and `crates/media/src/tests.rs`

**Interfaces:**
- Consumes: Task 1's `MEDIA_PROTOCOL_VERSION` and `hello_as`.
- Produces:
  ```rust
  pub trait MediaProvider: Send {
      fn probe(&mut self, decoder: &str, path: &Path) -> Result<MediaProbe, MediaError>;
      fn decode_video(&mut self, decoder: &str, request: &VideoRequest<'_>) -> Result<VideoFrame, MediaError>;
      fn decode_audio(&mut self, decoder: &str, path: &Path, sample_rate: u32, channels: u16) -> Result<AudioBuffer, MediaError>;
      // unchanged until P2a: check_encoder, open_encoder, encode_frame, finish_encoder, abort_encoder, mux_audio
  }
  pub const ROLES: &[&str] = &["media"];
  pub struct MediaProcess;   // was CodecProcess; start / start_with unchanged in shape
  MediaError::Unsupported { path: PathBuf, tried: Vec<String> }  // "no decoder could read {path} (tried {tried})"
  MediaError::NoDecoder                                           // "no media decoder is installed, so video and audio files cannot be read"
  ```
- On the wire:
  - `probe`, `decodeVideo`, and `decodeAudio` carry `"decoder": <id>`.
  - The error code `"unsupported"` decodes to `Unsupported { path, tried: vec![] }`. It carries `path`, the path as sent, and `MediaProcess` renames it to the caller's path as `named` does.
  - `MediaError::NoCodec` stays, meaning "no plugin to encode with", until P2a.

- [ ] **Step 1: Write failing tests** in `protocol.rs`'s tests, on the existing fake-provider pattern:
  - `requests_name_their_decoder`: `probe_request("ffmpeg", path)`'s header has `decoder == "ffmpeg"`, and the same goes for the video and audio requests.
  - `serve_passes_the_decoder_on`: a fake `MediaProvider` records the `decoder` it was given for each of the three requests, and serving `{"type":"probe","decoder":"x",…}` records `"x"`.
  - `unsupported_round_trips`: `decode_error(&encode_error(&Unsupported{path, tried: vec![]}))` matches `Unsupported { path, .. }`.
  - `a_media_process_needs_the_media_role`: the existing role-check test, updated. A program answering `roles: ["codec"]` is refused with `CodecUnavailable` containing `"is not a media plugin"`.
- [ ] **Step 2:** `cargo test -p celesta-media protocol` fails.
- [ ] **Step 3: Implement.**
  - Rename and thread `decoder` through.
  - `serve` / `serve_on` call `serve_requests_with_hello[_on]` with `ROLES`, so they answer protocol 2 through Task 1.
  - `MediaProcess::start_with` uses `hello_as(MEDIA_PROTOCOL_VERSION, offer)`.
  - Update the four fakes and every `CodecProvider` mention in `crates/media`.
- [ ] **Step 4:** `cargo test -p celesta-media` passes, except the backend tests Task 4 rewrites. Mark them `#[ignore = "rewritten in Task 4"]` only if they cannot compile otherwise, and remove the marks in Task 4.
- [ ] **Step 5: Commit:** `feat(media): protocol 2 decoding names the decoder`.

### Task 4: `MediaRegistry` and decoder routing in `MediaBackend`

**Files:**
- Create: `crates/media/src/registry.rs`
- Modify: `crates/media/src/backend.rs` (whole file), `crates/media/src/codec.rs`, where `CodecFactory`, `set_codec`, and `codec` are removed, and `crates/media/src/lib.rs`
- Test: `crates/media/src/tests.rs`, with the backend tests rewritten against the registry

**Interfaces:**
- Consumes: Task 2's `MediaOrigin` and `MediaDecoderEntry`; Task 3's `MediaProvider`, `MediaError::{Unsupported, NoDecoder}`.
- Produces, in `celesta_media`:
  ```rust
  pub struct ComponentRef { pub plugin: String, pub component: String }   // Display/FromStr "plugin/component"
  pub type ProviderFactory = Arc<dyn Fn(&PluginKillSwitch) -> Result<Box<dyn MediaProvider>, MediaError> + Send + Sync>;
  pub struct RegisteredPlugin { pub id: String, pub origin: MediaOrigin,
                                pub decoders: Vec<MediaDecoderEntry>, pub factory: ProviderFactory }
  pub struct DecoderUse { pub decoder: ComponentRef, pub origin: MediaOrigin, pub path: PathBuf }
  pub struct MediaRegistry { … }
  impl MediaRegistry {
      pub fn new(plugins: Vec<RegisteredPlugin>) -> Self;          // plugins in search order
      pub fn has_decoders(&self) -> bool;
      pub fn decoder_candidates(&self, path: &Path) -> Vec<(usize, ComponentRef)>;  // (plugin index, decoder)
      pub fn legacy_encoder(&self) -> Option<usize>;               // index of plugins[0], if any
      pub fn plugin(&self, index: usize) -> &RegisteredPlugin;
      pub fn record_use(&self, decoder: &ComponentRef, origin: MediaOrigin, path: &Path);
      pub fn project_decoder_uses(&self) -> (u64, Vec<DecoderUse>); // generation, project-origin uses only
  }
  pub fn set_registry(registry: Option<Arc<MediaRegistry>>);
  pub fn registry() -> Option<Arc<MediaRegistry>>;
  impl MediaBackend {
      pub fn new() -> Self;                                         // registry chosen at first use
      pub fn with_registry(registry: Arc<MediaRegistry>) -> Self;   // replaces with_codec
      pub fn decoder_of(&self, path: &Path) -> Option<&ComponentRef>;
      // with_sequential_video, with_kill_switch, probe, and the encoder methods keep their signatures
  }
  ```
- **Candidate order.**
  - Candidates are the decoders whose `extensions` contain the path's lower-cased extension. When none match, or the path has no extension, every decoder is a candidate.
  - Sort them stably by `origin`, then plugin index, then manifest order.

- [ ] **Step 1: Write failing tests** in `tests.rs`. Each uses an in-process fake provider factory that counts starts and records requests.
  - `candidates_follow_extension_then_origin`: plugins `p` (Project, `mp4`), `b` (Bundled, `mp4`), and `u` (User, `mp4`) give `[u, b, p]` for `clip.MP4`.
  - `no_extension_match_tries_every_decoder`: for `clip.xyz`, all three are candidates, in origin order.
  - `unsupported_moves_to_the_next_decoder`: `u` answers `Unsupported` and `b` succeeds. `probe` returns `b`'s probe, and `decoder_of` is `b/<id>`.
  - `a_routed_path_stays_pinned`: after that, `decode_frame` and `decode_audio` go only to `b`, and `u` sees no further requests.
  - `decode_without_probe_routes_first`: `decode_frame` on a fresh path probes first, then decodes through the winner.
  - `missing_file_starts_no_process`: for a path that does not exist, the error is `NotFound(path)` and the start count is 0.
  - `a_stopped_decoder_ends_routing`: `u` answers `CodecStopped`. The error is `CodecStopped`, and `b` is never started.
  - `an_unavailable_decoder_falls_through`: `u`'s factory yields providers that answer `CodecUnavailable` to `probe`. It is started twice (the retry), then `b` is used.
  - `every_decoder_failing_names_them`: all three answer `Unsupported`. The error is `Unsupported { tried: ["u/…","b/…","p/…"] }`.
  - `a_real_error_beats_unsupported`: `u` answers `Codec("broken")` and the others `Unsupported`. The error is `Codec("broken")`.
  - `no_decoders_is_no_decoder`: an empty registry gives `NoDecoder`.
  - `project_uses_are_recorded`: routing to `p` adds one `DecoderUse` and bumps the generation. Routing to `u` or `b` adds none.
  - `encoding_uses_the_first_plugin`: `open_encoder` goes to `plugins[0]` whatever its decoders. An empty registry gives `NoCodec`.
  - `a_backend_made_before_set_registry_uses_it`: `MediaBackend::new()` is made first, then `set_registry(…)`, then `probe` uses that registry. The existing test, ported.
- [ ] **Step 2:** `cargo test -p celesta-media` fails.
- [ ] **Step 3: Implement.**
  - `MediaBackend` keeps:
    - `registry: Option<Arc<MediaRegistry>>`, resolved from `registry()` at first use
    - `processes: HashMap<usize, Box<dyn MediaProvider>>`
    - `routes: HashMap<PathBuf, (usize, ComponentRef)>`
    - the probe cache
  - `retrying` takes the plugin index and reopens that plugin's process once on `CodecUnavailable`.
  - `route(path)`:
    1. Return `NotFound` unless `path.exists()`.
    2. Walk the candidates. On `Unsupported`, continue. On `CodecStopped`, return it. On any other error, remember the first one and continue. On success, pin the path, cache the probe, call `record_use`, and return.
    3. When every candidate fails, return the remembered error, else `Unsupported { tried }`.
  - `probe`, `decode_frame`, `decode_frame_for`, and `decode_audio` route first. `video_duration` uses `probe`.
- [ ] **Step 4:** `cargo test -p celesta-media` passes, with no ignored tests left.
- [ ] **Step 5: Commit:** `feat(media): route probes and decodes to decoder components`.

### Task 5: The test codec as a media plugin (`celesta-codec-test`)

**Files:**
- Modify: `crates/codec-test/src/lib.rs` (the `CodecProvider` impl, `factory`, `install` `:55-71`), `crates/codec-test/src/main.rs`, and `crates/codec-test/src/plugin.rs:76-81`
- Test: `crates/codec-test/tests/{codec,process,plugin_folder}.rs`

**Interfaces:**
- Consumes: Tasks 3 and 4.
- Produces:
  - `pub const DECODER_ID: &str = "test";`
  - `TestCodec: MediaProvider`, which accepts only `decoder == DECODER_ID`. Anything else is `MediaError::Codec("no decoder …")`.
  - `pub fn registered() -> RegisteredPlugin`: id `PLUGIN_ID`, origin `Bundled`, one decoder `{id:"test", name:"Test codec", extensions: []}`, and an in-process factory.
  - `install()`, which becomes `set_registry(Some(Arc::new(MediaRegistry::new(vec![registered()]))))`.
  - `TestCodecPlugin`'s manifest: `"protocol": 2, "roles": ["media"], "media": {"decoders": [{"id":"test","name":"Test codec","extensions":[]}]}`.
  - The test codec reads a file of any extension (`format.rs` sniffs its magic bytes) and answers `Unsupported` for a file it does not recognize. Today that is a `Codec` error at `format::sniff`; change it to `Unsupported`.

- [ ] **Step 1: Update the tests first.**
  - `plugin_folder.rs` lists the folder through `InstalledPlugins::media()` and expects one plugin with decoder `test`.
  - `process.rs` drives the program through `MediaProcess` with `decoder: "test"`.
  - Add `a_foreign_file_is_unsupported`, which writes a PNG's first bytes and expects `probe` to be `Unsupported`.
- [ ] **Step 2:** `cargo test -p celesta-codec-test` fails.
- [ ] **Step 3: Implement** the interfaces above.
- [ ] **Step 4:** `cargo test -p celesta-codec-test` passes, and so does `cargo test -p celesta-media`. The exporter and the editor are updated in Tasks 7 and 8.
- [ ] **Step 5: Commit:** `feat(codec-test): serve the media role on protocol 2`.

### Task 6: The FFmpeg plugin as a media plugin (`plugins/ffmpeg`)

**Files:**
- Modify: `plugins/ffmpeg/plugin.json`, `plugins/ffmpeg/src/lib.rs` (`FfmpegCodec`, `factory`, `install` `:31-128`), and `plugins/ffmpeg/src/main.rs`
- Test: `plugins/ffmpeg/tests/{process,decode,export}.rs` and `plugins/ffmpeg/examples/codec-bench.rs`

**Interfaces:**
- Consumes: Tasks 3 and 4. Task 9's manifest tooling generates `celesta-plugin.json` from `plugin.json`.
- Produces:
  - `plugin.json` gains `"roles": ["media"]` and `"media": {"decoders": [{"id": "ffmpeg", "name": "FFmpeg", "extensions": [...]}]}`.
    - The extensions are: `mp4`, `m4v`, `mov`, `webm`, `mkv`, `avi`, `mpg`, `mpeg`, `ts`, `m2ts`, `mts`, `ogv`, `gif`, `mp3`, `wav`, `flac`, `ogg`, `oga`, `m4a`, `aac`, `opus`, `aif`, `aiff`, `caf`, `wma`, `wmv`.
    - These are the formats the plugin's tests and the user guide read; an unlisted one still reaches FFmpeg through the no-match fallback.
  - `pub const DECODER_ID: &str = "ffmpeg";` and `FfmpegCodec: MediaProvider`.
  - `pub fn registered() -> RegisteredPlugin` (origin `Bundled`), with `install()` setting a registry of it.
  - `celesta-codec-ffmpeg --manifest` prints `plugin.json`'s `id`, `version` (`CARGO_PKG_VERSION`), `roles`, and `media` as one JSON line, then exits 0. The file is embedded with `include_str!("../plugin.json")`.
  - `FfmpegCodec::probe` answers `Unsupported` when FFmpeg cannot open the input as a container (`AVERROR_INVALIDDATA` from opening). A missing file stays `NotFound`.

- [ ] **Step 1: Write failing tests.**
  - `process.rs`: the existing tests, ported to `MediaProcess` and `decoder: "ffmpeg"`.
  - `manifest_flag_prints_the_media_manifest`: runs the built program with `--manifest` and parses `media.decoders[0].id == "ffmpeg"`.
  - `a_text_file_is_unsupported`: probing a `.mp4` that holds `"hello"` gives `Unsupported`.
- [ ] **Step 2:** `PKG_CONFIG_PATH=… cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg` fails.
- [ ] **Step 3: Implement** the interfaces above. Update `codec-bench.rs` to the new names.
- [ ] **Step 4:** The same command passes, and so does `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`.
- [ ] **Step 5: Commit:** `feat(codec-ffmpeg): serve the media role on protocol 2`.

### Task 7: Wiring the registry into the exporter, MCP, the React bridge, and benches

**Files:**
- Modify: `crates/exporter/src/codec.rs` → rename it to `crates/exporter/src/media.rs`. Also `crates/exporter/src/lib.rs:23`, `exporter.rs:27-32`, `main.rs:575`, and `report.rs:530-570` with its tests `:973-991`.
- Modify: `crates/mcp/src/main.rs:30`, `crates/react-bridge/examples/audio-bench.rs:25-32`, `crates/exporter/examples/preview-bench.rs:33-40`

**Interfaces:**
- Consumes: Tasks 2–4.
- Produces, in `celesta_exporter`:
  - `pub fn media_registry(plugins: &InstalledPlugins) -> Arc<MediaRegistry>`: one `RegisteredPlugin` for each `plugins.media()` entry, whose factory starts `MediaProcess`.
  - `pub fn use_media(plugins: &InstalledPlugins) -> Arc<MediaRegistry>`, which sets it with `set_registry` and returns it.
  - `pub fn use_installed_media()`.
  - `pub(crate) fn ensure_media(plugins: &InstalledPlugins)`, which sets the registry only when `registry()` is `None`.
  - These replace `use_codec`, `use_installed_codec`, and `ensure_codec`.
- Report codes:
  - `MediaError::NoDecoder`, anywhere in the chain, becomes `"no_decoder"`, with the hint `NO_DECODER_HINT` (the old `NO_CODEC_HINT` text with "codec plugin" → "media plugin").
  - `MediaError::Unsupported` becomes `"unsupported_media"`, with no hint.
  - `MediaError::NoCodec` stays `"no_codec"` (encoding), until P2a.

- [ ] **Step 1: Write failing tests** in `report.rs`:
  - `media_nothing_could_decode_is_reported_as_no_decoder` (renamed from `…no_codec`). It uses `NoDecoder` and expects `"no_decoder"`.
  - `an_unreadable_file_is_unsupported_media`.

  In `media.rs`: `registry_lists_media_plugins_in_order`, which takes two fake media plugin folders and expects `media_registry(...)` to have them at indexes 0 and 1.
- [ ] **Step 2:** `cargo test -p celesta-exporter report media` fails.
- [ ] **Step 3: Implement** and update every caller listed under Files.
- [ ] **Step 4:** `cargo test -p celesta-exporter -p celesta-mcp -p celesta-react-bridge` passes, and `cargo build --workspace --examples` builds.
- [ ] **Step 5: Commit:** `feat(exporter): read media through the media registry`.

### Task 8: The editor's media problems and the project-decoder notice

**Files:**
- Modify: `crates/editor/src/plugins.rs`
  - `missing_codec` → `missing_decoder` (`:46`, `:56`, `:89`)
  - `problem` (`:93-117`)
  - `missing_codec_detail` → `missing_decoder_detail` (`:122-160`)
  - tests (`:541-570`)
- Modify: `crates/editor/src/plugins_panel.rs`, where `PluginRow` gains a decoder-use line
- Modify: `crates/editor/src/view.rs` (or `refresh.rs`, wherever the view's per-frame refresh already runs), to call `sync_decoder_notice`
- Modify: `crates/editor/src/problems.rs`, adding `Source::MediaDecoders`

**Interfaces:**
- Consumes: Task 7's `use_media`, and Task 4's `has_decoders` and `project_decoder_uses`.
- Produces:
  - `FoundPlugins.missing_decoder: bool`, set to `!use_media(&found.installed).has_decoders()`.
  - `FoundPlugins.registry: Option<Arc<MediaRegistry>>`.
  - `pub(crate) fn decoder_notice(uses: &[DecoderUse]) -> Option<Problem>`:
    - title `"<plugin> decodes 1 file in this project"`, or `"… <n> files …"`
    - one line per plugin, then its files, one per line, relative to the project root when inside it
    - `None` when `uses` is empty
  - `PluginRow::decoder_uses(&self, uses: &[DecoderUse]) -> Option<String>`: `"Decodes 3 files in this project"` for a project plugin with uses, else `None`.
  - `EditorView::sync_decoder_notice(&mut self)`: reads `project_decoder_uses()` and, only when the generation changed, sets `Source::MediaDecoders`.

- [ ] **Step 1: Write failing tests** in `plugins.rs`'s tests:
  - `a_missing_decoder_is_a_problem`, ported. Its title is `"No media decoder can run"`, and its detail contains `"No media plugin was found"`.
  - `an_old_codec_plugin_is_not_a_reason`: an installed plugin with `roles: ["codec"]` leaves the detail at `"No media plugin was found"`.
  - `a_waiting_media_plugin_is_the_reason`: the waiting text, with "media plugin".
  - `decoder_notice_counts_files`: two uses by `proj/dec` give the title `"proj decodes 2 files in this project"`.
  - `decoder_notice_is_none_without_uses`.

  In `plugins_panel.rs`'s tests: `a_project_plugin_row_shows_its_decoder_uses`.
- [ ] **Step 2:** `cargo test -p celesta-editor plugins` fails.
- [ ] **Step 3: Implement.** The detail texts keep today's structure, with "codec plugin" → "media plugin" and `PluginRole::Codec` → `PluginRole::Media`. The last branch's build hint stays.
- [ ] **Step 4:** `cargo test -p celesta-editor` passes.
- [ ] **Step 5:** Manual check, using the `celesta-verify-tools` memory: build the editor, open `examples/greenscreen` with the FFmpeg plugin staged by `node scripts/plugins/build.mts plugins/ffmpeg` on `CELESTA_PLUGIN_PATH`, and confirm the video previews and Problems has no media entry. Then start it with no plugin and confirm "No media decoder can run".
- [ ] **Step 6: Commit:** `feat(editor): media decoder problems and the project-decoder notice`.

### Task 9: Plugin tooling and packaging checks

**Files:**
- Modify: `scripts/plugins/manifest.mts`
  - the `PluginSource` type `:17-18`
  - the `PluginManifest` type `:41-44`
  - `PLUGIN_SOURCE_KEYS` `:52`
  - generation `:117-119`
- Modify: `scripts/plugins/test/manifest.test.mts`
- Modify: `scripts/test-package.mjs:68-78`

**Interfaces:**
- Produces:
  - `plugin.json` may have `media`, which is copied verbatim into `celesta-plugin.json`.
  - `protocol` is `2` when `roles` includes `"media"`, else `1`. The type is `protocol: 1 | 2`.
  - `plugin.json` with both `content` and `media` roles is rejected with `"a plugin is either content or media"`.

- [ ] **Step 1: Write failing tests** in `manifest.test.mts`:
  - `copies media and speaks protocol 2`: `media` is copied, and the protocol is 2.
  - `keeps protocol 1 for content`.
  - `rejects content and media together`.
- [ ] **Step 2:** `pnpm --filter celesta-plugin-scripts test` fails. Use the package's own test script name.
- [ ] **Step 3: Implement.** In `test-package.mjs`, assert `roles?.includes('media')` and `media?.decoders?.length > 0`, with messages naming "the media plugin".
- [ ] **Step 4:** The tests pass, and so does `node --check scripts/test-package.mjs`. `node scripts/plugins/build.mts plugins/ffmpeg` writes a `celesta-plugin.json` with `"protocol": 2` and `media.decoders`.
- [ ] **Step 5: Commit:** `build(plugins): media manifests in the plugin tooling`.

### Task 10: Docs, the spec's deviations, the handoff note, and the end-to-end check

**Files:**
- Modify: `packages/website/content/en/docs/05-(more)/05-plugins.md` (the codec plugin section `:105-111`, and `:24`'s "a codec plugin …"), and the same page under `ja/`. Also `04-troubleshooting.md` in both languages, wherever it names `no_codec` for reading.
- Modify: `skills/celesta/references/plugins.md`, at its 8 `codec` mentions, and `references/errors.md`, adding `no_decoder` and `unsupported_media`.
- Modify: `HANDOFF.md`'s workspace map rows for `celesta-media`, `celesta-codec-ffmpeg`, `celesta-codec-test`, and `celesta-plugin-host` (the "codec role" and "`InstalledPlugins::codec`" wording), and its architecture boundary bullet.
- Modify: `docs/superpowers/specs/2026-11-08-media-components-design.md`, recording deviations 1–5 in Capability discovery, Registry and routing, Decoders, and Phases → P1/P2a.
- Create: `docs/handoff/2026-11-08-media-decoders.md`, with the spec, the implementation details, and the checks, in the existing handoff notes' format.

- [ ] **Step 1: Write the docs.**
  - The guide explains:
    - media plugins and decoders, and the decoder order (user → bundled → project)
    - the fallback, and the project-decoder notice
    - "No media decoder can run" and `no_decoder`
    - that the `cargo install` sibling still works
    - that an old `codec` plugin is ignored and needs a release for this Celesta
  - Keep the en and ja pages in step.
- [ ] **Step 2: Verify.** Each of these passes:
  - `cargo test --workspace`
  - `cargo clippy --workspace --all-targets --locked -- -D warnings`
  - `cargo fmt --all -- --check`, with only the files `main` already leaves unformatted
  - with `PKG_CONFIG_PATH` set: `cargo test --manifest-path plugins/Cargo.toml --workspace --locked` and `cargo clippy --manifest-path plugins/Cargo.toml --workspace --all-targets --locked -- -D warnings`
  - `pnpm run test:runtime`, and the `scripts/plugins` tests
- [ ] **Step 3: Run the end-to-end check** (as `2026-11-05-codec-plugin.md`'s Checks did).
  - Stage the FFmpeg plugin with `build.mts` into a scratch `CELESTA_PLUGIN_PATH` folder.
  - Export `examples/greenscreen` as MP4 with the branch's `celesta-exporter`, and a `<Video>` + `<Audio>` entry as MP4 with `--loop` and as MOV.
  - Compare with the exporter built from `main`. The outputs are byte-identical, since P1 changes no encoding.
  - Run the exporter with no plugin. Its JSON is `no_decoder` for a video entry, or `no_codec` for a composition without media files.
- [ ] **Step 4:** Record the results, the measured numbers, and any check not run in the handoff note.
- [ ] **Step 5: Commit:** `docs: media plugins and decoders (P1)`.
