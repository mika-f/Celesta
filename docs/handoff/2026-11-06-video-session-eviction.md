# Closing idle video decode sessions (2026-11-06)

Issue #706: the FFmpeg codec plugin kept one open decode run per
`(layer id, file)` for sequential frame requests and never closed any. The
editor keeps one renderer, and so one codec process, per open document, and
a React layer's id is its path in the element tree, so inserting an element
above a `<Video>` gave it a new id and a new session while the old one stayed
open. Each session holds an `ez-ffmpeg` decode run (its own threads and a
bounded channel) and the last frame as RGBA (about 33 MB at 4K); they piled
up until the document was closed.

## Specification

- `FfmpegBackend` (`plugins/ffmpeg/src/decoder.rs`) counts the sequential
  requests it serves and stamps each session with the count when it is
  asked.
- After every sequential request it closes the sessions not asked during
  the last `IDLE_SESSION_FRAMES` (30) × open sessions requests. The renderer
  asks once a frame for each video layer it draws, so between two requests
  of a session still on screen come at most as many requests as there are
  sessions open: a closed session has missed at least 30 frames. A clip that
  comes back after that has jumped in time and would have re-seeked anyway.
- At most `MAX_VIDEO_SESSIONS` (16) stay open; past that, the least recently
  asked one is closed. A scene with more videos on screen at once re-seeks
  instead of growing without bound.
- No protocol change. `celesta_media::Sequential`'s documentation now says
  a codec may close a session that stops asking, and a later request for it
  seeks again.

## Implementation notes

- The issue suggested either a frame counter or a
  `release_video_sessions_except` call from `GpuRenderer`. The renderer's
  frames are not visible to the codec process, and a new protocol request
  would need every codec to answer it, so the plugin measures idleness on
  its own request count, scaled by the number of open sessions.
- `decode_sequential` wraps the previous body (now `serve_sequential`), so
  the fast paths' early returns still stamp the session and run the
  eviction.

## Validation

- `cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg --locked`:
  `closes_the_session_of_a_layer_that_stopped_asking` (layers `a` and `b`,
  then only `a` for 90 frames, leaves only `a`'s session, without `a`
  re-seeking) and `closes_the_least_recently_asked_session_past_the_cap`
  fail with the eviction call removed and pass with it;
  `keeps_the_sessions_still_on_screen_without_reseeking` (two layers asked
  every frame) checks `sequential_video_processes_started` stays at 2.
- `cargo clippy --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg --all-targets --locked -- -D warnings`
  and `cargo fmt --check` in `plugins/`.
