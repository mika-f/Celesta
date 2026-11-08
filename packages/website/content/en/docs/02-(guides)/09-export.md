## In your browser

Open the [web editor](/en/#playground), change or open a TSX composition, and choose **Export MP4**. The source, preview, and export use Celesta’s React scene evaluator. Add local media files by name with **Add media**. The browser needs H.264 WebCodecs support; audio also needs AAC encoding. The browser preview is silent, while the exported MP4 includes constant-rate audio clips. For text measurement/fitting (TextBox, fitText, useFitText, measureText, useTextMetrics), PSDs, project timelines, `prepare()`, third-party imports, animated audio automation, and native-renderer output, use the installed app or CLI.

## From the app

Choose **Export…** and select a destination. Name it `.mp4` for H.264 video, or `.mov` or `.webm` for transparent video (see Transparent video below); any other extension becomes `.mp4`. For a section of the composition, press `I` and `O` to mark the start and end. The status bar shows progress; **Cancel export** stops the job, along with any plugin it is waiting on. To bring the soundtrack to a loudness target, choose **Loudness** next to **Export…** first: **Off**, **Streaming (-14 LUFS)**, or **Broadcast (-23 LUFS)**. The choice stays until you quit the app. When the export finishes, its notice shows the soundtrack's loudness (see Loudness for streaming and broadcast below). The warnings and notes the export reported, such as a font fallback or a plugin's note on a frame you never previewed, are logged as one entry in the **Problems** tab, each once and in the order they came, as the command line prints them.

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

Add `--overwrite` to replace an existing output file. Use `--from` and `--to` to select a time range:

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

## Encoding quality and speed

MP4 uses libx264 with --preset medium and --crf 18 by default. Faster presets such as veryfast reduce encoding time but usually increase file size at similar quality. CRF accepts 0–51; lower values give higher quality and larger files. --color-conversion accepts auto, gpu, or encoder. --render-quality draft speeds up timing checks by skipping scaled-text re-rasterization; use final (the default) to judge the finished image. --bit-depth 10 encodes 10-bit H.264 (High 10), which keeps dark gradients smoother but plays on fewer devices; the default is 8.

::::tabs[Exporter platform]{default="macOS"}
:::tab[macOS]

```sh title="Export with a faster encoder preset"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --preset veryfast --crf 18 first-scene.tsx draft.mp4
```

:::
:::tab[Windows]

```sh title="Export with a faster encoder preset"
& "C:\path\to\Celesta\Celesta-export.exe" --react --preset veryfast --crf 18 first-scene.tsx draft.mp4
```

:::
::::

## Videos that play on repeat

Shorts and TikTok play a video on repeat. When its last frame runs into its first, add `--loop` so the sound does too. An AAC encoder starts from silence, so without it a hit on the first frame, such as a kick, comes out smeared for about 10 ms, and the player repeats that on every pass. With `--loop`, the encoder first takes the end of the soundtrack, and the MP4's edit list hides that lead-in: the sound still lasts exactly as long as the video. The soundtrack itself must loop, with anything that rings past the end mixed into the start. `--loop` applies to MP4 only; a `.mov`'s PCM audio loops without it.

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

The loudness is measured and set on the soundtrack before it is encoded. A `.mov`'s PCM keeps it exactly. AAC (`.mp4`) and Opus (`.webm`) move the loudness by about a tenth of an LU and the true peak by up to about half a dB either way, so the -1 dBTP default can read about -0.5 dBTP in the file; give `--true-peak -2` for more headroom. In the app, **Loudness** next to **Export…** sets the streaming or broadcast target with the -1 dBTP limiter. For another target, ceiling, or `--peak-handling lower`, use the command line, the MCP `export_video` tool, or `@celesta.studio/exporter`.

## Transparent video

The file extension picks the video format. A `.mp4` file is H.264 with AAC audio, as above. A `.mov` file is ProRes 4444 (`yuva444p10le`) with 24-bit PCM audio, and a `.webm` file is VP9 with alpha (`yuva420p`, CRF 20) with 48 kHz Opus audio. In the app, choose the extension in the save dialog; there is no separate format menu. On the command line, run the same command as above with an output such as `overlay.mov` or `overlay.webm`. `--output-format mp4|mov|webm` states the format explicitly; one that disagrees with the extension is a usage error.

Only areas nothing draws on are transparent, so a layer that fills the whole frame, such as a full-frame background, makes the output opaque. The alpha is straight (unassociated), and colors are converted and tagged as BT.709 (limited range). `--preset`, `--crf`, `--bit-depth`, `--color-conversion`, and `--loop` apply to MP4 only and are usage errors with `.mov` or `.webm`.

Celesta encodes through its [media plugin](/en/docs/plugins/#media-plugins). If the plugin's FFmpeg build lacks the encoder for the chosen format, the export fails with the error code `encoder_unavailable`; without a media plugin, it fails with `no_codec`.

## Still frames and contact sheets

--frame / --frames selects zero-based frame numbers. Multiple frames create names such as check-000090.png. --every N selects every Nth frame of the --from/--to range (or the whole composition) and its last frame. Add --contact-sheet to place them in one labelled grid; --columns and --tile-width control its layout. Explicit frame numbers cannot be combined with --every, --from, or --to.

The file extension picks the image format: `.png` is lossless PNG, `.jpg` or `.jpeg` is JPEG, and `.webp` is lossy WebP. JPEG and WebP are smaller, which suits thumbnails; `--quality` sets their quality from 1 to 100 (default 90), and PNG does not take it. Frames are drawn over the same opaque background as an MP4, so a JPEG, which has no alpha channel, loses nothing. WebP holds at most 16383 pixels on each side, so a larger contact sheet is rejected before rendering.

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

Both the editor and exporter accept --driver auto (default), vulkan, dx12 (Windows), metal (macOS), or gl. Auto chooses an available adapter in wgpu’s backend order: Vulkan, Metal, DX12, then OpenGL. A specific driver must be available in the build and supported by the machine. This chooses the rendering API; MP4 encoding still uses libx264.

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

For scripts and AI agents, add `--json`. The exporter then prints no progress, only one line of JSON when it finishes: the status, every file it wrote, warnings, the composition's size, fps and frame count once the exporter has loaded it, and on failure an error code and message. Each video entry in the list of written files has `"format"` (`mp4`, `mov`, or `webm`) and `"alpha"` (`true` or `false`), and a video with sound has `"loudness"`: `"integratedLufs"`, `"truePeakDbtp"`, and with `--loudness` the `"gainDb"` it applied. When the arguments are rejected, the JSON has only the status and the error.

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

On a software renderer, `--color-conversion auto` leaves the RGB-to-YUV conversion to the encoder. `--render-quality draft` is quicker for checking timing, but not pixels. To export in a container without building Celesta yourself, build the [Linux container image](https://github.com/mika-f/Celesta/tree/main/packaging/linux) from the repository root, then run it from your project folder, which is mounted at `/work`. `--user` makes the output belong to you instead of root:

```sh title="Build the container image"
docker build -f packaging/linux/Dockerfile -t celesta-exporter .
```

```sh title="Export in a container"
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/work" celesta-exporter --react first-scene.tsx output.mp4
```

> [!NOTE]
> **Before you render**
> MP4 output uses H.264 video and AAC audio; MOV and WebM are described under Transparent video. Width and height must be non-zero, even numbers. Keep every referenced local media file available during export.
