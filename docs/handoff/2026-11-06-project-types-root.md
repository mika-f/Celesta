# TypeScript support stays inside the project root (2026-11-06)

File → Set Up TypeScript and the refresh on opening a project chose the
folder for `.celesta/` and `tsconfig.json` by walking up from the entry:
setup stopped at the nearest `tsconfig.json` or `package.json`, refresh at
the nearest `.celesta/version.json`. A stray `package.json` in a home folder
made the home folder the root, and every project below it without a
`.celesta/` of its own then refreshed the home folder's, plugin declarations
included (mika-f/Celesta#654). This is option 1 of that issue: one root rule,
the one the plugin trust check already uses.

## Specification

- The root is `celesta_plugin_host::project_root(entry)`: the nearest folder
  from the entry's own up that holds `celesta.config.json`, else the entry's
  folder. `package.json` and `tsconfig.json` do not count.
- `set_up_project_types(template, entry)` (File → Set Up TypeScript) installs
  into that root.
- `refresh_project_types(template, entry, plugins)` (every project open)
  looks only at that root's `.celesta/version.json`. A `.celesta/` in a
  folder above it is never touched.
- `set_up_project_folder(template, root)` (File → Set Up TypeScript in
  Folder…) installs into the chosen folder and writes an empty
  `celesta.config.json` there when it has none
  (`celesta_plugin_host::mark_project_root`). Without the mark, an entry in
  a subfolder of the chosen folder would have its own folder as the root,
  and the types set up there would not be refreshed.
- `set_up_project_types_in` (used by `initialize_project`: `--init`, Create
  New Project, MCP `init_project`) is unchanged: it writes into the given
  folder, and `initialize_project` already writes `celesta.config.json`, now
  through `mark_project_root`. It does not mark the folder itself, because a
  template may bring its own `celesta.config.json`, which the conflict check
  and copy would then trip over.

## Decisions

- **Option 2 (asking for the root in Set Up TypeScript) is not added.**
  Only projects with the entry in a subfolder and no `celesta.config.json`
  behave differently, and Set Up TypeScript in Folder… on the project
  folder already is that question, answered once: it writes the mark. The
  guide and troubleshooting page say so. If it turns out people miss it,
  Set Up TypeScript can offer it when it finds a `.celesta/` above the root.
- **The rule stays in `celesta-plugin-host`.** `celesta-react-bridge`
  already depends on it, and `celesta-project` is about `.celesta.json`
  timelines, not project folders. The doc comment on `project_root` now
  names TypeScript support too. Moving it to a crate of its own can wait
  until something outside plugins and types needs it.
- **Migration.** A project that had its `.celesta/` at the root and the entry
  in `src/` keeps working with tsc and editors (they read the nearest
  `tsconfig.json`), but Celesta stops refreshing that `.celesta/` until the
  root gets a `celesta.config.json`. Running Set Up TypeScript in Folder… on
  the root adds it and refreshes. Projects from `--init` and every example
  have the entry at the root, so they are not affected.
- VOICEVOX's `prepareNarration()` cache (`.celesta/voices` beside the entry)
  is unaffected: refresh still needs `version.json`, and `install()` still
  leaves `voices` alone.

## Tests

- The react-bridge tests that set up an entry without a mark use
  `Scratch::unmarked`, which skips (with a note on stderr) when a folder
  above the temp folder holds `celesta.config.json`. In that case the
  project root would be that folder, outside the scratch folder, and the
  test would write there. Tests whose project holds its own mark always
  run.
- New: setup ignores a `package.json` and `tsconfig.json` above the entry;
  setup uses the folder with `celesta.config.json` for an entry in `src/`;
  refresh leaves a `.celesta/` above the project root alone (the case in the
  issue); the folder variant marks the root and a later `src/main.tsx`
  refreshes it; `mark_project_root` keeps an existing file.

## Checks

- `cargo test -p celesta-plugin-host --lib project`
- `cargo test -p celesta-react-bridge --lib project_`
- `cargo clippy -p celesta-plugin-host -p celesta-react-bridge -p celesta-editor --all-targets -- -D warnings`
- `cargo fmt --all --check`
