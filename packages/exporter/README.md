# @celesta.studio/exporter

Export Celesta compositions from Node.js: videos, stills, and composition
information, with progress callbacks and cancellation. It runs
`celesta-exporter --json` for you, so a script or server does not build
arguments or parse output itself.

```ts
import { CelestaExportError, exportStills, exportVideo, getComposition } from '@celesta.studio/exporter';

const { composition } = await getComposition({ source: 'scene.tsx' });
console.log(`${composition.width}x${composition.height}, ${composition.duration}s`);

const controller = new AbortController();
const result = await exportVideo({
  source: 'scene.tsx',
  output: 'out/scene.mp4',
  props: { title: 'Launch' },
  overwrite: true,
  signal: controller.signal,
  onProgress(event) {
    if (event.event === 'rendering') console.log(`frame ${event.frame}/${event.total}`);
  },
});
console.log(result.outputs.map((output) => output.path));

await exportStills({ source: 'scene.tsx', output: 'out/thumb.jpg', frames: [90], quality: 85 });
```

The package has no dependencies. It is not published to npm: use it from
this workspace, or build it (`pnpm --dir packages/exporter run build`) and
install the folder (`npm install <celesta>/packages/exporter`).

## Functions

| Function | Does | Resolves with |
| --- | --- | --- |
| `exportVideo(options)` | Writes an `.mp4`, `.mov` or `.webm`, and any `subtitles` beside it. With `subtitles` and no `output`, writes only the subtitle files. | `ExportResult` |
| `exportStills(options)` | Writes `frames` (or every `every`-th frame) as `.png`, `.jpg`/`.jpeg` or `.webp`, or one `contactSheet`. | `ExportResult` |
| `getComposition(options)` | Reads the composition's size, frame rate and length, and a React entry's roots, without rendering (`--info`). | `CompositionInfo` |
| `listComponents(options?)` | Lists the installed media components: decoders, video and audio encoders, and muxers (`--list-components`). With `check: true`, each plugin is started and entries say whether they are `available`. Takes no source. | `ComponentInfo[]` |
| `describeComponent(id, options?)` | Asks one component (`<plugin>/<id>`) for its `settings` and capabilities (`--describe`). Takes no source. | `ComponentDescription` |

Every function takes the source options:

- `source`: a React entry (`.tsx`, `.ts`, `.jsx`, `.js`) or a JSON project
  (`.celesta.json`). Anything but a `.json` file is treated as a React
  entry; set `react` to say otherwise.
- `composition`, `project`, `props`, `propsFile`: the React entry's root
  composition, companion project, and project property values, as
  `--composition`, `--project`, `--props` and `--props-file`.

and the run options: `exporter` (see [Finding the exporter](#finding-the-exporter)),
`signal`, `onProgress`, `cwd` and `env`.

The rest mirror the exporter's flags, camelCased: `overwrite`,
`renderQuality`, `driver`, `from`/`to`, `loudness` (LUFS, `'streaming'` or
`'broadcast'`), `truePeak`, `peakHandling` for video; `frames`, `every`, `quality`,
`contactSheet` (`true` or `{ columns, tileWidth }`), `debugGuides` for
stills. The output's extension picks the video or still format; to choose
the components, a video takes:

- `muxer`: a muxer, such as `'ffmpeg/mp4'`.
- `videoEncoder`: `{ id: 'ffmpeg/libx264', options: { crf: 20, preset: 'slow' } }`;
  each option becomes `--video-option=key=value` and is checked against the
  component's settings (see `describeComponent`).
- `audioEncoder`: the same shape for audio, or `'none'` for a silent video.
- `preset`, `presetFile`: a named bundle of the above (`--preset`,
  `--preset-file`).

`from` and `to` are seconds (rounded to milliseconds) or timecode
strings such as `"01:30.5"`. The types in `dist/index.d.ts` document each
option, and [the exporter reference](../../skills/celesta/references/export.md)
explains what they do.

A video output reports its `muxer`, its `video` track (`component`, `codec`),
its `audio` track or `null`, `alpha`, and every file it wrote in `files`.

`ExportResult` is the exporter's `--json` result without `status`: the
`composition`, every written file in `outputs` with its absolute path and
frame numbers (a video with sound also has its mix's `loudness`:
`integratedLufs`, `truePeakDbtp`, and `gainDb` with a `loudness` target),
`warnings`, and `stats`. Read the paths from `outputs` rather
than working out file names.

## Progress

`onProgress` receives each `--json-progress` event as it happens:

| `event` | Fields |
| --- | --- |
| `compositions` | `compositionId`, `compositions` (React entries) |
| `composition` | `composition` (`width`, `height`, `fps`, `frames`, `duration`) |
| `output` | `muxer`, `muxerName`, `video` and `audio` (`component`, `name`, `settings` labels, or `audio: null`), sent before rendering |
| `rendering` | `frame` (counting from 1) of `total` |
| `mixingAudio`, `muxing` | none |
| `warning` | `message` |
| `wrote` | `output`, one of the result's `outputs` |

A callback that throws cancels the export, and the promise rejects with what
it threw.

## Cancellation

Aborting `signal` writes `cancel` to the exporter's stdin
(`--stdin-cancel`). The exporter stops before its next frame and removes the
partial video; stills it already wrote stay. The promise rejects with a
`CelestaExportError` whose `code` is `cancelled`. An exporter that has not
stopped within `CANCEL_TIMEOUT_MS` (10 seconds) is killed. An export that
finished before it noticed the cancellation resolves as usual.

The exporter's stdin stays open while it runs, so if your process exits, the
exporter sees it close and cancels too.

## Errors

A failed export rejects with a `CelestaExportError`. Its `code`, `message`
and `hint` are the exporter's `--json` error, so the
[error codes](../../skills/celesta/references/export.md#json-results) apply as
they are; `issues` lists project validation or property issues, and `report`
is the whole result (including any `outputs` written before the failure).
This package adds:

| `code` | Meaning |
| --- | --- |
| `usage` | Also raised before running anything for options that cannot work, such as `exportStills` with neither `frames` nor `every`. |
| `exporter_not_found` | The executable could not be started. |
| `exporter_failed` | The exporter ended without a result; `stderr` holds what it printed. |
| `cancelled` | Also raised for an already aborted `signal` or an exporter that had to be killed. |

## Finding the exporter

The exporter is found in this order; `resolveExporter()` returns what an
export would run:

1. The `exporter` option.
2. The `CELESTA_EXPORTER` environment variable (of `env`, when given).
3. An installed Celesta:
   - macOS: `/Applications/Celesta.app/Contents/MacOS/Celesta-export`, then
     `~/Applications/Celesta.app/Contents/MacOS/Celesta-export`;
   - Windows: `%LOCALAPPDATA%\Programs\Celesta\Celesta-export.exe`;
   - Linux: `/opt/celesta/celesta-exporter`, where the
     [container image](../../packaging/linux/Dockerfile) puts it.
4. `celesta-exporter` on `PATH`.

From a source checkout, build the exporter and point at it:

```sh
cargo build -p celesta-exporter --release
export CELESTA_EXPORTER="$PWD/target/release/celesta-exporter"
```

The exporter must support `--info`, `--json-progress` and `--stdin-cancel`.
An older one rejects them with a `usage` error whose `hint` says so; point
`CELESTA_EXPORTER` at a newer build.
