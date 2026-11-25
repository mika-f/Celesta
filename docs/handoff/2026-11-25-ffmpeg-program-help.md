# FFmpeg programs: `--help` and `--version`

Found in an end-to-end run of the headless Linux operator flow on a clean
ubuntu:22.04 container (transcript in
[PR #908](https://github.com/mika-f/Celesta/pull/908)): an operator who runs
`celesta-codec-ffmpeg --help` to find out what the binary is got
`unknown arguments: --help` and exit 2.

## Specification

- `celesta-codec-ffmpeg` and `celesta-codec-ffmpeg-x264` take `--help` / `-h`
  and `--version` / `-V`, alone, besides `--manifest`, `--check-build`, and
  `--check-license`.
- `--help` prints to stdout and exits 0: the program and version, what the
  plugin serves, that Celesta starts it as a subprocess (with its plugin id)
  and it is not run by hand, then `celesta doctor`,
  `celesta export --list-components --check`, and
  `celesta export <project> <output>` as what to run instead, and the flags.
- `--version` prints `<program> <version>` to stdout and exits 0.
- Both answer before any FFmpeg work: no component check, so an x264 program
  linked without libx264 still prints its help.
- Protocol mode (no argument) and the other flags are unchanged. Any other
  argument, or two arguments (`--help --version` included), is still the usage
  error (stderr, exit 2); the usage now lists `--help | --version` and points
  to `--help`.

## Implementation

- `plugins/ffmpeg/src/process.rs`: `Command::Help` and `Command::Version`,
  answered in `run_plugin` before `run`. The help text picks the plugin
  description and id from the codec's `Profile`.
- User guide (en, ja) `05-plugins.md`: the flag list and what `--help` and
  `--version` print, scoped to the two FFmpeg programs (the AVFoundation and
  Media Foundation programs take no arguments).
- Celesta Agent Skill `skills/celesta/references/plugins.md`: the same flags
  beside the `cargo install` discovery notes.

## Checks

- `cargo test -p celesta-codec-ffmpeg --lib process` (parsing) and
  `--test manifest` for both packages (`--help`, `-h`, `--version`, `-V`
  through the built programs, and the updated usage error).
