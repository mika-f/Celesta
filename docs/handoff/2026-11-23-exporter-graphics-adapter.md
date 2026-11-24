# Exporter graphics adapter reporting

The CLI reports the graphics backend it actually selected, not just the requested
`--driver`. Once the renderer initializes, the final JSON result contains
`graphics.backend`, `graphics.adapter`, and `graphics.software`. Human output
includes a `graphics:` line; it also appears in the interactive dashboard's
scrollback. The JSON progress event stream is unchanged. Commands that don't
initialize the renderer, such as `--info` or subtitle-only export, omit the field.

The common exporter renderer constructor emits the selected wgpu adapter info
for project and React video, stills, and quality checks. The software flag uses
the renderer's existing CPU-adapter detection. On Linux, an adapter selection
failure keeps the `render` error code and adds a Mesa/lavapipe setup hint.

## Checks

- `cargo fmt --check -p celesta-exporter`
- `cargo test --locked -p celesta-exporter --features runtime-tests` with Mesa
  lavapipe, after building the project runtime and the test codec.
- `cargo clippy --locked -p celesta-exporter --all-targets --features runtime-tests -- -D warnings`
- Headless PNG export with `--driver vulkan`, with and without `--json`;
  repeat with a nonexistent Vulkan ICD to check the error and hint.
