# Build helper help and argument errors

## Spec

Both build helpers print usage and exit successfully on `--help` or `-h`.
Malformed helper arguments report the problem and point to `--help` without a
Node stack trace. The plugin build help lists the standard and x264 FFmpeg
environment variables and gives a separate Unix command for each build.

## Implementation

`scripts/plugins/build.mts` handles help and parse errors before reading plugin
files or starting build tools. `scripts/plugins/run-cargo.mts` handles its
standalone help switch and reports invalid wrapper arguments before spawning
Cargo. Arguments for Cargo, including its own help flag, stay with Cargo.
`scripts/run-cargo.mts` is an entry point for the same runner.

## Checks

Run `pnpm --dir scripts/plugins test` for TypeScript checking and Vitest,
including help and error output for both entry points, Cargo argument
forwarding, and the existing build and runner tests. The CI and release
invocations continue to use the same commands and flags.
