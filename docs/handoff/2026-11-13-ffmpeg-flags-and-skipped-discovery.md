# FFmpeg programs: unknown flags, and discovery failures that reach the user

Issue [#820](https://github.com/mika-f/Celesta/issues/820), from the review
of [PR #810](https://github.com/mika-f/Celesta/pull/810). It covers
`celesta-codec-ffmpeg`, `celesta-codec-ffmpeg-x264`, and how plugin-host
reports a recognized program beside Celesta that fails discovery.

## Specification

1. **Unknown arguments are a usage error.** Each program takes no argument
   (protocol mode, as Celesta starts it), or exactly one of `--manifest`,
   `--check-build`, `--check-license`. Anything else, including a second
   argument, prints `<program>: unknown arguments: …` and the usage on
   stderr, writes nothing on stdout, and exits 2. Before, an unknown flag
   (`--check-licence`, or `--check-build` given to an older program) fell
   through to `protocol::serve`, which returned at the empty stdin's EOF
   and exited 0, so a CI, Docker `RUN`, or script gate passed unchecked.
2. **A skipped sibling says why.** When a recognized program beside an
   installed Celesta fails `--manifest` (an x264 program linked against an
   FFmpeg without libx264, as `cargo install` against Fedora's
   `ffmpeg-free` gives), discovery keeps the failure as a
   `SkippedMedia { id, program, reason }`. The registry carries it, and a
   component of that plugin then reads as unavailable with
   `skipped at discovery (<program>): <reason>` instead of
   `not installed`, in resolve (CLI, MCP, the editor's export page, preset
   fallbacks) and `--describe`. A skipped program is reported only while no
   discovered plugin has the id it would have served (`ffmpeg` for
   `celesta-codec-ffmpeg`, `ffmpeg-x264` for `celesta-codec-ffmpeg-x264`).
   The stderr line `celesta: skipped the media plugin …` is still printed
   the first time.
3. **The stderr diagnostic is not lost.**
   - The stderr reader signals once a whole line that is not blank has been
     read (or the kept limit, or the end), so a `"\n"` written before the
     reason in a separate write no longer ends the wait early.
   - Once the program has exited, the diagnostic waits the full
     `STDERR_GRACE` (250 ms), not `min(STDERR_GRACE, deadline - now)`, so a
     program that exits near `MANIFEST_TIMEOUT` keeps its reason.
   - "its --manifest output is not a manifest (<serde error>)" now ends with
     the program's first stderr line as well.
4. **Refusals name the program that refused.** `no_component` formats with
   the profile's plugin id, so the x264 program says
   `no video encoder "vp9" in ffmpeg-x264`, not `… in ffmpeg`.
   `check_decoder` is left as it is: it runs only after `serves`, which only
   the standard profile passes for a decoder, and over the protocol the x264
   program has no decoder interface at all ("this plugin has no decoder").

## Implementation

- `plugins/ffmpeg/src/process.rs`: `run_plugin` returns an `ExitCode`;
  `Command::parse` maps the arguments (`args_os`, so a non-UTF-8 argument
  is a usage error instead of a panic) and `run` does the work as before.
  `CHECK_BUILD_FLAG` and `CHECK_LICENSE_FLAG` are public constants. Both
  `main.rs` files return `run_plugin`'s code.
- `plugins/ffmpeg/src/lib.rs`: `Profile::plugin_id`, `X264_PLUGIN_ID`
  (re-exported from plugin-host; `celesta-codec-ffmpeg-x264`'s `PLUGIN_ID`
  is it), and `PLUGIN_ID` is plugin-host's `FFMPEG_PLUGIN_ID`.
- `crates/plugin-host/src/media.rs`: `X264_PLUGIN_ID`, `SIBLING_PROGRAMS`,
  `SkippedMedia` (with `unavailable_reason`), `MediaDiscovery`, and
  `InstalledPlugins::media_discovery`; `media()` is its `plugins`. The
  sibling cache keeps `Result<MediaPlugin, String>`, so the reason survives
  later discoveries in a long-lived editor. `has_diagnostic_line` decides
  when the stderr reader signals.
- `crates/media/src/registry.rs`: `MediaRegistry::with_skipped` (drops one
  whose id a registered plugin has), `skipped(id)`, `skipped_programs()`.
- `crates/exporter`: `media_registry` attaches the skipped programs;
  `resolve::not_installed(registry, plugin)` gives the reason `named` and
  `describe_component` use. The report's libx264 hint already treats any
  reason other than `not installed` as "the installed ffmpeg-x264 plugin
  cannot encode here", which fits.
- Docs: plugins and troubleshooting guides (en/ja) and the Skill's
  `errors.md` and `plugins.md`.

## Checks

- `cargo test -p celesta-plugin-host -p celesta-media -p celesta-exporter`
  (new: `a_skipped_sibling_says_why`,
  `a_blank_first_line_does_not_hide_the_diagnostic`,
  `output_that_is_not_a_manifest_keeps_the_diagnostic`,
  `the_lines_a_diagnostic_is_taken_from`,
  `a_component_of_a_program_skipped_at_discovery_says_why`,
  `a_component_of_a_skipped_program_says_why`). The exporter's
  `the_test_codecs_program_writes_what_it_writes_in_process` needs
  `cargo build -p celesta-codec-test` first.
- In `plugins/`: `cargo test -p celesta-codec-ffmpeg` (new:
  `an_unknown_argument_is_a_usage_error`,
  `only_known_flags_alone_are_commands`, refusal messages in
  `process.rs`) and `cargo test -p celesta-codec-ffmpeg-x264` (the
  program's `vp9` refusal names `ffmpeg-x264`), against Homebrew
  `ffmpeg@8`.
- `cargo fmt --check` and `cargo clippy --all-targets` in both workspaces.
