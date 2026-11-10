## In your browser

Open the [web editor](/en/#playground), change or open a TSX composition, and choose **Export MP4**. The source, preview, and export use Celesta’s React scene evaluator. Add local media files by name with **Add media**. The browser needs H.264 WebCodecs support; audio also needs AAC encoding. The browser preview is silent, while the exported MP4 includes constant-rate audio clips. For text measurement/fitting (TextBox, fitText, useFitText, measureText, useTextMetrics), PSDs, project timelines, `prepare()`, third-party imports, animated audio automation, and native-renderer output, use the installed app or CLI.

## From the app

Choose **Export…** in the title bar to open the export dialog. From top to bottom, it has:

- **Preset**: **None** for Celesta's defaults, then the project's presets under **This project** and yours under **My presets** (see Export presets below).
- **Container**: the muxer that writes the file, such as **MP4 · ffmpeg** for H.264 video, or **QuickTime · ffmpeg** or **WebM · ffmpeg** for transparent video (see Transparent video below).
- **Video encoder** and its settings, grouped as the encoder groups them. Settings that only apply to another choice are hidden until you make it, and the rarely needed ones are under **Advanced**, closed at first.
- **Audio encoder**, or **None** for a video without sound, and its settings.
- **Range**: **Whole composition**, or the start and end you marked by pressing `I` and `O` on the timeline, with **Clear** to export the whole composition again.
- **Loudness**: **Off**, **Streaming (-14 LUFS)**, or **Broadcast (-23 LUFS)**, to bring the soundtrack to a loudness target (see Loudness for streaming and broadcast below). The choice stays until you quit the app.

Each component shows its name and its plugin, and one from a plugin that comes with the project is marked **(project)**. When the dialog opens, Celesta asks each media plugin which of its components work on this computer: until it answers, they show **Checking…**, and one that cannot run is listed but cannot be chosen, with its reason. A plugin that could not answer at all is asked again the next time you open the dialog. The dialog opens with the project's default preset when it has one, else the choice you last exported with, else the defaults for `.mp4`. A presets file that cannot be read is named under **Preset**, with what is wrong with it, and its presets are left out of the list.

Changing **Container** keeps an encoder you chose in the dialog, or that the chosen preset names, with its settings, when the new container takes it. Any other encoder becomes the new container's default, so going from **MP4** to **QuickTime** gives ProRes 4444 with PCM audio, as an export to `.mov` does. Audio you set to **None** stays off. Audio left out only because a container takes none, such as IVF, comes back with the next container that takes audio.

Choose **Export…** in the dialog to pick the destination. The save panel suggests the container's first extension, and a name with an extension the container does not write takes that one instead. **Cancel** or Escape closes the dialog and keeps what you chose for the next time you open it, until you quit the app or reload the plugins. **Export…** also remembers the choice in your presets file (see Export presets below); when that file cannot be written, the **Problems** tab says so under **Export presets**, and the export goes ahead. The status bar shows progress; **Cancel export** in the title bar stops the job, along with any plugin it is waiting on. When the export finishes, its notice shows the soundtrack's loudness. The warnings and notes the export reported, such as a font fallback or a plugin's note on a frame you never previewed, are logged as one entry in the **Problems** tab, each once and in the order they came, as the command line prints them.

**Save as preset…** saves the dialog's choice under a name, to **This project** or **My presets**. A project that has not been saved yet has to be saved first. When the name is already in that file, the button reads **Replace**, and saving replaces that preset. Enter in the name field saves too; **Export…** always exports. The saved preset is then the one chosen, as if you had picked it from **Preset**, so a later **Container** change keeps its encoders. Like **Export…**, it cannot save while the dialog is still checking or a setting is invalid, and it says why. Choosing a preset fills in its container, encoders, and settings; changing any of them afterwards sets **Preset** back to **None**. A preset saved with an older version of a plugin may name a setting the plugin no longer has, a value it no longer takes, or a component that is gone: the dialog leaves the setting out, uses the setting's default, or uses the default component, and lists each change under **Preset**. The choice you last exported with is checked the same way, and its warnings call it “your last export”.

## From the command line

The app includes a command-line exporter. Open a terminal in the project folder containing `first-scene.tsx`. Select your platform:

::::tabs[Exporter platform]{default="macOS"}
:::tab[macOS]

These commands assume Celesta is installed in `/Applications`. Adjust the app path if you installed it elsewhere.

```sh title="Export a React composition"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react first-scene.tsx output.mp4
```

:::
:::tab[Windows]

Use PowerShell. Replace `C:\path\to\Celesta` with your installed or extracted Celesta folder. The installer does not add Celesta to PATH.

```sh title="Export a React composition"
& "C:\path\to\Celesta\Celesta-export.exe" --react first-scene.tsx output.mp4
```

:::
::::

The output's extension picks the kind of file: `.mp4` for H.264 video with AAC audio, `.mov` or `.webm` for transparent video (see Transparent video below), or `.png`, `.jpg`, or `.webp` for still frames (see Still frames and contact sheets below). Add `--overwrite` to replace an existing output file. Use `--from` and `--to` to select a time range:

::::tabs[Exporter platform]{default="macOS"}
:::tab[macOS]

```sh title="Export the first second"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --from 0 --to 1 --react first-scene.tsx section.mp4
```

:::
:::tab[Windows]

```sh title="Export the first second"
& "C:\path\to\Celesta\Celesta-export.exe" --from 0 --to 1 --react first-scene.tsx section.mp4
```

:::
::::

## Subtitle files

Import an SRT or WebVTT file and read it with `parseSubtitles()`. It returns `{ from, durationInFrames, text }` cues, the shape `Sequence` takes. A byte order mark, CRLF line ends, cues of several lines, and milliseconds after `,` or `.` all work, and tags such as `<i>` are removed.

```tsx title="Draw the cues of an SRT file"
import { Sequence, Subtitles, Text, parseSubtitles } from '@celesta.studio/react';
import captions from './captions.srt';

const cues = parseSubtitles(captions, { fps: 30 });

// Inside the composition
<Subtitles cues={cues} />
{cues.map((cue, index) => (
  <Sequence key={index} from={cue.from} durationInFrames={cue.durationInFrames}>
    <Text x={960} y={960} anchorX={0.5} anchorY={0.5}>{cue.text}</Text>
  </Sequence>
))}
```

`--subtitles` writes a subtitle file beside the video: SRT for a `.srt` path, WebVTT for `.vtt`. It holds the cues `<Subtitles>` declares and the `<Dialogue>` lines, each for as long as it is shown. With `--from` and `--to`, the times start at the span's start and cues are cut to the span.

::::tabs[Exporter platform]{default="macOS"}
:::tab[macOS]

```sh title="Export the subtitles too"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react first-scene.tsx output.mp4 --subtitles output.srt
```

:::
:::tab[Windows]

```sh title="Export the subtitles too"
& "C:\path\to\Celesta\Celesta-export.exe" --react first-scene.tsx output.mp4 --subtitles output.srt
```

:::
::::

To write timed data of your own, use `formatSubtitles(cues, { fps, format: 'srt' })`.

## Choosing encoders and settings

Celesta writes a video with three media components: a muxer, which writes the file, a video encoder, and an audio encoder. Each is named `<plugin>/<component>`, such as `ffmpeg/libx264`. The output's extension picks the muxer, and the muxer's default codecs pick the encoders:

| Extension | Muxer | Video encoder | Audio encoder |
| --- | --- | --- | --- |
| `.mp4`, `.m4v` | `ffmpeg/mp4` | `ffmpeg/libx264` (H.264) | `ffmpeg/aac` (AAC) |
| `.mov` | `ffmpeg/mov` | `ffmpeg/prores` (ProRes 4444) | `ffmpeg/pcm` (24-bit PCM) |
| `.webm` | `ffmpeg/webm` | `ffmpeg/vp9` (VP9 with alpha) | `ffmpeg/opus` (Opus, 48 kHz) |

`--list-components` lists every installed component with its kind and plugin, including Celesta's built-in image components (`builtin/png` and the others, shown as `built-in`), which write still frames only (see Still frames and contact sheets below); add `--check` to ask each plugin whether its components work on this computer, which starts each plugin once. `--describe <plugin/id>` shows one component's settings: each key with its type, default, and choices or range. Both print JSON with `--json`, and neither takes a source or an output.

`--video-encoder` and `--audio-encoder` choose an encoder other than the muxer's default, and `--muxer` a muxer other than the extension's. When that muxer does not list the output's extension, it still writes the file, and the export warns, such as `ffmpeg/webm does not write .mp4 files; the file is written as WebM anyway`. `--audio-encoder none` writes a video without audio, and cannot be given with `--audio-option`; nor can `--audio-option` be given when the muxer takes no audio and no audio encoder is named. `--video-option key=value` and `--audio-option key=value` set the chosen encoder's settings, read by each setting's type; repeat them for more settings. A key the encoder does not have, or a value it does not take, fails before anything renders. The FFmpeg plugin that comes with Celesta has these settings in this release (`--describe` shows those of any installed component):

| Component | Key | Values (default) |
| --- | --- | --- |
| `ffmpeg/libx264` | `preset` | `ultrafast`, `superfast`, `veryfast`, `faster`, `fast`, `medium`, `slow`, `slower`, or `veryslow` (`medium`) |
| | `rateControl` | `crf` for constant quality or `vbr` for an average bitrate (`crf`) |
| | `crf` | 0–51, with `rateControl=crf`; lower is higher quality and larger (18) |
| | `bitrate` | bits per second, with `rateControl=vbr` (8000000) |
| | `bitDepth` | `8`, or `10` for 10-bit H.264 (High 10) (`8`) |
| | `tune` | an x264 tune, such as `film` or `animation` (none) |
| `ffmpeg/prores` | `profile` | `4444`, or `hq` for 422 HQ without alpha (`4444`) |
| `ffmpeg/vp9` | `crf` | 0–63 (20) |
| | `alpha` | `true` or `false` (`true`) |
| `ffmpeg/aac`, `ffmpeg/opus` | `bitrate` | 32000–512000 bits per second (192000) |
| `ffmpeg/pcm` | `format` | `s16le`, `s24le`, or `f32le` (`s24le`) |

Muxers keep their defaults: MP4 and MOV are written with `faststart`, which puts the index first so a browser can start playing before the download ends.

Faster x264 presets such as `veryfast` reduce encoding time but usually make larger files at similar quality. `bitDepth=10` keeps dark gradients smoother, but plays on fewer devices. `--render-quality draft` speeds up timing checks by skipping scaled-text re-rasterization; use `final` (the default) to judge the finished image. Celesta converts RGB to YUV on the GPU whenever the encoder takes YUV and the renderer is not a software one; otherwise the encoder converts.

Every video export is BT.709 with limited range. Celesta converts on the GPU, or the encoder does when it takes RGB, and the file is tagged to say so (H.264's VUI, `colr` in MP4 and MOV, `Colour` in WebM), so players show the colors as rendered. Earlier releases wrote untagged MP4 files with BT.601 coefficients, which players that assume BT.709 for HD showed slightly off.

::::tabs[Exporter platform]{default="macOS"}
:::tab[macOS]

```sh title="Export with a faster encoder preset"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --video-option preset=veryfast --video-option crf=18 first-scene.tsx draft.mp4
```

```sh title="List the installed components"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --list-components --check
```

:::
:::tab[Windows]

```sh title="Export with a faster encoder preset"
& "C:\path\to\Celesta\Celesta-export.exe" --react --video-option preset=veryfast --video-option crf=18 first-scene.tsx draft.mp4
```

```sh title="List the installed components"
& "C:\path\to\Celesta\Celesta-export.exe" --list-components --check
```

:::
::::

The MCP `export_video` tool and `@celesta.studio/exporter` take the same choices as `muxer`, `videoEncoder` (`{ "id": "ffmpeg/libx264", "options": { "preset": "veryfast", "crf": 18 } }`), and `audioEncoder` (the same shape, or `"none"`), and an export preset as `preset` and `presetFile` (see Export presets below). MCP checks each option's type, so `crf` must be a number there; the package passes its options to the exporter as `--video-option` and `--audio-option` text, which is read by each setting's type as on the command line, so `"18"` works there too. MCP's `list_components` and `describe_component` tools, and the package's `listComponents()` and `describeComponent()`, list and describe the components.

## Export presets

An export preset is a named choice of container, encoders, and their settings. Presets are kept in files named `export-presets.json`, in two places:

- Yours are in Celesta's settings folder, beside `settings.json`: `~/Library/Application Support/Celesta` on macOS, `%APPDATA%\Celesta` on Windows, and `$XDG_CONFIG_HOME/celesta` (by default `~/.config/celesta`) elsewhere.
- A project's are at its root: the folder holding `celesta.config.json`, or else the composition's own folder. They travel with the project's repository, so a CI export uses them too.

The app's **Save as preset…** writes both, and you can also write them by hand. Rewriting a file keeps its permissions (a new one is readable by everyone, `0644`, on macOS and Linux), and a file that is a symbolic link is written where the link points:

```json title="export-presets.json"
{
  "version": 1,
  "presets": [
    { "name": "YouTube 1080p",
      "muxer": "ffmpeg/mp4",
      "video": { "component": "ffmpeg/libx264", "options": { "crf": 18, "preset": "slow" } },
      "audio": { "component": "ffmpeg/aac", "options": { "bitrate": 192000 } } },
    { "name": "Silent", "muxer": "ffmpeg/mp4", "audio": "none" }
  ],
  "default": "YouTube 1080p"
}
```

Components are named `<plugin>/<component>`, and `"audio": "none"` leaves the sound out. A part a preset leaves out takes its default: without `muxer`, the output's extension picks it, and without a `component`, the muxer's default encoder is used, with the preset's `options` if it has any (unless you name an encoder yourself, as below). `options` need only the settings that differ from their defaults; **Save as preset…** saves just those, so a preset follows a plugin's later defaults for everything you did not change. Only a project's file has `"default"`. Your file also keeps `"last"`, the choice you last exported with from the app. `version` is `1`.

`--preset <name>` exports with a preset. The name is looked up in the file `--preset-file <path>` names, then in the project's file, then in yours, and the first match is used. When a project preset has the same name as one of yours, the export warns that it hides yours. A name in no file fails with the error code `unknown_preset`. x264's preset is the setting above, not an export preset: `--preset veryfast` fails, and its hint says to use `--video-option preset=veryfast`. Without `--json`, an error's hint is printed on a `hint:` line after its message. A `--preset-file` that is missing or is not a presets file is rejected with the arguments.

The other flags go over the preset: `--muxer`, `--video-encoder`, and `--audio-encoder` replace its components, and `--video-option` and `--audio-option` replace its value for each key they give. The preset's options go only to the encoder they were saved for. When the preset names its encoder, `--video-encoder` (or `--audio-encoder`) naming that same encoder keeps them, and naming another drops them. When the preset names no encoder, its options go to the muxer's default encoder only while you name none: naming any encoder, even that default one, drops them. When the preset's muxer does not list the output's extension, it writes the file anyway, with the same warning as `--muxer`.

A video export that gives no `--preset` and none of `--muxer`, `--video-encoder`, and `--audio-encoder` uses the project's `default` preset, when the project's file names one. `--audio-encoder none` counts as one of them, while `--video-option` and `--audio-option` alone go over the default preset. A `default` naming a preset that is in no file fails with `unknown_preset`. A presets file that cannot be read, is not valid JSON, or has another `version` fails with `invalid_presets`, but only when the export has to look for a preset in it. A broken file of yours does not stop an export that names no preset in a project without a default preset, nor an export whose preset (or default preset) is found in the project's file: that export warns that your file could not be checked for a preset of the same name, and goes ahead.

A preset saved with an older version of a plugin may name a setting the encoder no longer has, give a value it no longer takes, or name a component that is gone or no longer works here. An export with that preset still runs: it leaves the setting out, uses the setting's default, or uses the default component (without the preset's options for it), and warns about each, on the command line and in the `--json` report's `warnings`. A value you give yourself, with `--video-option` or `--audio-option`, still fails with `invalid_option`, except that `--audio-option` with a preset that leaves the audio out (`"audio": "none"`, and no `--audio-encoder`) is ignored, with a warning that the preset leaves the audio out.

::::tabs[Exporter platform]{default="macOS"}
:::tab[macOS]

```sh title="Export with a preset"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --preset "YouTube 1080p" first-scene.tsx output.mp4
```

:::
:::tab[Windows]

```sh title="Export with a preset"
& "C:\path\to\Celesta\Celesta-export.exe" --react --preset "YouTube 1080p" first-scene.tsx output.mp4
```

:::
::::

The MCP `export_video` tool and `@celesta.studio/exporter` take `preset` and `presetFile`, which are looked up the same way, and use the project's default preset in the same cases; their reports carry the same warnings.

## Videos that play on repeat

Shorts and TikTok play a video on repeat. When its last frame runs into its first, add `--loop` so the sound does too. An AAC encoder starts from silence, so without it a hit on the first frame, such as a kick, comes out smeared for about 10 ms, and the player repeats that on every pass. With `--loop`, the encoder first takes the end of the soundtrack, and the MP4's edit list hides that lead-in: the sound still lasts exactly as long as the video. The soundtrack itself must loop, with anything that rings past the end mixed into the start. `--loop` is accepted for any video: MP4 and MOV hide the lead-in, while a muxer that cannot hide it, such as WebM's, ignores `--loop`. A `.mov`'s PCM audio loops without it, since PCM has no encoder delay.

::::tabs[Exporter platform]{default="macOS"}
:::tab[macOS]

```sh title="Export a video that plays on repeat"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --loop first-scene.tsx loop.mp4
```

:::
:::tab[Windows]

```sh title="Export a video that plays on repeat"
& "C:\path\to\Celesta\Celesta-export.exe" --react --loop first-scene.tsx loop.mp4
```

:::
::::

## Loudness for streaming and broadcast

Streaming services play every video at about the same loudness, around -14 LUFS on YouTube: a louder video is turned down, and a quieter one stays quiet. Every video export measures the whole soundtrack's integrated loudness (ITU-R BS.1770, as FFmpeg's `ebur128` filter measures it) and its true peak. The command line prints them after `export complete`, the app shows them when the export finishes, and `--json` reports them.

Add `--loudness` to bring the soundtrack to a target: a number of LUFS from -70 to -5, `streaming` (-14), or `broadcast` (-23, EBU R128). One gain turns the whole mix up or down, so its balance stays as you mixed it. Where that gain would take a true peak over `--true-peak` (default -1 dBTP; -9 to 0), a limiter holds the peak there. With `--peak-handling lower`, the gain stops where the loudest peak meets the ceiling instead, which leaves the soundtrack quieter than the target but otherwise untouched; the export then warns how loud it is. A soundtrack that is silent or shorter than 0.4 seconds has no integrated loudness and is left as it is. The same project always comes out the same.

::::tabs[Exporter platform]{default="macOS"}
:::tab[macOS]

```sh title="Export at streaming loudness"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --loudness streaming first-scene.tsx output.mp4
```

:::
:::tab[Windows]

```sh title="Export at streaming loudness"
& "C:\path\to\Celesta\Celesta-export.exe" --react --loudness streaming first-scene.tsx output.mp4
```

:::
::::

The loudness is measured and set on the soundtrack before it is encoded. A `.mov`'s PCM keeps it exactly. AAC (`.mp4`) and Opus (`.webm`) move the loudness by about a tenth of an LU and the true peak by up to about half a dB either way, so the -1 dBTP default can read about -0.5 dBTP in the file; give `--true-peak -2` for more headroom. In the app, the export dialog's **Loudness** sets the streaming or broadcast target with the -1 dBTP limiter. For another target, ceiling, or `--peak-handling lower`, use the command line, the MCP `export_video` tool, or `@celesta.studio/exporter`.

## Transparent video

The file extension picks the video format. A `.mp4` file is H.264 with AAC audio, as above. A `.mov` file is ProRes 4444 (`yuva444p10le`) with 24-bit PCM audio, and a `.webm` file is VP9 with alpha (`yuva420p`, CRF 20) with 48 kHz Opus audio. In the app, choose **QuickTime** or **WebM** as the export dialog's **Container**. On the command line, run the same command as above with an output such as `overlay.mov` or `overlay.webm`. The encoder decides whether the video keeps alpha: ProRes 4444 and VP9 do, unless you give `--video-option profile=hq` or `--video-option alpha=false`, and H.264 never does.

Only areas nothing draws on are transparent, so a layer that fills the whole frame, such as a full-frame background, makes the output opaque. The alpha is straight (unassociated), and colors are converted and tagged as BT.709 (limited range).

Celesta encodes through its [media plugins](/en/docs/plugins/#media-plugins). If the plugin's FFmpeg build lacks an encoder the video needs, such as an LGPL build without libx264, the export fails with the error code `component_unavailable` and the reason. Without a media plugin, no muxer writes the extension, and the export fails with `unsupported_output` and a hint to install one.

## Still frames and contact sheets

--frame / --frames selects zero-based frame numbers. Multiple frames create names such as check-000090.png. --every N selects every Nth frame of the --from/--to range (or the whole composition) and its last frame. Add --contact-sheet to place them in one labelled grid; --columns and --tile-width control its layout. Explicit frame numbers cannot be combined with --every, --from, or --to.

The file extension picks the image format, which Celesta writes with its built-in image components, so stills need no media plugin: `.png` is lossless PNG (`builtin/png`), `.jpg` or `.jpeg` is JPEG (`builtin/jpeg`), and `.webp` is WebP (`builtin/webp`), lossy unless you set `lossless=true` (below). JPEG and WebP are smaller, which suits thumbnails; `--quality` sets their quality from 1 to 100 (default 90), and PNG does not take it. By default, frames are drawn over the same opaque background as an MP4, so a JPEG, which has no alpha channel, loses nothing; `transparent=true` (below) keeps a PNG or WebP transparent. A contact sheet is at most 16384 pixels on each side, and a larger one fails with `invalid_selection` before rendering. WebP holds at most 16383 pixels on each side, so a WebP sheet 16384 pixels wide or high fails with `invalid_option`, also before rendering.

`--video-option key=value` sets the image encoder's settings, as it does a video encoder's. `--describe builtin/png` shows them:

| Component | Key | Values (default) |
| --- | --- | --- |
| `builtin/png` | `compression` | `none`, `fastest`, `fast`, `balanced`, or `high`; every level is lossless, and a smaller file takes longer to write (`balanced`) |
| | `depth` | `8`, or `16` for 16-bit PNG (`8`); a contact sheet is 8-bit only |
| | `transparent` | `true` to leave the background transparent (`false`) |
| `builtin/jpeg` | `quality` | 1–100, the same as `--quality` (90) |
| `builtin/webp` | `lossless` | `true` for lossless WebP (`false`) |
| | `quality` | 1–100, the same as `--quality`, when not lossless (90) |
| | `transparent` | as for PNG (`false`) |

Give the quality with `--quality` or `--video-option quality=`, not both. With `transparent=true`, areas nothing draws on stay transparent, as in a transparent video: the alpha is straight (unassociated), and a layer that fills the whole frame makes the image opaque. JPEG has no alpha channel, so it has no such setting. In a contact sheet, transparent areas show over a gray checkerboard, and the sheet itself is opaque.

A media plugin can add other image formats, or its own writer for these, as components. Celesta uses them only when you name them, with `--muxer` and `--video-encoder`: a muxer that writes one image, such as `builtin/image`, takes one frame or a contact sheet, and several frames need a sequence muxer, such as `builtin/imageSequence`. Stills take no export preset: `--preset`, `--preset-file`, the project's default preset, and the audio flags apply to video only. Several frames are written one at a time, so when a frame fails or the export is cancelled, the files already written stay; a single image is written only once it is whole, so a cancelled export leaves none. The MCP `render_frames` and `contact_sheet` tools and `@celesta.studio/exporter`'s `exportStills` take `muxer` and `videoEncoder` (`{ "id": "builtin/webp", "options": { "lossless": true } }`), as a video export does; a choice setting such as `depth` takes a string there (`"16"`).

::::tabs[Exporter platform]{default="macOS"}
:::tab[macOS]

```sh title="Export selected frames"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --frames 0,90 first-scene.tsx check.png
```

```sh title="Export a contact sheet"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --every 30 --contact-sheet --columns 4 --tile-width 320 first-scene.tsx sheet.png
```

```sh title="Export a JPEG thumbnail"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --frame 90 --quality 85 first-scene.tsx thumbnail.jpg
```

```sh title="Export a transparent PNG"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --frame 90 --video-option transparent=true first-scene.tsx overlay.png
```

:::
:::tab[Windows]

```sh title="Export selected frames"
& "C:\path\to\Celesta\Celesta-export.exe" --react --frames 0,90 first-scene.tsx check.png
```

```sh title="Export a contact sheet"
& "C:\path\to\Celesta\Celesta-export.exe" --react --every 30 --contact-sheet --columns 4 --tile-width 320 first-scene.tsx sheet.png
```

```sh title="Export a JPEG thumbnail"
& "C:\path\to\Celesta\Celesta-export.exe" --react --frame 90 --quality 85 first-scene.tsx thumbnail.jpg
```

```sh title="Export a transparent PNG"
& "C:\path\to\Celesta\Celesta-export.exe" --react --frame 90 --video-option transparent=true first-scene.tsx overlay.png
```

:::
::::

Add `--debug-guides` to draw a React composition's `<DebugOverlay>` and `<DebugBounds>` into the still frames or contact sheet, as the preview shows them. A video export never draws them.

::::tabs[Exporter platform]{default="macOS"}
:::tab[macOS]

```sh title="Check the safe area in a contact sheet"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --every 30 --contact-sheet --debug-guides first-scene.tsx sheet.png
```

:::
:::tab[Windows]

```sh title="Check the safe area in a contact sheet"
& "C:\path\to\Celesta\Celesta-export.exe" --react --every 30 --contact-sheet --debug-guides first-scene.tsx sheet.png
```

:::
::::

## Select the graphics API

Both the editor and exporter accept --driver auto (default), vulkan, dx12 (Windows), metal (macOS), or gl. Auto chooses an available adapter in wgpu’s backend order: Vulkan, Metal, DX12, then OpenGL. A specific driver must be available in the build and supported by the machine. This chooses the rendering API; encoding still goes through the media components.

::::tabs[Exporter platform]{default="macOS"}
:::tab[macOS]

```sh title="Choose a rendering API"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --driver auto first-scene.tsx output.mp4
```

:::
:::tab[Windows]

```sh title="Choose a rendering API"
& "C:\path\to\Celesta\Celesta-export.exe" --react --driver auto first-scene.tsx output.mp4
```

:::
::::

For scripts and AI agents, add `--json`. The exporter then prints no progress, only one line of JSON when it finishes: the status, every file it wrote, warnings, the composition's size, fps and frame count once the exporter has loaded it, and on failure an error code and message. Each video entry in the list of written files has `"muxer"` (such as `"ffmpeg/mp4"`), `"video"` and `"audio"` (the encoder's `"component"` and the `"codec"` string it wrote, such as `"avc1.64001F"`; `"audio"` is `null` for a video without sound), `"alpha"` (`true` or `false`), and `"files"`, every file the muxer wrote. A video with sound has `"loudness"`: `"integratedLufs"`, `"truePeakDbtp"`, and with `--loudness` the `"gainDb"` it applied. The error codes about components are `unsupported_output` (no muxer writes the output's extension), `component_unavailable` (a component is not installed or does not work here, with the reason), `incompatible_components` (the muxer does not take the encoder's codec), `invalid_option` (a setting the encoder does not have or a value it does not take), `unknown_preset` (no presets file has the preset's name), and `invalid_presets` (a presets file cannot be read or is not one). When the arguments are rejected, the JSON has only the status and the error.

Using a source build? The [developer instructions](/en/docs/build-from-source/) include the equivalent Cargo command.

Time values accept seconds, `MM:SS.mmm`, or `HH:MM:SS.mmm`. The selected span becomes a new video starting at its own 00:00.

## On Linux without a GPU

On Linux, use Vulkan with --driver vulkan. For machines without a GPU, such as CI runners and cloud containers, install Mesa’s software Vulkan driver (lavapipe), which renders on the CPU. This works with a [source build](/en/docs/build-from-source/):

```sh title="Install software Vulkan"
sudo apt-get install -y mesa-vulkan-drivers
```

Software rendering is much slower than a GPU: with 4 CPU cores, 1080p compositions render at roughly 10 to 60 frames per second, depending on the scene. Check a single frame as a PNG before exporting the whole video. From the repository root:

```sh title="Export one frame"
cargo run -p celesta-exporter --release -- --react --driver vulkan packages/cli/examples/title.tsx --frame 0 frame.png
```

Minimal containers often have no fonts installed, and text cannot be drawn without at least one. Install some, such as `fonts-dejavu-core` (and `fonts-noto-cjk` for Japanese), or load font files with `<Font>`.

On a software renderer, the encoder converts RGB to YUV, which is faster there than converting on the renderer. `--render-quality draft` is quicker for checking timing, but not pixels. To export in a container without building Celesta yourself, build the [Linux container image](https://github.com/mika-f/Celesta/tree/main/packaging/linux) from the repository root, then run it from your project folder, which is mounted at `/work`. `--user` makes the output belong to you instead of root:

```sh title="Build the container image"
docker build -f packaging/linux/Dockerfile -t celesta-exporter .
```

```sh title="Export in a container"
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/work" celesta-exporter --react first-scene.tsx output.mp4
```

> [!NOTE]
> **Before you render**
> MP4 output uses H.264 video and AAC audio; MOV and WebM are described under Transparent video. Width and height must be non-zero, even numbers. Keep every referenced local media file available during export.
