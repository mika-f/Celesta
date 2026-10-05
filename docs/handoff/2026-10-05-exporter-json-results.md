# Exporter `--json` results (2026-10-05)

`celesta-exporter --json` replaces all progress output with one line of JSON
on stdout when the export ends (`crates/exporter/src/report.rs`); stderr stays
empty. It is meant for scripts and AI agents, which otherwise had to guess
PNG file names, parse progress lines, and pattern-match error strings.

- The report has `status`, `source`, `composition` (`width`, `height`, `fps`,
  `frames`, `duration`), `outputs`, `warnings`, `stats` (`elapsedSeconds`,
  `renderFps`), and on failure `error` (`code`, `message`, optional `hint`,
  and `issues` with each project validation `{path, message}`). Paths are
  absolute; times are seconds rounded to milliseconds. Field names are
  camelCase. Exit status: 0 ok, 1 export failed, 2 usage (including clap
  parse errors when `--json` is among the arguments). `composition` is
  omitted when the export failed before the composition was known, and a
  usage-error report has only `status` and `error`.
- The library reports what the CLI prints through two `ExportProgress`
  events: `Composition(CompositionInfo)` once the composition is known (before
  the range or frame selection is checked, so out-of-range errors still carry
  the frame count) and `Wrote(ExportedFile)` after each file is published
  (`Video` with `first_frame`/`frames`/`audio`, `Frame`, `ContactSheet` with
  its tile frames and effective columns). The editor ignores both.
- PNG selection and contact-sheet layout errors are now
  `ExportError::InvalidSelection` instead of `Io` with `InvalidInput`; the
  message text is unchanged.
- `progress::run` and the CLI's export function now carry `ExportError`
  rather than `String`, so a companion project that fails to load reads
  "could not load export project: …" like the JSON-project path already did.
- The error codes are listed in
  `skills/celesta/references/export.md#json-results`; keep that
  table in step with `report::error_report`.
