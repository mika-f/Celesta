# No console window for helper processes (2026-11-05)

Issue #688: on Windows, the release editor (a GUI-subsystem program) opened
a console window for every plugin process it started, because a
console-subsystem child of a GUI program gets a new console window unless it
is started with `CREATE_NO_WINDOW`.

## Specification

- Every helper process the editor starts is started with `CREATE_NO_WINDOW`
  on Windows: plugin processes (content plugins and the codec plugin), the
  Node.js runtime of the React bridge, and the external code editor opened
  from the editor.
- The flag is set in one place, `celesta_plugin_host::without_console_window`
  (`crates/plugin-host/src/console.rs`), with the one named
  `CREATE_NO_WINDOW` constant. Elsewhere than Windows it does nothing.
- A plugin's stderr is piped and passed through to Celesta's stderr, with its
  last lines kept for errors (`crates/plugin-host/src/child.rs`), so hiding
  the console loses none of it.

## Implementation

- `PluginChild::start` already set the flag since `73908b2c` (#681), which
  moved the content plugins and the codec plugin onto that one start path.
  The React bridge (`crates/react-bridge/src/bridge.rs`) and the editor
  launcher (`crates/editor/src/editor_launch.rs`) set it too, each with its
  own `0x0800_0000` literal. All three now call `without_console_window`.
- The helper lives in `celesta-plugin-host` because `celesta-react-bridge`
  and `celesta-editor` already depend on it; no `windows-sys` dependency is
  added for one constant.

## Checks

- `cargo clippy -p celesta-plugin-host --target x86_64-pc-windows-msvc
  --all-targets -- -D warnings` compiles the Windows branch. The other two
  crates cannot be cross-checked from macOS (`ring`'s C build needs the MSVC
  headers), but their call sites are no longer platform-specific.
- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --all -- --check`.
- No test: a test of the constant's value would prove nothing, and whether a
  window opens can only be seen on Windows.
- By hand, on Windows: build the release editor
  (`cargo build -p celesta-editor --release`), open a project with the flag
  sample allowed, add a flag layer, and confirm that no console window
  opens. Then open a scene that decodes a video or audio file and confirm the
  same for the codec plugin.
