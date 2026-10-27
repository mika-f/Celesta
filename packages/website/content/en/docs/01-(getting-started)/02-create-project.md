Initialize a folder for your React composition, media, and npm dependencies. Celesta includes the runtime needed to preview and export the starter scene.

## From the app

Choose **File → Create New Project…**, click **Create New Project…** in the toolbar, or press `⌘ N` on macOS / `Ctrl N` on Windows and Linux. Select a project folder; the folder picker can create one. Celesta initializes that folder and opens `film.tsx`.

## From the command line

With `celesta-editor` on PATH, pass the folder to `--init`. Omit the folder to initialize the current directory:

```sh title="Initialize and open a project"
celesta-editor --init my-video
cd my-video
celesta-editor film.tsx
celesta-exporter --react film.tsx output.mp4
```

```sh title="Initialize the current directory"
celesta-editor --init
```

Desktop packages name the executables `Celesta` and `Celesta-export` (`.exe` on Windows). Use their installed paths in place of `celesta-editor` and `celesta-exporter`, including in generated scripts when needed:

::::tabs[Project initialization platform]{default="macOS"}
:::tab[macOS]

This command assumes the app is installed in `/Applications`. Adjust the path if needed.

```sh title="Initialize with the installed app"
"/Applications/Celesta.app/Contents/MacOS/Celesta" --init my-video
```

:::
:::tab[Windows]

Use PowerShell and replace `C:\path\to\Celesta` with your installed or extracted app folder. The installer does not add Celesta to PATH.

```sh title="Initialize with the installed app"
& "C:\path\to\Celesta\Celesta.exe" --init my-video
```

:::
::::

For a [source build](/en/docs/build-from-source/), build the React runtime first, then run `cargo run -p celesta-editor --release -- --init my-video` from the repository root.

## What initialization creates

- `film.tsx`: a five-second, 1920 × 1080 composition at 30 fps. The background and title sit in a `<Group>`, which is the parent the editor's Add menu inserts new layers into.
- `package.json`: a private project with preview, export, and typecheck scripts, and TypeScript `^7.0.2` as a development dependency.
- `tsconfig.json` and `.celesta/`: configuration and declarations matching Celesta’s bundled runtime.
- `.gitignore`: ignores `node_modules/` and `output.mp4`. The generated `.celesta/` directory ignores itself.

Preview and export work without installing packages. To use the project’s TypeScript compiler, install Node.js and pnpm separately, then run these commands in the project folder:

```sh title="Install the TypeScript compiler and check your project"
pnpm install
pnpm typecheck
```

> [!NOTE]
> **Initializing an existing folder**
> Re-running initialization refreshes `.celesta/` and creates missing starter files. Existing source files, `package.json` dependencies and scripts, `tsconfig.json`, and `.gitignore` are preserved. If an existing TypeScript configuration does not extend `./.celesta/tsconfig.json`, Celesta reports the change to make; merge it into your configuration.

## Add npm dependencies

Install external packages in the project that imports them. For a project outside the Celesta repository, run this in its folder:

```sh title="Add a project dependency"
pnpm add ag-psd
```

Preview and export resolve external imports from the importing file’s project `node_modules`. Celesta supplies every `@celesta/*` package and React from its runtime; you do not need to install them in an external project.

Node asset preparation scripts use the same dependencies. Save this as `prepare-assets.mjs` beside `package.json`, with `portrait.psd` in the same folder:

```tsx title="prepare-assets.mjs"
import { readFileSync, writeFileSync } from 'node:fs';
import { readPsd } from 'ag-psd';

const { width, height } = readPsd(
  readFileSync(new URL('./portrait.psd', import.meta.url)),
  { skipLayerImageData: true, skipCompositeImageData: true, skipThumbnail: true },
);
writeFileSync(
  new URL('./portrait-info.json', import.meta.url),
  JSON.stringify({ width, height }),
);
```

```sh title="Prepare assets"
node prepare-assets.mjs
```

Import the generated JSON in your composition as usual. Direct Node scripts use your installed Node.js; the app’s bundled Node.js evaluates compositions. For `.ts` scripts, use a Node.js version with TypeScript stripping support or a TypeScript runner installed in the project.

## Dependencies in repository examples

The React examples under `examples/` belong to the pnpm workspace. Run `pnpm install` from the repository root, then add dependencies to the example that needs them:

```sh title="Add a dependency to an example and preview it"
pnpm --dir examples/versus add ag-psd
cargo run -p celesta-editor --release -- examples/versus/film.tsx
cargo run -p celesta-exporter --release -- --react examples/versus/film.tsx output.mp4
```

Example manifests use `workspace:*` for local Celesta packages so Node tools can resolve their imports too. Complete the [source-build setup](/en/docs/build-from-source/) before running Cargo commands.

Continue with [your first React composition](/en/docs/react-compositions/), [preview controls](/en/docs/preview/), or [MP4 export](/en/docs/export/).
