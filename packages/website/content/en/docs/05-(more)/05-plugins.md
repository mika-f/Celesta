Content plugins add what Celesta does not ship itself: a kind of layer drawn from code of its own, or a component library such as a speech channel. A plugin is a folder you put on your computer; compositions import its components as `@celesta-plugin/<id>`.

Plugins are new and still change. They work in the desktop app's preview and in `celesta-exporter`, not in `@celesta.studio/web` or the CPU reference renderer.

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
6. The plugin folder that comes with Celesta: `plugins` beside the executable on Windows, `Contents/Resources/PlugIns` in the macOS app, and `../lib/celesta/plugins` from the executable's folder elsewhere.

A plugin says which versions of Celesta it works with. Celesta skips a plugin made for other versions, or one that does not say, and tells you why in the Problems tab, the [Plugins tab](#the-plugins-tab), and the output of `celesta-exporter` and the MCP server; get a release of the plugin made for your Celesta. A skipped plugin does not hide another one with the same id found later in this order, so a media plugin made for your Celesta further down the list is used instead. Neither does a plugin you turned off in the [Plugins tab](#the-plugins-tab).

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

`celesta-exporter` and the MCP server do not ask, but they follow the choices you made in the app. They search the folders above except a project's own, and skip a plugin you turned off. A plugin from your user plugin folder or a folder you added runs only once you allowed it in the app as it is now; otherwise they skip it and say it is waiting. An entry that imports a waiting plugin fails to load, with an error that names the plugin and how to allow it, so a layer is never left out of an export without your knowing. To use such a plugin without the app:

- pass `--allow-plugin <id>` to `celesta-exporter` (repeatable). It runs that plugin as it is now, for that export only, and saves nothing. A plugin you turned off in the app stays off.
- or name its folder with `--plugin-dir` or in `CELESTA_PLUGIN_PATH`, which are never asked about. This is how to give the MCP server a plugin you have not allowed in the app.

```sh
celesta-exporter --react film.tsx out.mp4 --allow-plugin flag
```

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

The desktop app looks for plugins each time it opens a project. The **Plugins** tab in the right dock lists every plugin it found (of several with the same id, the one it uses, and any before it that you turned off), with:

- where the plugin came from (a project, your plugin folder, an added folder, `CELESTA_PLUGIN_PATH`, or the folder that comes with Celesta);
- its version, and whether it brings only components or a program, and whether that program was built for this computer;
- whether it is on, off, or waiting for your approval. A waiting plugin's switch is greyed out and off until you allow it.

From the tab you can:

- allow a waiting plugin, or turn it off with **Turn Off**;
- turn any plugin off or back on with its switch. A plugin that is off does not run, in the app or in `celesta-exporter`, and the next plugin with its id in the search order is used in its place, if there is one: turning off a plugin in your user plugin folder goes back to the one that comes with Celesta, for example. A plugin waiting for approval does not give way like this, so a plugin you have not allowed cannot be replaced by one further down the list without you noticing;
- show its folder;
- open your plugin folder, creating it if needed;
- add more folders of plugins, which are searched after your plugin folder, and remove them again;
- reload: **Reload Plugins**, also in the **File** menu, picks up a plugin installed while the app is open.

What you change is kept in `settings.json` with your other choices, and the project reopens with the plugins as they now are. Plugins that were skipped, and why, are listed at the bottom of the tab and in the Problems tab.

A plugin that draws layers runs as a program of its own, which Celesta starts when a frame needs it; that program comes with the plugin, one for each platform it supports.

## Use its components

```tsx
import { Composition } from '@celesta.studio/react';
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

When a plugin's program does not start, or cannot open a layer's model, the preview shows the error in the Problems tab and does not ask the plugin again for every frame. After you fix the plugin, use **Reload Plugins**; after you fix a file the layer draws from, such as `<Flag>`'s `src`, the next frame tries again with the new file.

A plugin can tell you something about what a layer draws, such as a model's terms of use. The desktop app lists it in the Problems tab while the preview shows that layer, and again in the export's entry there once an export that drew it ends; `celesta-exporter` prints it once, as a `warning:` or, when it is only information, as a `note:`.

Project TypeScript support copies each plugin's declarations into `.celesta/plugins/`, so your code editor completes its props. When the desktop app opens a project, that includes the project's own plugins and plugins still waiting for your approval: declarations run nothing.

In the desktop app, a plugin can also offer its components in the **Add** menu, after Celesta's own elements. Choosing one adds it like an Image or a Rect: a component that takes a file asks for it first, and you drag where it goes. A component that takes a model file, such as a VRM avatar, accepts only the file types it lists (`Choose a .vrm file`) and draws its box at its layer's shape. The app writes the element with the plugin's import, such as `import { Flag } from '@celesta-plugin/flag'`.

## Media plugins

Celesta's executables link no FFmpeg. Of the two FFmpeg builds, app packages
and the Linux image bundle only standard `ffmpeg` (LGPL): decoding, ProRes,
VP9, AAC, Opus, PCM and MP4/MOV/WebM muxing. An optional download adds
`ffmpeg-x264/libx264` (GPL H.264). The base package can decode H.264 and
export ProRes/MOV and WebM (VP9 with alpha, Opus) without addons.

macOS also bundles `avfoundation`, and Windows `mediafoundation`, using their
system media frameworks under MIT/Apache-2.0. Their H.264 encoders outrank a
user-installed x264 download and provide default MP4 video when available;
the standard FFmpeg plugin supplies the muxer and AAC. On Linux, MP4 needs
`ffmpeg-x264`. Explicitly name `ffmpeg-x264/libx264` to choose it on any OS.
See the [export guide](/en/docs/export/).

Only `ffmpeg/libx264` moved: presets and commands that name it must use
`ffmpeg-x264/libx264`, which has no alias. `ffmpeg/opus` and `ffmpeg/vp9` keep
the IDs they had in earlier releases. Local builds may link a system FFmpeg
8.1.x, including GPL. Distribution builds check the license explicitly; the
optional program also rejects a build missing libx264. See [Build from source](/en/docs/build-from-source/).

### Optional codec downloads

Open the [release downloads](celesta:download/all) for your Celesta version and
choose the ZIP matching your OS and CPU architecture:

| Download | Adds | Needed for |
| --- | --- | --- |
| `ffmpeg-x264` | `ffmpeg-x264/libx264` (GPL) | libx264 H.264; required for MP4 on Linux |

Files are named `Celesta-plugin-ffmpeg-x264-<version>-<os>-<arch>.zip`
(for example `Celesta-plugin-ffmpeg-x264-0.9.0-macos-arm64.zip`). Use the same
release as the app. The ZIP contains one `ffmpeg-x264` folder with its
manifest, executable, native libraries and license notices; no separate FFmpeg
installation is needed.

Extract it and copy the entire folder into your user plugin folder listed
above. Keep all files together. Open Celesta, choose **Reload Plugins** and
allow the added plugin when prompted. For command-line use, allow it in the
app first, pass `--allow-plugin ffmpeg-x264`, or pass its parent folder with
`--plugin-dir`.

Linux MP4 needs x264. macOS/Windows can use bundled native H.264. WebM needs
no download. Without an H.264 encoder, choose **QuickTime · ffmpeg** for
ProRes/MOV or **WebM · ffmpeg** for VP9, or export still images. Media decoding stays in the standard plugin. Browser
MP4 export uses WebCodecs and does not use these native plugins.

For other codecs and containers, the standard `ffmpeg` components take priority;
native components are used when explicitly named. H.264 is the exception:
AVFoundation and Media Foundation provide the default MP4 video when available.
Their decoders read files that FFmpeg cannot (see
[AVFoundation components on macOS](/en/docs/export/#avfoundation-components-on-macos)
and [Media Foundation components on Windows](/en/docs/export/#media-foundation-components-on-windows)).

A media plugin's manifest has `"roles": ["media"]` and `"protocol": 2`, and lists the components the plugin offers under `media`, each with an `id` and a `name` to show:

- `decoders` read files. Each lists the file types it reads in `extensions` (without the dot).
- `videoEncoders` and `audioEncoders` compress pictures and sound. Each lists the codecs it writes in `codecs`, as the part of a WebCodecs codec string before its first dot (`avc1` for H.264, `mp4a` for AAC), and a video encoder may list the pixel formats it takes in `inputFormats`.
- `muxers` write files. Each lists the `extensions` it writes, the codecs it takes in `video` and `audio` (a muxer without `audio` writes no sound), the codecs it writes by default in `defaultVideo` and `defaultAudio`, and `"trimsLeadingAudio": true` when it can hide sound placed before the start, which `--loop` uses.
- `priority` is optional: a whole number, `0` when absent. Among plugins from the same place (the ones you installed, or the ones that come with Celesta), a plugin with a higher `priority` has its decoders asked first and its muxers and encoders chosen as defaults first. The `ffmpeg` plugin that comes with Celesta has `"priority": 1`, so it goes before the other plugins that come with Celesta. A `priority` that is not a whole number is skipped with a reason, and the plugin is read as `0`.

```json
{
  "id": "ffmpeg",
  "name": "FFmpeg",
  "version": "0.9.0",
  "celesta": ">=0.9.0, <0.10.0",
  "protocol": 2,
  "roles": ["media"],
  "executable": { "macos-aarch64": "bin/celesta-codec-ffmpeg" },
  "media": {
    "decoders": [
      { "id": "ffmpeg", "name": "FFmpeg", "extensions": ["mp4", "mov", "webm", "mp3", "wav"] }
    ],
    "videoEncoders": [
      { "id": "prores", "name": "ProRes", "codecs": ["ap4h"], "inputFormats": ["rgba16"] }
    ],
    "audioEncoders": [
      { "id": "aac", "name": "AAC", "codecs": ["mp4a"] }
    ],
    "muxers": [
      { "id": "mp4", "name": "MP4", "extensions": ["mp4"], "video": ["avc1"], "audio": ["mp4a"],
        "defaultVideo": "avc1", "defaultAudio": "mp4a", "trimsLeadingAudio": true }
    ]
  }
}
```

The plugin's own `name` is optional: the display name the editor's export page shows beside a component, the plugin's `id` when absent. A component's `id` follows the rules for a plugin's id (see [Write a plugin](#write-a-plugin)) and is not used by another component of the same kind in the same plugin; Celesta skips a component that breaks this, says why, and keeps the plugin's other components. Components are named `<plugin>/<component>`, such as `ffmpeg-x264/libx264`. When a plugin runs, Celesta asks it to describe its components: whether each works on this computer (an FFmpeg built without libvpx says so for `vp9`, with the reason) and its settings. A plugin is either a media plugin or a content plugin: a manifest with both roles is skipped. A plugin made for the older `"roles": ["codec"]` is ignored, and Celesta reads no media through it: install a release of it made for your Celesta, or, in a source build, build `plugins/ffmpeg` again.

### Which decoder reads a file

Celesta uses every media plugin it may run, not only the first. To read a file, it asks the decoders that list the file's extension, in this order:

1. The decoders of plugins you installed: those from `--plugin-dir`, `CELESTA_PLUGIN_PATH`, your user plugin folder, and folders you added.
2. The decoders of the plugins that come with Celesta.
3. The decoders of a project's own plugins, once you trust the project and allow them.

Within each group, plugins with a higher `priority` go first, then plugins go in the search order above (so among the plugins that come with Celesta, `ffmpeg` goes first), and a plugin's decoders in the order its manifest lists them. A file whose extension no decoder lists, or that has none, goes to every decoder in this order. Project plugins come last, so a project's decoder gets a file only when the decoders before it that list the file's extension do not read it; a file whose extension only a project's decoders list goes to them alone. For the same reason, a project's media plugin cannot replace Celesta's bundled one: a project plugin with the id of a media plugin that comes with Celesta is not used.

When a decoder says the file is not one it reads, fails to read it, or its program does not start, Celesta asks the next one. A decoder whose program does not answer in time, or one stopped because you cancelled an export, ends the search: the file fails with that error, and no later decoder is asked. The first decoder that reads the file keeps it: its frames and audio come from that decoder from then on, and a later error there does not move on to another. A file that is known not to exist fails before any plugin starts; a path Celesta cannot check (for example, one that goes through a file as if it were a folder) is passed to the decoders. When no decoder reads a file, the error says the most it can: that the file was not found, that it has no video stream, or the first other error a decoder gave; when every decoder only said the file is not one it reads, the error names the decoders it tried, such as `no decoder could read <file> (tried ffmpeg/ffmpeg)`, and an export fails with the error code `unsupported_media`.

When a project's own plugin decodes files, the desktop app tells you: the Problems tab lists it as information, such as `my-decoder decodes 2 files in this project`, with the files, and the plugin's row in the [Plugins tab](#the-plugins-tab) says how many files it decodes.

### Which components write a video

An export takes its muxer from the output's extension, unless you name one with `--muxer`: the first muxer listing that extension among the plugins that come with Celesta, then among the plugins you installed, each by `priority` and then in the search order above (so `ffmpeg` goes first among the plugins that come with Celesta). Its `defaultVideo` and `defaultAudio` codecs then pick the encoders, unless you name them with `--video-encoder` and `--audio-encoder`: the first encoder that writes that codec and works on this computer, from the muxer's own plugin, then the plugins that come with Celesta, then the plugins you installed, each by `priority`. A project's own components are used only in the app, which loads the project's plugins: when you choose them on the export page, which marks them **(project)**, or when the project's [default preset](/en/docs/export/#export-presets) names them (and, for its encoders, when you choose its muxer). The command line and the MCP server never load a project's plugins. `--list-components` lists every installed component, and `--describe <plugin/id>` one component's settings; see [Choosing encoders and settings](/en/docs/export/#choosing-encoders-and-settings).

### With `cargo install`

Celesta also uses each media plugin program beside its own executable whose name starts with `celesta-codec-`, where `cargo install` leaves them, such as `celesta-codec-ffmpeg`, in name order. It asks each program for its manifest with `--manifest`, uses it only when none of the plugins Celesta found has the id it prints, other than a project's own, and counts it among the plugins that come with Celesta, so a project's plugin with that id cannot replace it either. It does not look there when that executable is in a Cargo build folder such as `target/release`: a source build uses the media plugin you build (see [Build from source](/en/docs/build-from-source/)). If you install Celesta with `cargo install`, install the media plugin too, so it lands beside the app; installing only `celesta-editor` or `celesta-exporter` leaves none. Until `celesta-codec-ffmpeg` is published to crates.io, install it from the repository with `cargo install --git https://github.com/mika-f/Celesta celesta-codec-ffmpeg` (FFmpeg 8.1.x development libraries are needed, as in [Build from source](/en/docs/build-from-source/)).

With `cargo install`, install optional encoders separately with their matching native environment, such as `celesta-codec-ffmpeg-x264`, which Celesta finds beside its executable in the same way. Installing the standard plugin alone does not add libx264 H.264 encoding. A `celesta-codec-ffmpeg-x264` built against an FFmpeg without libx264 (Fedora's `ffmpeg-free`, for example) refuses `--manifest`, so Celesta leaves it out. A program that gives no manifest counts as serving the plugin its name gives after `celesta-codec-` (`ffmpeg-x264` here), so naming `ffmpeg-x264/libx264` fails with `component_unavailable` and the reason `skipped at discovery (<program>): --manifest failed (…): <what the program said>` rather than `not installed`. Install it again against an FFmpeg with libx264. Each media program takes only `--manifest`, `--check-build`, or `--check-license`, or no argument when Celesta starts it; any other argument prints its usage and exits with status 2, so a misspelled check fails rather than passing.

### Without a decoder

Without any media decoder, video and audio cannot be shown or exported. The Problems tab shows **No media decoder can run** and says why: a media plugin has no program for this computer, waits for your approval, or is turned off, the media plugins found declare no decoder, or none was found. With no media plugin at all, no muxer writes a video, so a video export fails at once with the error code `unsupported_output` and a hint to install the media plugin; a still export, or a video export whose media plugins have no decoder, fails with `no_decoder` when it reads a video or audio file.

### Shared memory and other FFmpeg builds

Video frames cross memory Celesta shares with the media plugin's process, which no other process or user can open, rather than the plugin's pipes, when the plugin supports it (the `ffmpeg` plugin does from Celesta 0.10): the frames a decoder sends Celesta, and the frames an export sends a video encoder. An export sends its video encoder up to three frames ahead, in shared memory or through the pipe, so Celesta renders the next frames while the plugin encodes the earlier ones; for very large frames, such as 8K, it sends fewer, so that the frames waiting stay within 256 MiB. A plugin or video encoder that does not support it and decoded frames larger than 4096×2560 go through the pipe. So do exported frames of more than about 33 megapixels (such as 8192×4320): the shared memory holds a frame in the largest format the encoder accepts (16-bit RGBA for the `ffmpeg` plugin's encoders) and stays within 256 MiB. When Celesta cannot set up the shared memory itself (the system refuses to create it or to hand it to the plugin's process), the frames go through the pipe, and Celesta prints `celesta: frames go through the pipe:` with the reason once on stderr. If a media plugin misbehaves with it, set the environment variable `CELESTA_CODEC_SHARED_MEMORY=0` where you start Celesta to keep every frame, decoded or exported, in the pipe.

To use your own build, use a separate prefix for each variant and the environment variables in [Build from source](/en/docs/build-from-source/). Build the desired plugin with `node scripts/plugins/build.mts plugins/ffmpeg-x264 --out <your user plugin folder>` (or `plugins/ffmpeg` for the standard build). The user folder is searched before bundled plugins, and the app asks before running them. Only `ffmpeg-x264` ships `LICENSE-GPL-2.0` and `GPL-SOURCE-OFFER.md`; `ffmpeg` ships `LICENSE-LGPL-2.1` and `FFMPEG-SOURCE.md`. Both builds retain `LICENSE-MIT` and `LICENSE-APACHE`. The repository root contains only Celesta's MIT/Apache license texts; FFmpeg GPL/LGPL texts live under the relevant plugin folders. In a package, they remain in each plugin's `licenses/` folder and describe the linked FFmpeg build. Update source and relinking materials to match any build you distribute.

## Samples

The repository's `examples/plugins` has three plugins to start from:

| Plugin | What it shows |
| --- | --- |
| `flag` | A layer drawn by a plugin program: an image waving like a flag, as textured meshes Celesta draws. |
| `turntable` | A layer drawn in 3D by a plugin program: a checkered cube turning on a turntable, toon-shaded by the plugin's own WGSL material under a light and outlined, with a see-through pane and a cut-out leaf, as a scene Celesta draws with depth. |
| `beep-voice` | A plugin with only components: a speech channel that voices `planDialogue()` lines in game-style beeps instead of a speech engine, with lip sync. |

```sh
pnpm install && pnpm run build:runtime       # once, at the repository root
node scripts/plugins/build.mts examples/plugins/flag examples/plugins/turntable examples/plugins/beep-voice
node scripts/plugins/build.mts plugins/ffmpeg plugins/ffmpeg-x264   # the media plugin, which writes the MP4
cargo run -p celesta-exporter -- --react examples/plugins/flag/demo.tsx flag.mp4 --plugin-dir examples/plugins/dist --plugin-dir plugins/dist
cargo run -p celesta-exporter -- --react examples/plugins/turntable/demo.tsx turntable.mp4 --plugin-dir examples/plugins/dist --plugin-dir plugins/dist
```

The second `--plugin-dir` gives the exporter the media plugin, whose build needs the FFmpeg development libraries (see [Build from source](/en/docs/build-from-source/)).

## The VRM plugin

The repository's `plugins/vrm` draws VRM 1.0 avatars (`.vrm`) in 3D, toon-shaded with MToon as UniVRM draws it, with expressions, blinking, lip sync, eyes that follow a target, motion clips (`.vrma`), and swinging hair and clothes. Build it, then install the folder it writes:

```sh
pnpm install && pnpm run build:runtime       # once, at the repository root
node scripts/plugins/build.mts plugins/vrm   # writes plugins/dist/vrm
```

Copy `plugins/dist/vrm` into your user plugin folder, or pass `plugins/dist` to the exporter with `--plugin-dir`. Building needs Rust and Node.js, as building Celesta does. To export video with a source build of the exporter, build the media plugin into the same folder too, `node scripts/plugins/build.mts plugins/vrm plugins/ffmpeg plugins/ffmpeg-x264`, so that `--plugin-dir plugins/dist` gives the exporter both; the media plugin needs the FFmpeg development libraries.

```tsx
import { Composition } from '@celesta.studio/react';
import { Vrm } from '@celesta-plugin/vrm';

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={180}>
      <Vrm src="./avatar.vrm" camera="bust" lookAt="camera" expressions={{ happy: 0.4 }} mouth="a" />
    </Composition>
  );
}
```

| Prop | What it does |
| --- | --- |
| `src` | The `.vrm` file (VRM 1.0). |
| `width`, `height`, `aspect` | The layer's size in pixels. Give both, or one and `aspect` (width over height, 16:9 by default) for the other. `aspect` alone keeps the width at 1920; without any of them, 1920×1080. Each side, given or computed, must be from 1 to 16384 pixels, or `<Vrm>` throws. The avatar is drawn at that resolution: the layer is not scaled up from a smaller one. |
| `expressions` | Expression weights from 0 to 1, by VRM preset (`happy`, `angry`, `sad`, `relaxed`, `surprised`, `aa`, `blink`, …) or by the model's own custom names. They win over `blink` and the mouth. |
| `mouth` | A mouth shape (`'a'`, `'i'`, `'u'`, `'e'`, `'o'`, or `'closed'`), or vowel weights such as `{ aa: 0.5, oh: 0.3 }`. It sets all five vowels (`aa`, `ih`, `ou`, `ee`, `oh`): the ones it leaves out are 0, so a motion's mouth does not show through. |
| `lipSync`, `mouthSmoothing` | A lip-sync track (from `loadLipSync()`, or a spoken line's `lipSync`), used when `mouth` is not given, its shapes blended over `mouthSmoothing` frames (2 by default; 0 snaps). Like `mouth`, it sets all five vowels, so the mouth closes when the voice is silent. |
| `blink` | Blink timing, seeded like a character portrait's; `false` never blinks. |
| `lookAt` | Where the eyes look: `'camera'`, a point `[x, y, z]` in the avatar's space, or `{ yaw, pitch }` in degrees (positive yaw to the avatar's right, positive pitch up). Without it, straight ahead. |
| `camera` | `'full'` (the default: the whole avatar), `'bust'` (from the waist up, starting at the humanoid's spine bone), `'face'`, or `{ position, target, fov?, near?, far? }` in the avatar's space (`fov` in degrees, 30 by default). |
| `lights` | `{ directional: [{ direction, color, intensity }], ambient }`, up to four lights (a fifth throws), colors as `#rgb` or `#rrggbb`. It replaces the reference light, so without `ambient` the ambient light is black. Without `lights`, the plugin's reference light: Unity's default directional light from the upper front and its default ambient color. A light's `intensity`, and its color times that intensity, must stay finite as 32-bit floats; otherwise the frame fails with an error naming `lights.directional[i].intensity`. |
| `motions` | VRM animations (`.vrma`) on the timeline: `[{ src, from?, loop?, fadeIn? }]`. `from` is the frame of the enclosing sequence the motion starts on (0 by default), and `fadeIn` the frames over which it blends in from the motion before it (0 by default). A non-finite `from`, or a negative or non-finite `fadeIn`, throws. |
| `tracks` | Keyframed turns of humanoid bones on top of the motions: `{ head: { yaw: [{ frame, value, easing? }, …] }, … }`, with `pitch`, `yaw`, and `roll` in degrees, and `x`, `y`, and `z` in metres for `hips` only. |

The avatar's space is the file's: metres, +Y up, the avatar facing +Z with its left at +X, its feet near 0. The presets frame the avatar as it stands at rest, on the layer's shape; `full` fits its whole width too, so a prop held far out to one side moves the avatar off center.

When it opens a model, the plugin reports the model's terms of use as notices, one line each: who made it, its licence, and each term. A term that likely forbids what you are making is a `warning`: the avatar only for its author or for people the author licenses separately, no commercial use, or a credit required (the line says whom to credit where the video is published). A term the file leaves out counts as VRM 1.0's default, which is the strictest. The plugin draws the avatar either way: whether your use is allowed is for you to decide.

Meshes the file marks as first-person only (such as a body without a head, for the wearer's own view) are hidden, as UniVRM's third-person camera hides them. When the file marks one node more than once, its first mark counts, as in UniVRM, and each later one is a warning.

Other warnings point out parts of the file the plugin does not use as written: primitives it does not draw, textures a material is drawn without, and morph target binds it ignores. Each of these kinds, and the repeated first-person marks, is listed up to 20 times, then one line counts the rest (`and 12 more primitives that are not drawn`).

A material's emission is brightened by `KHR_materials_emissive_strength` when the material has it, and otherwise by `VRMC_materials_hdr_emissiveMultiplier`, as UniVRM reads them. A multiplier that is not a finite number refuses the model, and the message names the field.

Materials the file gives in plain glTF PBR (often metal or glass parts) are drawn as their flat base color for now, and a note names them. Not supported yet: VRM 0.x files, which the plugin refuses with a message saying how to convert them to VRM 1.0 (with UniVRM or the VRM Add-on for Blender).

The plugin refuses a file over 1 GiB, or one whose glTF JSON is over 64 MiB; a `.vrm`'s JSON is a few MB. `src` also opens a glTF JSON file (`.gltf`) whose buffers are embedded as `data:` URIs, but there the buffers are part of the JSON, so the whole file is capped at 64 MiB. A file whose buffers or images are in other files is refused.

The plugin writes the model's textures as image files to a cache, and Celesta reads them from there. The cache is `celesta-plugin-vrm` in your user cache folder (`%LOCALAPPDATA%` on Windows, `~/Library/Caches` on macOS, `$XDG_CACHE_HOME` or `~/.cache` elsewhere). To put it somewhere else, set `CELESTA_VRM_TEXTURE_CACHE` to an absolute path. The plugin refuses to open the model when the cache is a relative path, a symbolic link, not a folder, or (outside Windows) another user's or writable by others, and the message names the variable. Each time a model opens, the plugin removes the images no model has used for 30 days, and then, while the images take more than 2 GiB, the least recently used ones. Images used in the last day stay, so a model open in another window or an export running alongside never loses its textures. The plugin removes only the image files it wrote, and the temporary files a crashed write left behind, so other files in a folder you name with `CELESTA_VRM_TEXTURE_CACHE` stay. To empty the default cache yourself, delete its `celesta-plugin-vrm` folder while Celesta is closed. In a folder of your own, delete only the files named with 64 hexadecimal characters and `.png`, `.jpg`, `.webp`, or `.tmp`.

In the desktop app's Add menu, **VRM avatar** asks for a `.vrm` file and draws its box at 16:9.

### Motion, constraints, and spring bones

`motions` plays VRM animation clips (`.vrma`, `VRMC_vrm_animation` 1.0), retargeted onto the avatar as UniVRM retargets them: each bone turns from its rest as it does in the clip, and the hips travel as far as the clip's do, scaled by the avatar's hips height over the clip's. Motions run in `from` order, and each plays until the next one starts. Before the first, the avatar stands at rest. A motion with `loop` repeats; without it, its last pose holds. Expression weights in a clip apply too, but the ones you set in `expressions` win, and `mouth` or `lipSync` sets every vowel: with either, the clip's mouth never shows. Without them, the clip moves the mouth.

`tracks` adds keyframed turns after the motions, in the bone's own frame from rest: the angles turn as `lookAt`'s do (as in UniVRM): `yaw` turns it to the avatar's right, `pitch` lifts it to look up, and `roll` tilts its top toward the avatar's right. Negative values turn the other way: a negative `yaw` turns to the left, a negative `pitch` nods down. `x`, `y`, and `z` on `hips` move the whole avatar, in metres. A keyframe's `frame` is a frame of the enclosing sequence; a fractional one is rounded to the nearest frame. This plays a walk, fades into an idle clip over 15 frames from frame 90, then turns the head to look over the avatar's left shoulder and back:

```tsx
<Vrm
  src="./avatar.vrm"
  motions={[
    { src: './walk.vrma', loop: true },
    { src: './idle.vrma', from: 90, loop: true, fadeIn: 15 },
  ]}
  tracks={{
    head: {
      yaw: [
        { frame: 120, value: 0 },
        { frame: 135, value: -40, easing: 'ease-in-out' },
        { frame: 165, value: -40 },
        { frame: 180, value: 0, easing: 'ease-in-out' },
      ],
    },
  }}
/>
```

The avatar's node constraints (parts that follow other parts, such as a twisting sleeve) follow every frame, in UniVRM's order. Its spring bones (hair, skirts, and other swinging parts) are stepped on a fixed 60 Hz clock from the start of the layer, so any frame is the same however you reach it: scrubbed to, played in order, or exported at 30 or 60 fps. Keep `motions` and `tracks` the same across a layer's frames: the plugin keeps the springs' progress for each set of them, and data that changes every frame makes every frame step the springs again from the layer's start. Put values that change per frame in `expressions`, `mouth`, and `lookAt`, which the springs do not read.

Clips to try: `test.vrma` from [pixiv/three-vrm](https://github.com/pixiv/three-vrm) (`packages/three-vrm-animation/examples/models`, MIT), and VRoid Project's free VRMA motion pack on BOOTH (pixiv). The motion pack's terms allow commercial use but not redistributing the files, and ask for the credit "Animation credits to pixiv Inc.'s VRoid Project" (in Japanese, 「キャラクターアニメーション: ピクシブ株式会社 VRoidプロジェクト」) where the video is published. Check each clip's terms before you publish a video with it.

Limits you may meet:

- A layer opens at most 256 different clips, whose files take at most 1 GiB together, and whose animation keys take at most 1 GiB once read.
- The springs are stepped up to 4 hours into a layer; a later frame fails with an error saying so. An avatar whose springs are expensive to step (about ten thousand nodes, or a few thousand spring–collider checks, say) reaches less: the plugin warns of it when it opens the model, and a frame past its reach fails, naming the time it reaches.
- Not read: the angle limits of `VRMC_springBone_limit` (the plugin warns when the model has them, and those joints swing past their limits), a clip's `lookAt` animation, and any glTF animation but a clip's first. The springs do not react to `lookAt` or expressions.
- There is no foot IK: as in UniVRM, a clip whose rest pose stands a little higher or lower than its animation, or an avatar whose legs are long or short for its hips' height, can leave the feet a few centimetres into or above the floor.

## Write a plugin

A plugin has up to two parts, in any language:

- **Components** (`module` and `types` in the manifest): an ES module and its declarations. Celesta bundles the module into the entry, and its `react` and `@celesta.studio/*` imports use the app's own. A speech channel needs nothing else: it runs in `prepare()` like `@celesta.studio/voicevox` and returns lines with `audio` and `lipSync`.
- **A program** (`executable`, one per platform): it reads requests on standard input and answers on standard output. For a layer it returns, for a given time, either 2D textured triangle meshes with their colors, masks, and blending, or a 3D scene: skinned, morphing triangle geometry with materials, seen through a camera it places. Celesta draws them; scenes get depth, anti-aliasing, and opaque, cut-out, and see-through materials. A material is either Celesta's unlit material or one the program brings, written in WGSL against Celesta's material interface, which can shade with the directional lights and ambient light the program sends and add an outline. Celesta checks that WGSL before compiling it: it may not bind resources of its own or declare entry points, workgroup variables (`var<workgroup>`, which are for compute shaders), pipeline-overridable constants (`override`), names WGSL already predeclares (such as `clamp` or `vec4f`), or names starting with `celesta` in any letter case. Each frame can also change a material's parameters, as a model's material animation does, and can set the layer's canvas size for that frame in place of the model's (each side from 1 to 16384 pixels), so a component can pass the resolution it wants in the layer's data. Each texture, for meshes or a scene, is 1 to 8192 pixels a side (the GPU's limit): Celesta reads an image file's size before decoding it and refuses a larger or empty texture as the layer's error. An image file is given by its absolute path: the program runs in its own folder, but Celesta opens the file from its own, so it refuses a relative path, naming the texture. Every number in a model's vertex data must be finite: positions, normals, texture coordinates, tangents, and joint weights, and the offsets of morph targets. Celesta refuses a model with a NaN or an infinity there when it opens it, and the message names the geometry (or mesh), the stream, and the vertex, such as `geometry 0's normals are not finite at vertex 12`. A frame must depend only on the model, the time, and the layer's data: Celesta may ask for the same frame again, such as when it renders it again or exports it. Celesta waits a limited time for each answer: 30 seconds for the program to start and for a frame, 2 minutes to open a model, and 5 seconds to close one. Celesta closes a model that no layer has drawn in 300 renders in a row (about five seconds of playback or export at 60 fps), and opens it again if a layer draws it later. A program that does not answer in time is stopped and reported, with the last lines it wrote to standard error, and is started again when it is next needed. Cancelling an export stops the programs it runs at once, even in the middle of a frame, and **File > Reload Plugins** (or opening another file) stops the ones the preview was running. When it opens a model, the program can also return notices, such as the model's terms of use, which Celesta shows as above. Rust programs can use the `celesta-plugin` crate, whose `serve` keeps standard output for the answers on Windows, macOS, and Linux: whatever else the program, or a library it loads, prints there goes to standard error. The protocol itself is plain length-prefixed JSON.

The paths in the manifest (`module`, `types`, and each `executable`) are relative to the plugin's folder and may not leave it. Write them with `/`; a `\` is read as a separator too, on every platform, so `js\index.js` names the same file on Windows, macOS, and Linux.

To offer components in the desktop app's Add menu, list them in the manifest's `components`:

```json
"components": [
  { "name": "Flag", "label": "Flag", "src": "image", "props": { "amplitude": 0.06 } }
]
```

`name` is the component the module exports: a capital letter, then letters, digits, or `_`. `label` is what the menu says (the name when absent). With `src` (`"image"` or `"video"`), the component is added like an Image: the app asks for a file and writes `src`, `x`, `y`, and `width`. `"src": "model"` is for a file the plugin reads itself, such as a 3D model: list the file types it takes in `extensions` (lowercase letters and digits without the dot, each once, such as `["glb"]`), and give its layer's width over its height in `aspect` (from 1/16384 to 16384; required) so the app draws the box at that shape. The app then accepts only a file of those types and writes `src`, `x`, `y`, and `width` as for an Image. Since it writes no `height`, the component's own default aspect must equal `aspect`, or the layer will not match the box that was drawn. Without `src`, the component is drawn as a box and gets `x`, `y`, `width`, and `height`. `props` are written after those: names made of letters, digits, and `_`, with strings, numbers, and booleans as values, and none of the props the drawing writes. A manifest with `components` needs a `module`.

Every manifest gives `celesta`, the Celesta versions the plugin works with, as a Cargo version range such as `">=0.9.0, <0.10.0"`. It covers what the program relies on beyond the protocol (`protocol` is the format of its messages alone: `1` for a content plugin, which stays `1` while features such as 3D scenes are added on top of it, and `2` for a [media plugin](#media-plugins)) and the `react` and `@celesta.studio/*` APIs the module uses, which come with Celesta at its version. A pre-release of Celesta, such as `0.10.0-beta.1`, is in a range only when a bound of the range names a pre-release of the same version, as in `">=0.10.0-beta.1, <0.11.0"`. The repository's `scripts/plugins/build.mts` writes the range for you, from the Celesta it builds against up to that Celesta's next minor release (before 1.0) or next major release.

A plugin's id uses lowercase letters, digits, `-`, `_`, and `.`, and starts with a letter or digit; `builtin` is reserved for what Celesta has built in. Every file its manifest names is a relative path inside the plugin's folder, without `..` and without a link that leads out of it. Celesta skips a plugin that breaks these rules, has no `celesta` range or a range that leaves out the running Celesta, gives a `protocol` other than its role's, has both the content and media roles, or has a component that breaks the rules above, and says why.

The samples' READMEs walk through both parts. The repository's `docs/handoff/2026-11-01-plugin-system.md` describes the manifest and the protocol, the `docs/handoff/2026-11-02-plugin-*.md` notes describe what came later (such as `components`), `docs/handoff/2026-11-03-plugin-scene-content.md` describes 3D scenes and notices, `docs/handoff/2026-11-03-plugin-scene-materials.md` describes materials in WGSL, lights, render queues, and outlines, and `docs/handoff/2026-11-03-plugin-vrm.md` describes the VRM plugin and components that take a model file.
