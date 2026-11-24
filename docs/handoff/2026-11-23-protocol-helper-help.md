# Help and version for protocol helpers

`celesta-react-render` and `celesta-mcp` are protocol helpers, not the
command-line exporter. When invoked with just `--help` (`-h`) they explain
their role and point to `celesta-exporter`. A standalone `--version` (`-V`)
prints the helper name and its package version. Both modes exit successfully
without starting the protocol.

The Node bin wrapper handles the flags before loading `dist/cli.js`, which
the React bridge still invokes directly. The Rust MCP entry point checks
for the flags before starting the Tokio runtime. Other arguments go through
the original entry points unchanged.

## Checks

- `pnpm run build:runtime` and `pnpm --dir packages/cli test`: 138 tests
  passed, including help/version and both React response framings.
- `cargo build --locked -p celesta-mcp -p celesta-codec-test` and
  `cargo test --locked -p celesta-mcp --features runtime-tests`:
  108 tests passed, including help/version and MCP client tool calls.
- `cargo fmt --all -- --check` passed.
- `cargo clippy --locked -p celesta-mcp --all-targets --features runtime-tests -- -D warnings`
  reports the same unfulfilled `clippy::result_large_err` expectation in
  `crates/mcp/src/inspect.rs` on both the base and this branch. It passes on
  this branch with `-A unfulfilled-lint-expectations`.
