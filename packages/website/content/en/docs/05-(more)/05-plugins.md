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
3. In the desktop app, the open project's own plugin folders, once you trust it (see [Plugins that come with a project](#plugins-that-come-with-a-project)).
4. Your user plugin folder.
5. Folders you added in the desktop app's [Plugins tab](#the-plugins-tab).
6. The plugin folder that comes with Celesta: `plugins` beside the executable on Windows, `Contents/PlugIns` in the macOS app, and `../lib/celesta/plugins` from the executable's folder elsewhere.

A folder in 1, 2, 3, or 5 may be a plugin itself or hold several. Give `CELESTA_PLUGIN_PATH` absolute paths: a relative entry is skipped, since the folder it names would depend on where Celesta is started.

On macOS, Linux, and other Unix systems, Celesta does not use a folder in 3 to 6, or anything in a plugin from one, that other users can change (it is group- or other-writable, or belongs to neither root nor you): whoever can change a plugin's files can run code as the person who uses it. Remove the write permission with `chmod -R go-w`.

```sh
celesta-exporter --react film.tsx out.mp4 --plugin-dir ~/celesta-plugins
```

## Trust and approval

Opening a project runs its code on your computer, and a plugin runs its own. The desktop app therefore asks before either runs:

- **Projects.** The first time you open a project, the app asks whether you trust its folder, and evaluates nothing until you do. A project downloaded from the internet is marked as such in the question (Windows). Projects you create with **File > Create New Project** are trusted already. Trust covers that project's folder, but not a project in a folder below it that has its own `celesta.config.json`. It is remembered when the project root holds `celesta.config.json`; for a folder without one, it lasts until you quit the app, and the app asks again next time. Creating a project writes `celesta.config.json` for you.
- **Plugins.** A plugin in your user plugin folder, in a folder you added, or brought by a project runs only once you allow it. The app asks when it finds one, and asks again when its manifest or a file the manifest names (its program, module, or declarations) changes, such as after an update; other files in the plugin's folder are not checked. Until you allow a plugin, none of its code runs, and Problems says the plugin is waiting. A project that imports it still opens: the plugin's components draw nothing, so their layers are missing from the preview. Calling one of its other functions, such as in `prepare`, fails and says the plugin is waiting. Allowing it reloads the project with the plugin. Plugins named with `--plugin-dir` or `CELESTA_PLUGIN_PATH`, and the ones that come with Celesta, are not asked about.

Your choices are kept in `settings.json` in Celesta's settings folder: `%APPDATA%\Celesta` on Windows, `~/Library/Application Support/Celesta` on macOS, and `$XDG_CONFIG_HOME/celesta` (`~/.config/celesta` when unset) elsewhere.

`celesta-exporter` and the MCP server do not ask: they use every plugin in the folders above except a project's own, whether or not you allowed it in the app.

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

## The Plugins tab

The desktop app looks for plugins each time it opens a project. The **Plugins** tab in the right dock lists every plugin it found (of several with the same id, the one it uses), with:

- where the plugin came from (a project, your plugin folder, an added folder, `CELESTA_PLUGIN_PATH`, or the folder that comes with Celesta);
- its version, and whether it brings only components or a program, and whether that program was built for this computer;
- whether it is on, off, or waiting for your approval.

From the tab you can:

- allow a waiting plugin;
- turn any plugin off or back on (a plugin that is off does not run, in the app or in `celesta-exporter`);
- show its folder;
- open your plugin folder, creating it if needed;
- add more folders of plugins, which are searched after your plugin folder, and remove them again;
- reload: **Reload Plugins**, also in the **File** menu, picks up a plugin installed while the app is open.

What you change is kept in `settings.json` with your other choices, and the project reopens with the plugins as they now are. Plugins that were skipped, and why, are listed at the bottom of the tab and in the Problems tab.

A plugin that draws layers runs as a program of its own, which Celesta starts when a frame needs it; that program comes with the plugin, one for each platform it supports.

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

A plugin can tell you something about what a layer draws, such as a model's terms of use. The desktop app lists it in the Problems tab, and `celesta-exporter` prints it once, as a `warning:` or, when it is only information, as a `note:`.

Project TypeScript support copies each plugin's declarations into `.celesta/plugins/`, so your code editor completes its props. When the desktop app opens a project, that includes the project's own plugins and plugins still waiting for your approval: declarations run nothing.

In the desktop app, a plugin can also offer its components in the **Add** menu, after Celesta's own elements. Choosing one adds it like an Image or a Rect: a component that takes a file asks for it first, and you drag where it goes. The app writes the element with the plugin's import, such as `import { Flag } from '@celesta-plugin/flag'`.

## Samples

The repository's `examples/plugins` has three plugins to start from:

| Plugin | What it shows |
| --- | --- |
| `flag` | A layer drawn by a plugin program: an image waving like a flag, as textured meshes Celesta draws. |
| `turntable` | A layer drawn in 3D by a plugin program: a checkered cube turning on a turntable, with a see-through pane and a cut-out leaf, as a scene Celesta draws with depth. |
| `beep-voice` | A plugin with only components: a speech channel that voices `planDialogue()` lines in game-style beeps instead of a speech engine, with lip sync. |

```sh
node examples/plugins/build.mjs
cargo run -p celesta-exporter -- --react examples/plugins/flag/demo.tsx flag.mp4 --plugin-dir examples/plugins/dist
cargo run -p celesta-exporter -- --react examples/plugins/turntable/demo.tsx turntable.mp4 --plugin-dir examples/plugins/dist
```

## Write a plugin

A plugin has up to two parts, in any language:

- **Components** (`module` and `types` in the manifest): an ES module and its declarations. Celesta bundles the module into the entry, and its `react` and `@celesta/*` imports use the app's own. A speech channel needs nothing else: it runs in `prepare()` like `@celesta/voicevox` and returns lines with `audio` and `lipSync`.
- **A program** (`executable`, one per platform): it reads requests on standard input and answers on standard output. For a layer it returns, for a given time, either 2D textured triangle meshes with their colors, masks, and blending, or a 3D scene: skinned, morphing triangle geometry with materials, seen through a camera it places. Celesta draws them; scenes get depth, anti-aliasing, and opaque, cut-out, and see-through materials, for now unlit. When it opens a model, the program can also return notices, such as the model's terms of use, which Celesta shows as above. Rust programs can use the `celesta-plugin` crate; the protocol itself is plain length-prefixed JSON.

To offer components in the desktop app's Add menu, list them in the manifest's `components`:

```json
"components": [
  { "name": "Flag", "label": "Flag", "src": "image", "props": { "amplitude": 0.06 } }
]
```

`name` is the component the module exports: a capital letter, then letters, digits, or `_`. `label` is what the menu says (the name when absent). With `src` (`"image"` or `"video"`), the component is added like an Image: the app asks for a file and writes `src`, `x`, `y`, and `width`. Without it, the component is drawn as a box and gets `x`, `y`, `width`, and `height`. `props` are written after those: names made of letters, digits, and `_`, with strings, numbers, and booleans as values, and none of the props the drawing writes. A manifest with `components` needs a `module`.

A plugin's id uses lowercase letters, digits, `-`, `_`, and `.`, and starts with a letter or digit. Every file its manifest names is a relative path inside the plugin's folder, without `..` and without a link that leads out of it. Celesta skips a plugin that breaks these rules, or has a component that breaks the rules above, and says why.

The samples' READMEs walk through both parts. The repository's `docs/handoff/2026-11-01-plugin-system.md` describes the manifest and the protocol, the `docs/handoff/2026-11-02-plugin-*.md` notes describe what came later (such as `components`), and `docs/handoff/2026-11-03-plugin-scene-content.md` describes 3D scenes and notices.
