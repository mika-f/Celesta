# Plugin CLI JSON output

`celesta plugin list`, `install` (including `--locked`, `--offline`
and `--global`), and `audit` accept `--json` after the subcommand.
Without the flag the commands retain their text output. Warnings still
go to stderr; successful or failed JSON invocations write a single
report to stdout. Help still prints text. Exit codes are 0 for success,
1 for failure, and 2 for usage errors.

## Report contract

Reports have `schemaVersion: 1`, `command` (omitted if parsing failed),
`status` (`ok` or `error`), and a `plugins` array. Each plugin row
includes `scope` (`project` or `global`), `id`, `version`, and
`status`. A plugin that appears in both scopes has a row per scope.

- List reports `installed`, `missing`, `no_build`, or `blocked` with
  `yanked` and `deprecated` flags and the blocking `advisory`, if any.
- Project install reports the host `target` and a `lockfile` outcome:
  `written`, `satisfied`, or `unchanged`. A successful row is
  `installed`, `already_installed` (the store or bundled plugins held
  the version), or `skipped` with reason `no_build`. Global install
  reports the chosen version as `installed`. Install is fail-fast;
  a failure identifies the affected version when the operation reports it,
  not every plugin processed before the failure.
- Audit reports `ok`, `warning`, `skipped` (index unavailable),
  `blocked`, or `withdrawn`. Each row includes `listingChecked` and
  `findings`. Findings have `code`, `message`, `fails`, and a structured
  `advisory` when blocked. Their codes are `plugin_blocked`,
  `plugin_withdrawn`, `plugin_yanked`, and `plugin_deprecated`.
- List and audit report whether a verified advisory list was available in
  `advisoriesAvailable`. This includes cached lists and does not imply
  freshness.

Failures include an `error` object. Use its stable `code`, not its
human-readable `message`:

| Stage | Codes |
| --- | --- |
| Usage or setup | `invalid_arguments`, `invalid_plugin_spec`, `registry_selection`, `user_folder`, `cache_folder`, `current_directory` |
| Settings or lock | `project_config`, `settings`, `lockfile`, `lock_missing`, `lock_out_of_date`, `lock_incompatible`, `lock_unsatisfied`, `lock_hash_mismatch` |
| Registry or install | `registry`, `plugin_not_found`, `resolution`, `install_failed`, `plugin_missing`, `plugin_withdrawn`, `plugin_blocked` |
| Audit findings that fail the check | `audit_failed` (see the individual findings) |

The CLI derives codes from typed errors and builds the report from the
same state used for text output. It does not parse human-readable text
or change the registry client's install/audit rules. Other plugin
commands retain their text interface.

## Sample output

These runs used a fresh Linux home and a temp project copied from
`examples/font-stack`, with the registry unreachable. Stderr advisory and
index warnings are omitted below.

```text
$ celesta plugin list
This project
  code 0.10.0-alpha.4  not installed
Global
  none
$ celesta plugin list --json
{"advisoriesAvailable":false,"command":"list","plugins":[{"deprecated":false,"id":"code","scope":"project","status":"missing","version":"0.10.0-alpha.4","yanked":false}],"schemaVersion":1,"status":"ok","target":"linux-x86_64"}

$ celesta plugin install --offline
celesta: error: code 0.10.0-alpha.4 is not installed, and this run is offline
$ celesta plugin install --offline --json
{"command":"install","error":{"code":"plugin_missing","message":"code 0.10.0-alpha.4 is not installed, and this run is offline"},"plugins":[{"id":"code","scope":"project","status":"missing","version":"0.10.0-alpha.4"}],"schemaVersion":1,"scope":"project","status":"error"}

$ celesta plugin audit
no problems found in what could be checked
$ celesta plugin audit --json
{"advisoriesAvailable":false,"command":"audit","plugins":[{"findings":[],"id":"code","listingChecked":false,"scope":"project","status":"skipped","version":"0.10.0-alpha.4"}],"schemaVersion":1,"status":"ok"}
```

Both modes exited 0 for list and audit, and 1 for install.

## Checks

- `cargo fmt --all -- --check`
- `cargo clippy --locked -p celesta-cli --all-targets -- -D warnings`
- `cargo test --locked -p celesta-cli` (57 tests passed, including CLI subprocess tests)
