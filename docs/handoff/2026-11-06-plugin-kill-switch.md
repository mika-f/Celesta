# Killing plugin processes from another thread (2026-11-06)

Issue #686: a plugin that hangs blocked the thread that asked it. Request
deadlines came with `2026-11-05-codec-plugin.md` (`PluginChild`), so a
hang ended at its deadline (30 s for a frame, up to an hour for a mux), but
nothing could end it sooner:

- **Export.** Cancel only set a flag that the export checks between frames
  and stages, so a frame waiting on a plugin went on waiting, and the editor
  refused Reload Plugins, File > Reload, opening another file, and plugin
  setting changes until the export thread returned.
- **Preview.** The preview's renderer, its `PluginHost`, and their processes
  live on the preview worker's thread. Reload Plugins replaces the view and
  drops the old worker, but a thread waiting on a plugin does not see its
  channels close, so the old thread, renderer, GPU device, and processes
  stayed until the deadline (longer, as a failed frame was retried on a
  fresh process, which could hang again).
- **Quit.** A plugin that spins never reads the end of its stdin, and could
  outlive Celesta.

## Specification

- **`PluginKillHandle`** (`crates/plugin-host/src/kill.rs`): kills one
  plugin process from any thread. `PluginChild` keeps its `Child` behind an
  `Arc<Mutex<…>>` that the handle holds weakly (`PluginChild::kill_handle`),
  so a handle does not keep a dropped process alive.
- **`PluginKillSwitch`**: kills every handle registered with it, and every
  one registered after it was killed, at once. Clones share it.
- **`RequestError::Stopped`**: the request's process was stopped on purpose:
  it missed its deadline, or a kill handle killed it while the request waited
  (the reader thread sees its stdout close and the request returns at once).
  Its message says which (`did not answer \`frame\` within 30 s`, or
  `was stopped while it worked on \`frame\``) and ends with the plugin's last
  stderr lines, as `Unavailable` does. `Unavailable` is left for a process
  that went away on its own or fell out of step.
- **`PluginHost`**:
  - `with_kill_switch` / `kill_switch`: every process it starts is
    registered with the switch before `hello`, so a slow start is killed too.
    Once the switch is killed it starts no process: every request fails as
    `PluginError::Unavailable` at once.
  - A request whose process was stopped is not retried on a fresh process;
    the process is forgotten and started again on the next request (reopening
    its models). A hung frame costs one deadline, not two. A process that
    went away on its own is still retried once, as before.
  - The deadlines are a `Deadlines` field (the same constants: `open` 120 s,
    `frame` 30 s, `close` 5 s), which tests shorten.
- **Codec**: `CodecProvider::kill_handle` (default `None`; `CodecProcess`
  returns its child's). `MediaBackend::with_kill_switch` registers the codec
  process it opens with the switch, and opens none once the switch is
  killed. `Stopped` maps to `MediaError::CodecUnavailable`, so the backend's
  own retry is unchanged (it refuses a fresh codec once the switch is
  killed). The codec's `hello` runs inside the factory, before the backend
  registers it, so a kill during the codec's start waits for its 30 s
  deadline.
- **Export** (`crates/exporter`): `ExportCancellation` holds a
  `PluginKillSwitch`, and `cancel()` kills it. The export's renderer (its
  `PluginHost` and video decoder), its encoder, the audio mix's decoder, and
  the mux register with it. Once cancelled, the error a killed request
  causes settles as `ExportError::Cancelled` (`ExportCancellation::settle`,
  in `finish_encode`, `render_stills`, the mux, and the audio mix), so the
  editor reports a cancelled export, not a failed one.
- **Editor preview** (`crates/editor/src/view.rs`): the preview renderer's
  `PluginHost` and video decoder share a switch, and the preview worker kills
  it when dropped (`LatestWorker::stopping_on_drop`). Replacing the view
  (Reload Plugins, File > Reload, opening another file) drops the old worker:
  the frame it waited on returns, the worker's thread sees its channels
  closed and ends, and the renderer, GPU device, and processes are dropped
  with it.
- **Quit**: every `PluginChild` is also registered with a process-wide
  switch; `celesta_plugin_host::kill_every_plugin()` kills it. The editor
  calls it from GPUI's `on_app_quit`.

## Checks

- `celesta-plugin-host` (Unix): `child::tests::killing_it_from_another_thread_ends_the_request_it_waits_on`,
  `a_switch_killed_already_kills_what_is_registered_later`, and the existing
  deadline test now expecting `Stopped`;
  `host::tests::a_frame_that_hangs_ends_at_its_deadline_without_a_second_try`
  (a shell-script plugin that answers `hello` and `open` and never `frame`:
  `frame` fails as unavailable at its shortened deadline, the process
  exited, and none was started again) and
  `killing_its_switch_ends_a_frame_in_progress_and_starts_nothing`.
- `celesta-exporter` integration (Unix, needs `packages/cli` built and a
  GPU): `export::cancel_ends_a_frame_waiting_on_a_plugin_that_hangs` exports
  a React entry whose `<Hang>` layer is that plugin's content and cancels
  while its frame waits: the export ends as `Cancelled` in about a second and
  the plugin's process is gone. Without the kill in `cancel()` it fails
  after the 30 s frame deadline.
- `celesta-editor`: `worker::tests::dropping_a_worker_stops_its_job_and_ends_its_thread`.
- User guide: the plugin chapter and the export guide (en and ja) say that
  cancelling an export stops the plugins it waits on, and Reload Plugins the
  preview's.

Not done: the editor's other workers that own a `MediaBackend` (audio mix,
media probe, React audio) are not given a switch; their requests end at the
codec's deadlines, and `kill_every_plugin` stops their processes on quit.
