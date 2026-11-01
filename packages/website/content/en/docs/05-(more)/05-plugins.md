Content plugins add what Celesta does not ship itself: a kind of layer drawn from code of its own, or a component library such as a speech channel. A plugin is a folder you put on your computer; compositions import its components as `@celesta-plugin/<id>`.

Plugins are new and still change. They work in the desktop app's preview and in `celesta-exporter`, not in `@celesta/web` or the CPU reference renderer.

## Install a plugin

A plugin is a folder with a `celesta-plugin.json`. The simplest place for one is your user plugin folder:

| OS | User plugin folder |
| --- | --- |
| Windows | `%APPDATA%\Celesta\plugins` |
| macOS | `~/Library/Application Support/Celesta/plugins` |
| Linux and others | `$XDG_DATA_HOME/celesta/plugins` (`~/.local/share/celesta/plugins` when unset) |

Celesta looks for plugins in this order, and of several plugins with the same id uses the first it finds:

1. Each `--plugin-dir` of the exporter.
2. Each folder of the `CELESTA_PLUGIN_PATH` environment variable (separated by `;` on Windows and `:` elsewhere).
3. Your user plugin folder.
4. The plugin folder that comes with Celesta: `plugins` beside the executable on Windows, `Contents/PlugIns` in the macOS app, and `../lib/celesta/plugins` from the executable's folder elsewhere.

A folder in 1 or 2 may be a plugin itself or hold several. Give `CELESTA_PLUGIN_PATH` absolute paths: a relative entry is skipped, since the folder it names would depend on where Celesta is started.

On macOS and Linux, Celesta does not use a folder or plugin in 3 or 4 that other users can change (it is group- or other-writable, or belongs to neither root nor you): whoever can change a plugin's files can run code as the person who uses it. Remove the write permission with `chmod go-w`.

```sh
celesta-exporter --react film.tsx out.mp4 --plugin-dir ~/celesta-plugins
```

## Trust and approval

Opening a project runs its code on your computer, and a plugin runs its own. The desktop app therefore asks before either runs:

- **Projects.** The first time you open a project, the app asks whether you trust its folder, and evaluates nothing until you do. A project downloaded from the internet is marked as such in the question (Windows). Projects you create with **File > Create New Project** are trusted already. Trust covers the folder and everything in it, and is remembered.
- **Plugins.** A plugin in your user plugin folder or brought by a project runs only once you allow it. The app asks when it finds one, and asks again when the plugin's files change, such as after an update. Until you allow a plugin, its layers are not drawn and Problems says it is waiting. Plugins named with `--plugin-dir` or `CELESTA_PLUGIN_PATH`, and the ones that come with Celesta, are not asked about.

Your choices are kept in `settings.json` in Celesta's settings folder: `%APPDATA%\Celesta` on Windows, `~/Library/Application Support/Celesta` on macOS, and `$XDG_CONFIG_HOME/celesta` (`~/.config/celesta` when unset) elsewhere.

`celesta-exporter` and the MCP server run what you point them at and do not ask.

### Plugins that come with a project

A project can bring plugins in its own folders, listed in `celesta.config.json` at the project's root:

```json
{
  "plugins": {
    "paths": ["./plugins"]
  }
}
```

Each path is relative to the project root and must stay inside it. The desktop app searches these folders after `CELESTA_PLUGIN_PATH` and before your user plugin folder, once you trust the project. The folder holding `celesta.config.json` is also the project root the trust question names; without the file, the root is the entry's own folder. `celesta-exporter` does not read the file: pass the folders with `--plugin-dir`.

## Reloading plugins

The desktop app looks for plugins each time it opens a project; to pick up a plugin installed while it is open, choose **File > Reload Plugins**. Plugins it skipped, and why, are listed in the Problems tab. A plugin that draws layers runs as a program of its own, which Celesta starts when a frame needs it; that program comes with the plugin, one for each platform it supports.

## Use its components

```tsx
import { Composition } from '@celesta/react';
import { Flag } from '@celesta-plugin/flag';

export default function Root() {
  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={150}>
      <Flag src="./portrait.png" x={80} y={160} amplitude={0.1} />
    </Composition>
  );
}
```

An entry that imports a plugin no folder provides fails to load and names the plugin. A frame stays a function of its time: a plugin's layer depends only on its props and the frame, so preview, export, and scrubbing agree.

Project TypeScript support copies each installed plugin's declarations into `.celesta/plugins/`, so the editor completes its props.

## Samples

The repository's `examples/plugins` has two plugins to start from:

| Plugin | What it shows |
| --- | --- |
| `flag` | A layer drawn by a plugin program: an image waving like a flag, as textured meshes Celesta draws. |
| `beep-voice` | A plugin with only components: a speech channel that voices `planDialogue()` lines in game-style beeps instead of a speech engine, with lip sync. |

```sh
node examples/plugins/build.mjs
cargo run -p celesta-exporter -- --react examples/plugins/flag/demo.tsx flag.mp4 --plugin-dir examples/plugins/dist
```

## Write a plugin

A plugin has up to two parts, in any language:

- **Components** (`module` and `types` in the manifest): an ES module and its declarations. Celesta bundles the module into the entry, and its `react` and `@celesta/*` imports use the app's own. A speech channel needs nothing else: it runs in `prepare()` like `@celesta/voicevox` and returns lines with `audio` and `lipSync`.
- **A program** (`executable`, one per platform): it reads requests on standard input and answers on standard output. For a layer it returns textured triangle meshes and their colors, masks, and blending for a given time, and Celesta draws them. Rust programs can use the `celesta-plugin` crate; the protocol itself is plain length-prefixed JSON.

A plugin's id uses lowercase letters, digits, `-`, `_`, and `.`, and starts with a letter or digit. Every file its manifest names is a relative path inside the plugin's folder, without `..` and without a link that leads out of it. Celesta skips a plugin that breaks these rules and says why.

The samples' READMEs walk through both parts, and the repository's `docs/handoff/2026-11-01-plugin-system.md` describes the manifest and the protocol in full.
