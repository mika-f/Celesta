# Shared background-worker types in the editor (2026-10-28)

The editor's seven background threads now share their channel, thread and
"worker stopped unexpectedly" plumbing (issue #573). Only the five that
really are latest-only share coalescing and generations: export and the
occurrence sweep keep their own loops, as the issue's investigation comment
found their contracts differ.

## Types

`crates/editor/src/worker.rs`:

- `Worker<Req, Res>`: a thread named `celesta-<name>` with a request and a
  result channel. `spawn(name, body)` hands `body` both channel ends;
  `request` fails with `"<name> worker stopped unexpectedly"` once the thread
  ended; `poll()` takes every result that arrived and sets `stopped` on every
  poll after the thread ended. The caller decides whether that matters.
- `LatestWorker<Req, Res>`: a `Worker` whose loop serves the newest queued
  request (`take_latest`) with `serve(request, &Job)`. It owns the
  generation and the pending flag that `EditorView` used to keep per worker:
  - `supersede()` starts a new generation, clears pending, and publishes the
    generation to running jobs; `Job::superseded()` reads it (the audio mix's
    cancellation).
  - `request(req)` tags it with the current generation; pending until polled.
  - `poll()` keeps results of the current generation; `poll_newer_than(shown)`
    keeps every newer one (the preview, which shows a render behind while
    renders are slower than requests). A current result clears pending.
  - `stopped` is set once, only when the thread ended with a request pending,
    and clears pending.
- `take_latest` moved here from `audio.rs`.

Request and result types lost their `generation` fields; the wrapper tags
them. Results that were a single field became type aliases
(`MediaProbeResult`, `ReactAudioResult`, `AudioMixResult`).

| Worker | Type | Contract |
| --- | --- | --- |
| preview, media probe, component schema, react audio, audio mix | `LatestWorker` | newest only; stale results dropped |
| occurrences | `Worker` | newest only, abandons a running sweep between chunks; results carry the reload generation `land` matches |
| export | `Worker` | every request in order; progress then `Finished` |

Each module keeps a type alias (`PreviewWorker`, …) and a `spawn_*()`
function. Worker state such as `FfmpegBackend` and `CachedAudioDecoder` is now
built on the UI thread and moved in, as the preview's renderer already was.
The occurrence thread is now named `celesta-occurrence` (was
`celesta-occurrences`).

## `EditorView`

Eight fields are gone: `preview_generation`, `preview_pending`,
`media_generation`, `media_pending`, `audio_generation`, `audio_pending`,
`component_schema_generation`, `component_schema_pending`. Readers use
`*_worker.generation()` / `*_worker.pending()`. The audio pipeline spans two
workers, which `refresh_audio_preview` supersedes together;
`EditorView::audio_pending()` is either one pending.
`preview_shown_generation` stays, as it is the preview's own state.

`poll_background_work` calls one `poll_*` method per worker, in the old
order. No wider split of `EditorView` was done: the issue's comment advises
against splitting by field count alone.

## Behavior changes

- A preview request that fails because the thread ended now also tells
  `ElementEdit` the preview stopped. Before, only a poll that saw the
  disconnect did, and it did so even with nothing pending.
- A stopped react-audio worker is reported only while its sweep is pending,
  and a stopped mix worker only while a mix is pending. Before, one shared
  `audio_pending` flag covered both.

## Checks

- `cargo test -p celesta-editor`: 647 passed. `worker.rs` adds tests for
  coalescing behind a running job, `Job::superseded`, `poll_newer_than`, and
  reporting a stopped worker. `transform_drag::tests::a_left_edge_resize_is_one_undo`
  failed once in a parallel run and passed alone and on rerun.
- `cargo clippy -p celesta-editor --all-targets -- -D warnings` fails on
  `clippy::large_enum_variant` in `layer_select.rs` (`SourceLookup`) with
  rustc 1.99.1. A clean checkout of `a9463072` fails the same way, so the
  failure predates this change. With that lint allowed, clippy is clean.
- `cargo fmt --all -- --check`, `git diff --check`: clean.
- Launched the built-in demo: the threads start and nothing panics in 15 s.
  The preview and export were not checked by hand.
