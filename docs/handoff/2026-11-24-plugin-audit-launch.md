# Plugin audit launch failures

On Ubuntu 22.04, the registry's `ffmpeg-x264` 0.10.0 installed successfully
but its bundled `libx264.so.164` required `GLIBC_2.38`. Both audit formats
reported no problems because they only checked listings and advisories.

Audit now tries the existing, time-bounded plugin protocol handshake on
installed executables in the registry store, using the project lock or
global install's tree hash to verify the build. It does not run blocked or
disabled builds, missing or changed builds, or plugins without a program for
this platform. It checks startup, not individual encoders or render output.

Startup errors (including a bounded stderr excerpt when available) and a
remediation hint are shown in text and JSON. The JSON report keeps
`schemaVersion: 1` and `error.code: "audit_failed"`. The new finding is
`plugin_launch_failed`, with per-plugin status `launch_failed` unless a
block or withdrawal takes precedence. A launch finding causes exit 1.
The `launchChecked` flag distinguishes a checked build from a skipped one.

## Checks

- `cargo fmt --all -- --check`
- `cargo clippy --locked -p celesta-cli -p celesta-plugin-host --all-targets -- -D warnings`
- `cargo test --locked -p celesta-cli -p celesta-plugin-host` includes a CLI
  regression test for both audit formats, the glibc hint, and skipping an
  unverified or advisory-blocked executable.
- On Ubuntu 22.04.5 LTS (glibc 2.35), install
  `celesta plugin install --global ffmpeg-x264@0.10.0` from the official
  registry. Run the before and after CLI binaries against that same install
  in an `ubuntu:22.04` container. On main, `audit` printed
  `no problems found` and `audit --json` returned `"status":"ok"` and
  `"findings":[]` (both exit 0). The changed CLI printed
  `GLIBC_2.38' not found` with a hint to install a distro-compatible build
  or use a newer distro. JSON returned `"status":"error"` and
  `"code":"plugin_launch_failed"` in the finding (both exit 1).
