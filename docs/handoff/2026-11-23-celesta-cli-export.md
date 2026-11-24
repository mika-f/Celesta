# Unified export and info commands

`celesta export` takes the existing exporter flags, including component
queries, progress, and machine-readable results. `celesta info` takes the
source and property/plugin options and reports metadata without rendering,
like the exporter's existing `--info`.

The exporter crate now owns the reusable command frontend. `celesta-exporter`
is a wrapper for that frontend with its original invocation, name and exit
behavior. The Linux image's entry point and binary paths do not change.

Validation: `cargo test --locked -p celesta-cli -p celesta-exporter --features runtime-tests`
with the staged runtime and a Vulkan adapter; run both info forms and both
export entry points headlessly with matching options.
