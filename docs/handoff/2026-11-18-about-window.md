# About Celesta window (2026-11-18)

## Specification

- **Celesta → About Celesta** (`AboutCelesta`) opens a small fixed-size
  window with the name, the version, a one-line description, the license,
  and a link to the repository. The version, license, and repository come
  from the crate's `CARGO_PKG_*` values, so a pre-release build shows its
  full version.
- One About window exists at a time. Choosing the menu item again brings
  the open one to the front.
- `Esc` and `⌘ W` / `Ctrl W` close it (**File → Close Window** too).
- Pressing the version copies `Celesta <version>`; a check shows beside the
  version for 1.5 s. (A toast would cover the license and the link in a
  window this small, and a label that replaced the version would move the
  target of the next press.)
- No logo: the window shows the name as text in the brand orange.

### The strip

A strip of clips in the timeline's colors sits under the title bar, dimmed.
Pressing the version 7 times, each press within 600 ms of the one before,
plays it: a playhead crosses the strip twice (2.4 s each), and each clip
lifts as the playhead enters it, is lit under it, and fades behind it. Then
the strip rests again. Pressing on while it plays leaves it playing. This is
not in the user guide.

With `App::reduce_motion` set, the clips are lit for the same time without
a playhead or a lift.

## Implementation

Everything is in `crates/editor/src/about.rs`.

- `about::init` registers `AboutCelesta` with `cx.on_action`, so it works
  wherever the focus is. `about::show` keeps the window's handle in the
  `AboutWindow` global and opens a new window when activating that handle
  fails (the window was closed).
- The window is the app's first that is not an editor. `main.rs` quits when
  no window is left, so its `on_window_closed` handler first calls
  `about::close_when_left_alone`: an About window that is the only window
  left closes, which runs the handler again and quits. Otherwise closing the
  editor would leave a window with no menu bar on Windows and Linux.
  `lifecycle::quit_after_inline_commits` already skips windows whose root
  view is not an `EditorView`.
- The window uses `TitleBar::window_options()` and renders a `TitleBar`,
  like the editor's, and is neither resizable nor minimizable. Its key
  context is `CelestaAbout`; `main.rs` binds `Esc` and `⌘ W` to
  `CloseWindow` there.
- The strip is laid out with elements, not painted: each clip is an
  absolutely placed `div` whose left and width are shares of the strip.
  `PressStreak`, `playhead_at`, `glow`, and `lift` are plain functions of
  time and position. While `playhead_at` returns a position, `render` asks
  for the next animation frame.
- Only the first press of a streak copies. The check is held by a task in
  the view that clears it; a new copy replaces the task.

## Checks

- `cargo test -p celesta-editor about::`: the streak, the playhead, the
  glow and lift, one window for two requests, reopening after a close,
  closing when left alone, `CloseWindow`, the copy, the strip playing after
  seven presses, and reduced motion.
- `cargo test -p celesta-editor` and `cargo clippy -p celesta-editor
  --all-targets -- -D warnings` pass.
- By hand on macOS: the menu item opens the window; a press of the version
  copies it and shows the check; seven presses play the strip; `Esc` and
  `⌘ W` close the window; `⌘ Q` with it focused quits; and closing the
  editor window with it open quits. Windows and Linux, where the title bar
  draws its own controls, were not run.
