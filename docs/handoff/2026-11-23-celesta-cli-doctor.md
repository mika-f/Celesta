# Headless render preflight

`celesta doctor [SOURCE OUTPUT] [--json]` checks Node.js and the selected
offscreen adapter. A native JSON project skips Node. With a source and output,
the command verifies a render through the same source, registry lock, output
resolver, encoders, muxer, and decoders as the exporter. It tests either one
still (`--frame`) or the video, optionally limited by `--from` and `--to`.
Probing a video without a range can take as long as an export; required media
may only be reached in later frames.

The probe writes to a temporary folder on the destination side and discards
the result. It checks existing destination files without replacing them
(`--overwrite` permits a probe when the destination already exists), and
does not create missing named destination directories.

JSON v1 is one line on stdout: `schemaVersion`, `status`, and `checks` with
`node`, `graphics` and `output` entries. Each has a status (`ok`, `error`
or `skipped`) and a human-readable message. Errors have a stable `code`,
reusing export codes for the render. Exit status is 0 for success, 1 for
failed checks, or 2 for invalid CLI arguments.

Validate with the CLI and exporter Rust tests and a headless lavapipe run of a
working render, a missing Node bundle, an unavailable graphics API, and a
missing media path. The CLI suite also checks a media program that launches
and fails, existing output preservation, and an unsatisfied project lock.
