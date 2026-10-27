# Setup: tools, new projects, dependencies, TypeScript

Where the Celesta executables are, how to use the Celesta MCP server, how to
start a new React project, how to use npm packages, how to type-check, and
how the user previews your work.

## Contents

- [Find the Celesta tools](#find-the-celesta-tools)
- [Use the Celesta MCP server](#use-the-celesta-mcp-server)
- [Create a React project](#create-a-react-project)
- [npm dependencies](#npm-dependencies)
- [Type-check](#type-check)
- [Preview in the app](#preview-in-the-app)

## Find the Celesta tools

| Install | App (preview, `--init`) | Exporter | Bundled Node.js | React runtime (`cli.js`) |
| --- | --- | --- | --- | --- |
| macOS app | `/Applications/Celesta.app/Contents/MacOS/Celesta` | `/Applications/Celesta.app/Contents/MacOS/Celesta-export` | `…/Contents/Helpers/node` | `…/Contents/Resources/react/dist/cli.js` |
| Windows installer | `%LOCALAPPDATA%\Programs\Celesta\Celesta.exe` | `…\Celesta-export.exe` | `…\runtime\node.exe` | `…\runtime\react\dist\cli.js` |
| Windows portable ZIP | `<extracted folder>\Celesta.exe` | `…\Celesta-export.exe` | `…\runtime\node.exe` | `…\runtime\react\dist\cli.js` |
| Source checkout | `cargo run -p celesta-editor --release --` | `cargo run -p celesta-exporter --release --` | `node` on PATH | `packages/cli/dist/cli.js` |
| Linux container image | — | `docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/work" celesta-exporter …` | inside the image | inside the image |

- The packaged apps name the executables `Celesta` and `Celesta-export`;
  source builds, docs, and generated `package.json` scripts call them
  `celesta-editor` and `celesta-exporter`. They take the same arguments.
  This skill writes `Celesta-export`; substitute the path you found.
- Installers do not add Celesta to PATH; quote the full path. In
  PowerShell, call it as `& "C:\…\Celesta-export.exe" …`.
- `--version` prints the version. An installed app that predates a feature
  in this skill (for example `--every`, `--contact-sheet`, `--no-ui`,
  `--init`) rejects the option; tell the user to update Celesta.
- A source checkout needs the React runtime built once, from the repository
  root: `pnpm install && pnpm --dir packages/react run codegen && pnpm run build:runtime`.
- The container image (`packaging/linux/Dockerfile` in the repository)
  renders on the CPU, includes the DejaVu and Noto CJK fonts, and mounts the
  project at `/work`.
- If nothing is found, ask the user where Celesta is installed.

## Use the Celesta MCP server

`Celesta-mcp` is an MCP server (stdio) next to `Celesta-export`. When its
tools are available to you, prefer them over the shell commands in this
skill: they return rendered frames as images, need no tool discovery, and
report progress for long exports.

| Install | Command to register |
| --- | --- |
| macOS app | `/Applications/Celesta.app/Contents/MacOS/Celesta-mcp` |
| Windows | `<install folder>\Celesta-mcp.exe` (next to `Celesta.exe`) |
| Source checkout | `target/release/celesta-mcp` after `cargo build -p celesta-mcp --release` |
| Linux container image | `docker run --rm -i -v "$PWD:/work" --entrypoint /opt/celesta/celesta-mcp celesta-exporter` |

For example, with Claude Code:

```sh
claude mcp add celesta -- /Applications/Celesta.app/Contents/MacOS/Celesta-mcp
```

| Shell command in this skill | MCP tool |
| --- | --- |
| `node inspect.mjs <entry> [--frames …] [--loop]` | `inspect` (`entry`, `frames`, `every`, `loop`) |
| `Celesta-export --json … --frame/--frames/--every … out.png` | `render_frames` (returns the images; `format`: `png`, `jpeg`, `webp`) |
| `Celesta-export --json … --every N --contact-sheet out.png` | `contact_sheet` (`output` extension or `format` picks the image format) |
| `Celesta-export --json … out.mp4` | `export_video` |
| `Celesta --init <folder> [--template <name>]` | `init_project` |
| `Celesta --list-templates` | `list_templates` |
| reading these references | `read_docs` (`topic`: `setup`, `animation`, …) |

Tool arguments are camelCase versions of the options (`propsFile`,
`debugGuides`, `quality`, `start`/`end` for `--from`/`--to`); a `source` ending in
`.json` is a JSON project, anything else a React entry. Failed calls return
the same error `code`s as `--json` ([export.md](export.md#json-results)).
`inspect.mjs --psd-layers` has no tool; run it in a shell.

## Create a React project

For a new video, initialize a folder instead of writing every file by hand:

```sh
/Applications/Celesta.app/Contents/MacOS/Celesta --init my-video   # macOS app
cargo run -p celesta-editor --release -- --init my-video            # source checkout
```

`--init` without a folder initializes the current directory. It prints
`Initialized Celesta project in …` and exits without opening a window. In
the GUI, the same thing is **File → Create New Project…** (Command-N /
Ctrl-N), which then opens `film.tsx`.

By default it creates the `blank` starter. To start from a ready-made video,
pick a template:

```sh
Celesta --list-templates                       # name, title, description
Celesta --init my-video --template slideshow
```

| Template | Contents |
| --- | --- |
| `blank` | The 5-second title card below. The default. |
| `slideshow` | Title card, then photos with captions and transitions, optional music (`slides.json`, `assets/`). |
| `ranking` | Ranking card from a data file, with theme and accent (`data.json`, `variants/`). |
| `dialogue` | Two characters with subtitles and lip sync; voices optional (`script.json`, `assets/`). |

A template other than `blank` adds `props.json` (values for the entry's
`defineProjectProperties()` inputs, loaded automatically by preview, export,
and `inspect.mjs`), a data file, sample media where it applies (`slideshow`, `dialogue`),
and a `README.md` saying what to replace. Change `props.json` and the data file rather than `film.tsx`; see
[project-data.md](project-data.md#pass-values-from-the-command-line). `--init`
prints the template's "edit" hint afterwards. An unknown name fails with the
available names. With `--template`, `--init` writes nothing and lists the
paths if any file the template would create already exists, so use a new or
empty folder. In the GUI, **File → Create New Project…** shows the same
choices first.

The files the `blank` starter creates, without overwriting anything that exists:

| File | Contents |
| --- | --- |
| `film.tsx` | A 5-second 1920×1080, 30 fps composition with a centered title inside a `<Group>`. The editor's Add menu inserts new layers into that group. Replace its body. |
| `package.json` | Private, with scripts `preview` (`celesta-editor film.tsx`), `export` (`celesta-exporter --react --overwrite film.tsx output.mp4`), `typecheck` (`tsc --project tsconfig.json`), and a `typescript` dev dependency. |
| `tsconfig.json` | Extends `./.celesta/tsconfig.json`. |
| `.gitignore` | `node_modules/` and `output.mp4`. |
| `.celesta/` | Type declarations for the Celesta packages in [SKILL.md](../SKILL.md#packages), React, and Node.js matching this Celesta build. Regenerated by every `--init`; ignores itself in Git. |

Re-running `--init` in an existing project refreshes `.celesta/` and adds
missing starter files only. If an existing `tsconfig.json` does not extend
`./.celesta/tsconfig.json`, it says so; add the `extends` yourself.

Preview and export need no `npm install`: Celesta supplies the runtime
packages. The `package.json` scripts call `celesta-editor`/`celesta-exporter`
by name; with a packaged app that is not on PATH, run the executables by
their full paths instead.

Keep media, fonts, and voices inside the project folder and reference them
with `./relative/paths` from the entry file.

The user can change an entry's properties without editing JSON: in the app,
the **Properties** section of the React Preview panel lists every property
declared with `defineProjectProperties()`. A change is saved to the props file
in use (`props.json`, or the `--props-file` the app was opened with) and the
preview reloads. A file chosen from outside the project is copied into
`assets/`. The panel writes the same props file you would edit, so edits from
you and from the panel stay in step: the panel re-reads the file before each
write and follows your changes. Keep the file valid JSON; the panel refuses to
write over a broken one.

## npm dependencies

- `react` and the Celesta packages in [SKILL.md](../SKILL.md#packages) always come from Celesta's
  runtime. **Never `npm install` them**, and never add them to `dependencies`.
- Any other npm package is installed in the project that imports it
  (`pnpm add ag-psd` or `npm install ag-psd` in the project folder). Preview
  and export resolve imports from the importing file's project
  `node_modules`, so no extra configuration is needed.
- Node scripts that prepare assets ahead of time (`node prepare-assets.mjs`
  that writes a JSON file the composition imports) run with your installed
  Node and the same project dependencies.
- Inside the Celesta repository, examples under `examples/<name>/` are pnpm
  workspace packages: `pnpm --dir examples/<name> add <package>`.

## Type-check

If the project has a `tsconfig.json` that extends
`./.celesta/tsconfig.json` (created by `--init`, **File → Set Up
TypeScript** with a React entry open, or **File → Set Up TypeScript in
Folder…** for a folder with no entry yet), type-check in that folder:

```sh
pnpm install && pnpm typecheck   # projects created with --init
npx tsc --noEmit -p .            # any project with the tsconfig
```

`.celesta/` contains the matching declarations, so nothing Celesta-specific
needs installing. Celesta refreshes `.celesta/` when a newer version opens
the project. Without a tsconfig, rely on `scripts/inspect.mjs` and PNG
frames ([verify.md](verify.md)).

Inside the Celesta repository, give a new `examples/<name>/` a
`tsconfig.json` that extends `./.celesta/tsconfig.json`, but do not run
`--init` there: it adds starter files the example does not need. Instead,
after `pnpm run build:runtime`, run `pnpm run typecheck:examples <name>` from
the repository root. It writes only `.celesta/` and type-checks the example;
CI checks every example with a `tsconfig.json` the same way.

## Preview in the app

The app previews and exports. It never edits your source; the only file it
writes is the props file, when the user changes a value in the Properties
section (see above). A file the user chooses from outside the project is also
copied into `assets/`.

- **File → Open…** (Command-O / Ctrl-O) opens a `.celesta.json` or a React
  entry (`.tsx`, `.jsx`, `.ts`, `.js`). Passing a path on the command line
  does the same: `Celesta film.tsx`.
- React entries reload automatically when saved. JSON projects need
  **File → Reload** (Command-R / Ctrl-R).
- Space plays and pauses; arrow keys step frames; I and O mark an export
  range; **Export…** writes an MP4.
- Track mute/solo in the app change what the preview (and an export started
  from the app) plays, and the volume slider is a monitor gain; neither is
  written to the files.
- Load errors and font warnings appear in the app; the user may paste them
  to you. Reproduce them yourself with `inspect.mjs` or a PNG export.
