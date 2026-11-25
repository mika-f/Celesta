# `celesta init` projects use the unified CLI and a bundled font

Projects from `celesta init` had an `export` script that called
`celesta-exporter`, so a headless Linux machine driving Celesta through
`celesta` alone still needed a second binary for `pnpm export`. The blank
template's title used `fontFamily: 'sans-serif'`; the renderer matches family
names literally and has no CSS generic families, so every export warned
`font family "sans-serif" (weight 400) is not installed or loaded`, even with
fonts such as DejaVu installed.

## Changes

- The generated `package.json` `export` script is
  `celesta export --react --overwrite film.tsx output.mov`. `preview` stays
  `celesta-editor film.tsx`, since the editor is a separate desktop
  executable and the unified CLI has no editor command.
- `templates/blank/fonts/` ships Inter 4.1 Regular (`Inter-Regular.woff2`,
  111 KB, from the rsms/inter release's `web/` folder) with its SIL OFL 1.1
  license as `OFL.txt`, and `inter.css`, a one-rule stylesheet declaring the
  file as the `Inter` family. `film.tsx` loads the stylesheet with `<Font>`
  inside `<Assets>` (outside the `<Group>` the editor's Add menu inserts
  into) and the title
  uses `fontFamily: 'Inter'`. The title renders the same on every machine
  and needs no installed fonts and no network. Loading the WOFF2 directly
  is not enough: its faces join the installed ones under the name `Inter`,
  and with Ubuntu's `fonts-inter` (Inter 3.19) installed the export drew the
  installed face. A stylesheet family hides an installed family of the same
  name, so the bundled file always draws.
- The user guide (en/ja Create a project) and the Agent Skill's setup
  reference describe the new script and the `fonts/` folder.

## Checks

- `initialize_without_template_keeps_todays_behaviour` asserts both scripts.
- `crates/cli/tests/export.rs` (`runtime-tests`) runs `celesta init` and
  exports frame 0 of the new project, asserting `"warnings":[]`.
- The `templates/` staging test asserts the staged blank template ships the
  font, `inter.css`, and `OFL.txt`.
- `cargo test --release --locked -p celesta-react-bridge --lib project_templates`.
- `templates/` Vitest suite after `pnpm run build:runtime`.
- In a clean `ubuntu:22.04` container with no fonts, no display and no GPU
  (Mesa lavapipe only), `celesta init` followed by `celesta export` of a PNG
  frame and of the project's `export` script command reports no warnings;
  the MOV is 150 ProRes frames at 1920x1080. With `fonts-dejavu-core`
  installed, the previous `sans-serif` template still warns
  `font family "sans-serif" (weight 400) is not installed or loaded` and the
  new template reports `"warnings":[]`. With no fonts installed at all, the
  previous template panics with `no default font found`. With `fonts-inter`
  3.19 installed, the new template's frame is byte-identical to the one
  rendered with no fonts installed.
