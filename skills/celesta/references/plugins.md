# Content plugins

Plugins add components Celesta does not ship: layers a plugin's own program
draws (such as `@celesta-plugin/flag`) or component libraries such as a
speech channel (`@celesta-plugin/beep-voice`). They are new and may change.

## Using one

- Import its components as `@celesta-plugin/<id>`. Never `npm install` it:
  the runtime finds the plugin's folder and bundles its module into the
  entry.
- Celesta finds plugins, first of an id winning, in: each `--plugin-dir`
  (`Celesta-export`), each folder of `CELESTA_PLUGIN_PATH` (`;` on Windows,
  `:` elsewhere; absolute paths only, relative ones are skipped), the user's
  plugin folder (`%APPDATA%\Celesta\plugins`,
  `~/Library/Application Support/Celesta/plugins`, or
  `$XDG_DATA_HOME/celesta/plugins` / `~/.local/share/celesta/plugins`),
  folders the user added in the app's Plugins tab, and the folder bundled
  with Celesta; the app also searches a trusted project's own folders after
  `CELESTA_PLUGIN_PATH`. A folder is a plugin
  (it has `celesta-plugin.json`) or holds several. On Unix, a plugin from
  anywhere but `--plugin-dir` and `CELESTA_PLUGIN_PATH` that other users
  can write to (any file in it) is skipped.
- The app looks again each time a project opens and on File > Reload
  Plugins; skipped plugins show in its Problems tab. Its Plugins tab lists
  every plugin with its source and state, and lets the user allow, turn
  off, and add folders of plugins (kept in their `settings.json`). A plugin
  turned off there does not run in `Celesta-export` either.
- A plugin whose manifest lists `components` offers them in the app's Add
  menu; the app writes the element with its `@celesta-plugin/<id>` import.
  Code written by hand works the same. A component that takes a model file
  (such as `<Vrm>`) accepts only the file types it lists and is drawn at
  its manifest's aspect.
- The app asks the user to trust a project folder before it evaluates
  anything in it (remembered only when the root holds
  `celesta.config.json`, which `--init` writes; a project inside it with
  its own file is asked about separately), and to allow each plugin from
  the user's folder, an added folder, or a project (again
  when its manifest or a file the manifest names changes). Until allowed, a
  plugin does not run: an entry importing it opens in the app with its
  components drawing nothing (missing layers are expected, not a bug in the
  entry), and calling its other exports throws "waits for your approval";
  Problems says it is waiting. Only the user can answer: tell them to,
  rather than working around it. `Celesta-export` and MCP do not ask: they run the
  plugins they find (not a project's own) whether or not the app allowed
  them.
- A project may list its own plugin folders in `celesta.config.json` at its
  root: `{ "plugins": { "paths": ["./plugins"] } }`, relative and inside
  the project. Only the app reads it (after trust); give `Celesta-export`
  the same folders with `--plugin-dir`.
- Check and export with the folder named:

  ```sh
  Celesta-export --json --react film.tsx --frames 0,60 /tmp/celesta-check.png --plugin-dir <plugins>
  CELESTA_PLUGIN_PATH=<plugins> node <this skill>/scripts/inspect.mjs film.tsx --frames 0
  ```

  `inspect.mjs` lists a plugin layer as `plugin <id> at <time> s` with its
  assets (checked like other media) and `data`; only a rendered frame shows
  what it draws.
- Plugin layers draw in the app's preview and `Celesta-export` only, not in
  `@celesta.studio/web`.
- The usual rules hold: props must depend only on the frame, and async work
  (a speech channel's synthesis) goes in `prepare()`.
- `.celesta/plugins/<id>/index.d.ts` holds each plugin's declarations for
  type-checking, refreshed with the rest of `.celesta/` (in the app, a
  trusted project's own plugins and plugins waiting for approval too).

## The codec plugin

- Celesta links no codec: video and audio files are probed, decoded,
  encoded, and muxed by a codec plugin (`"roles": ["codec"]` in its
  manifest). The app's packages bundle `ffmpeg`. A source build makes it
  with `node scripts/plugins/build.mts plugins/ffmpeg` and points
  `CELESTA_PLUGIN_PATH` at the absolute path of `plugins/dist`
  (`export CELESTA_PLUGIN_PATH="$PWD/plugins/dist"` at the repository root).
  `cargo install` uses `celesta-codec-ffmpeg` beside Celesta's executables;
  that lookup is skipped in a Cargo build folder such as `target/release`.
- It is not imported and has no components. Without one, video and audio
  layers fail to decode and exports fail with `no_codec` (see
  [export.md](export.md)).

## The VRM plugin

The Celesta repository's `plugins/vrm` draws VRM 1.0 avatars (`.vrm`) as a
3D layer, MToon-shaded as UniVRM draws them. Build it with
`node scripts/plugins/build.mts plugins/vrm` (after `pnpm install && pnpm
run build:runtime` at the repository root); it writes `plugins/dist/vrm`,
which the user copies into a plugin folder, or name `plugins/dist` with
`--plugin-dir`. To export video from a source checkout, build the codec
plugin with it (`node scripts/plugins/build.mts plugins/vrm plugins/ffmpeg`),
so `--plugin-dir plugins/dist` names both.

```tsx
import { Vrm } from '@celesta-plugin/vrm';

<Vrm src="./avatar.vrm" camera="bust" lookAt="camera" expressions={{ happy: 0.4 }} mouth="a" />
```

- `width`, `height`, `aspect`: the layer's size in pixels (both, or one
  and `aspect`, default 16:9; `aspect` alone keeps 1920 wide; none is
  1920×1080). Each side, given or computed, is 1 to 16384, or `<Vrm>`
  throws. It is drawn at that resolution, not scaled.
- `expressions`: weights 0..1 by VRM preset (`happy`, `aa`, `blink`, …) or
  the model's custom names; they win over `blink` and the mouth. `mouth`:
  `'a'`…`'o'`, `'closed'`, or vowel weights (`aa ih ou ee oh`).
  `lipSync` (a `LipSyncTrack`) with `mouthSmoothing` (frames, default 2)
  when `mouth` is absent. Either sets all five vowels (the rest 0), so a
  motion's vowels never leak through a closed or silent mouth; with
  neither, a motion's vowels play. `blink`: blink timing, `false` for none.
- `lookAt`: `'camera'`, `[x, y, z]` in the avatar's space (metres, +Y up,
  facing +Z, its left at +X), or `{ yaw, pitch }` in degrees (yaw
  positive to the avatar's right, pitch positive up).
- `camera`: `'full'` (default), `'bust'`, `'face'`, or
  `{ position, target, fov?, near?, far? }`. Presets frame the avatar at
  rest on the layer's shape; `full` also fits a prop held out sideways,
  which moves the avatar off centre.
- `lights`: `{ directional: [{ direction, color, intensity }], ambient }`,
  at most 4 (a fifth throws), colours `#rgb`/`#rrggbb`; without
  `ambient`, the ambient is black. Default, the plugin's reference light
  (Unity's default light and ambient). `intensity`, and colour times
  `intensity`, must stay finite in f32, or the frame fails naming
  `lights.directional[i].intensity`.
- On open it reports the model's terms of use as notices; a term that
  likely forbids the video (author only, separately licensed people only,
  no commercial use, credit required) is a `warning`, and a term the file
  omits counts as VRM 1.0's strict default. It still draws: relay the
  terms, the user decides. Plain glTF PBR materials draw as flat base
  colour, with a note.
- First-person-only meshes are hidden (UniVRM's third-person view); a node
  annotated twice keeps its first annotation, as UniVRM, and each later
  one warns. Repeated warnings of one kind (primitives not drawn, textures
  a material draws without, morph binds ignored, repeated firstPerson
  annotations) list 20, then `and N more …`.
- Emission: `KHR_materials_emissive_strength`, else
  `VRMC_materials_hdr_emissiveMultiplier` (as UniVRM); a multiplier that is
  not a finite number refuses the model.
- Files: over 1 GiB, or glTF JSON over 64 MiB, refused. `src` may be a
  `.gltf` with `data:` URI buffers, but those are JSON too, so the whole
  `.gltf` is capped at 64 MiB. Buffers or images in other files: refused.
- Textures are written as image files to a cache: `celesta-plugin-vrm` in
  the user's cache folder, or `CELESTA_VRM_TEXTURE_CACHE`, which must be
  an absolute path to a real folder of the user's own, not writable by
  others. Never emptied.
- Not yet: VRM 0.x (refused, with how to convert it to 1.0).

### Motion, constraints, and spring bones

```tsx
<Vrm
  src="./avatar.vrm"
  motions={[
    { src: './walk.vrma', loop: true },
    { src: './idle.vrma', from: 90, loop: true, fadeIn: 15 },
  ]}
  // Look over the avatar's left shoulder and back.
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

- `motions`: `[{ src, from?, loop?, fadeIn? }]`, `src` a `.vrma`
  (`VRMC_vrm_animation` 1.0); `from` and `fadeIn` in frames of the
  enclosing sequence (default 0). Retargeted as UniVRM does: each bone's
  turn from rest, and the hips' travel scaled by the avatar's hips height
  over the clip's. Motions run in `from` order, each until the next
  starts; before the first the avatar rests; `loop` repeats, otherwise the
  last pose holds; `fadeIn` blends from the motion before. A clip's
  expression weights apply, but `expressions` wins key by key, and
  `mouth` or `lipSync` sets every vowel over the clip's. A
  non-finite `from`, or a negative or non-finite `fadeIn`, throws.
- `tracks`: `{ bone: { pitch?, yaw?, roll?, x?, y?, z? } }`, each a list
  of `{ frame, value, easing? }` (a fractional frame is rounded to the
  nearest), on top of the motions in the bone's frame from rest: degrees,
  turning as `lookAt`'s angles do: `yaw` to the avatar's right, `pitch`
  up, `roll` top to the avatar's right (negative `yaw` turns left,
  negative `pitch` nods down). `x`, `y`, `z`: metres, `hips` only (moves the avatar).
  An unknown bone or axis, `x`/`y`/`z` off the hips, an empty axis, or a
  non-finite frame or value throws.
- Node constraints follow every frame, in UniVRM's order. Spring bones
  (hair, skirts) step on a fixed 60 Hz clock from the layer's start, so
  any frame is the same however it is reached: scrubbed, played, or
  exported at 30 or 60 fps.
- Keep `motions` and `tracks` constant across a layer's frames. The
  plugin keeps the springs' progress per set of them; data computed from
  the frame makes every frame step the springs from 0 s again. Per-frame
  values go in `expressions`, `mouth`, `lookAt` (the springs read none).
- Limits: 256 distinct clips a layer, 1 GiB of clip files and 1 GiB of
  their keys together; springs step up to 4 h into a layer (a later frame
  fails); an avatar whose springs are costly to step (about ten thousand
  nodes, or a few thousand spring–collider checks) reaches less, warns
  when it opens, and fails past its reach, naming the time.
- Not read: `VRMC_springBone_limit` (warned; those joints swing past their
  limits), a clip's `lookAt` animation, glTF animations after a clip's
  first. Springs ignore `lookAt` and expressions. No foot IK: as in
  UniVRM, a clip's rest hips offset or the avatar's leg proportions can
  leave the feet a few cm into or above the floor.
- Clips: `test.vrma` from pixiv/three-vrm (MIT); VRoid Project's free VRMA
  motion pack on BOOTH (pixiv): commercial use allowed, the files may not
  be redistributed, and the credit
  「キャラクターアニメーション: ピクシブ株式会社 VRoidプロジェクト」 (in
  English, "Animation credits to pixiv Inc.'s VRoid Project") is required
  where the video is published. Relay each clip's terms to the user.

## Errors

| Message | Fix |
| --- | --- |
| `no plugin "<id>" is installed (looked in CELESTA_PLUGIN_PATH)` | In the app, the plugin may be waiting for the user's approval (Problems says so): ask the user to allow it, and do not work around the question. Otherwise the entry imports a plugin no folder provides: put it in the user plugin folder, pass `--plugin-dir`, or set `CELESTA_PLUGIN_PATH` (absolute paths). |
| `…; <folder> was skipped: <reason>` | That folder's manifest has a bad id or names a file outside the folder (`..`, an absolute path, a link out). The plugin must be fixed; do not work around it. |
| `plugin "<id>" has no JavaScript module` | The plugin has only a program; its layers come from another package. |
| `… has no executable for this platform` | The plugin was not built for this OS and CPU. |
| `plugin "<id>": …` | The plugin's own error, such as a missing asset. |
| `plugin layer <layer> cannot be drawn: …` | The plugin answered with meshes or a scene that fails Celesta's checks (bad indices, counts that do not match, a camera that sees nothing, more than 4 directional lights, a material shader that does not compile, parameters of the wrong size, a material that needs tangents on geometry without them, a model's or frame's canvas with a side under 1 or over 16384). The plugin must be fixed. |
| `the texture cache <path> is not an absolute path; set CELESTA_VRM_TEXTURE_CACHE …` (or `is a symbolic link`, `belongs to another user`, …) | The VRM plugin's texture cache is unsafe or relative: have the user set `CELESTA_VRM_TEXTURE_CACHE` to an absolute path of a folder of their own. |
| `the file is larger than the 1024 MiB the VRM plugin reads`, `the glTF JSON is <n> bytes, more than the 64 MiB the VRM plugin reads` | The model is past the VRM plugin's limits (a `.gltf`'s `data:` buffers count as JSON). Use a smaller file, or a `.vrm` instead of a `.gltf`. |
| `lights.directional[<i>].intensity is not finite: …`, or `… intensity times its color is not finite: …` | A `<Vrm>` light's `intensity` is too large for f32: lower it. |
| `<id> (layer <layer>): …`, or `note: <id> (layer <layer>): …` for information | Not an error: the plugin's notice about a model it opened, such as its terms of use. The exporter's terminal output prints a warning as a `warning: …` line; the app's Problems tab and `--json` (either kind among `warnings`) carry the message itself, a note keeping its `note: `. Relay it to the user; do not try to silence it. |

## Writing one

The manifest's optional `components` puts a component in the app's Add
menu: `{ "name": "Flag", "label": "Flag", "src": "image", "props": {
"amplitude": 0.06 } }`. `name` is the export (a capitalized identifier).
With `src` (`image` or `video`) it is added like an Image (`src`, `x`, `y`,
`width`); without, like a Rect's box (`x`, `y`, `width`, `height`). `props`
(strings, numbers, booleans) follow those and may not repeat them.
`"src": "model"` is a file the plugin reads itself: it needs `extensions`
(lowercase letters and digits, no dot, no repeats, e.g. `["glb"]`) and
`aspect` (width / height, 1/16384 to 16384) for the drawn box. It is
added like an Image, `width` only, so the component's own default aspect
must equal `aspect`.

A plugin's program answers each frame with either 2D meshes or a 3D scene
(skinned, morphing geometry with materials and a camera; Celesta draws it
with depth; each material is Celesta's unlit one or the plugin's own in
WGSL, which can shade with the directional lights (at most 4) and
spherical-harmonics ambient light the program sends and add an outline; a
frame can replace a material's parameters, and set the layer's canvas for
that frame in place of the model's, each side 1 to 16384, as a resolution
the component passes in `data`), and may return notices when it opens a
model. A frame must depend only on the model, time, and `data`: Celesta
may ask for the same frame again. A material's WGSL may not bind resources or declare entry
points, `var<workgroup>` variables, `override` constants, names WGSL
predeclares (such as `clamp` or `vec4f`), or names starting with `celesta`
in any letter case.

Start from the samples in the Celesta repository's `examples/plugins`
(`flag`: a program that returns meshes; `turntable`: a program that returns
a 3D scene; `beep-voice`: a module-only speech channel),
`docs/handoff/2026-11-01-plugin-system.md` (manifest and protocol), the
later `docs/handoff/2026-11-02-plugin-*.md` notes (search locations,
approval, `components`), `docs/handoff/2026-11-03-plugin-scene-content.md`
(scenes, notices, the current `ContentProvider`), and
`docs/handoff/2026-11-03-plugin-scene-materials.md` (materials in WGSL and
their interface, lights, render queues, outlines, per-frame parameters),
and `docs/handoff/2026-11-03-plugin-vrm.md` (the VRM plugin, its own
material, and model-file components). A plugin with a bad component is
skipped whole, like one with a bad id.
