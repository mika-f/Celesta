Celesta is split into packages, like Remotion. The core APIs come from `@celesta/react`, and the Package column names the package for the rest. Random numbers, noise, and other math helpers come from `@celesta/math` (see [its guide](/en/docs/math/)), and syntax-highlighted code from `@celesta/code` (see [its guide](/en/docs/code/)). Celesta’s TypeScript declarations provide the complete prop types and editor completions.

**Layers**

| API | Package | Use it for |
| --- | --- | --- |
| `Composition` | `@celesta/react` | Set `width`, `height`, `fps`, and `durationInFrames`. Optional `lang` supplies the default text language. Return exactly one from your default export. |
| `Rect` | `@celesta/react` | A rectangle with `width`, `height`, and optional `fill`, `stroke`, `strokeWidth`, `cornerRadius`, and a dashed stroke with `dash`, `dashOffset`, and `cap`. |
| `Text` | `@celesta/react` | Text from string or number children, styled with `style` and wrapped with `maxWidth`. Optional `lang` selects its language; `style.lang` takes priority. See [Text & fonts](/en/docs/text-fonts/). |
| `Group` | `@celesta/react` | Applies a shared position, scale, rotation, and opacity to child layers. Optional `lang` sets their default text language, `clip` a rectangle and `mask` any drawing to show them through. See [Shapes & layout](/en/docs/layout/). |
| `Mask` | `@celesta/react` | Sets how a `Group`’s mask is read: `mode="luminance"` and `invert`. |
| `Image` / `Video` / `Audio` | `@celesta/react` | Local files or HTTP(S) media through `src`. See [Images, video & sound](/en/docs/media/). |
| `defineShader` | `@celesta/shader` | Define a custom WGSL filter; call it with its parameters and pass the result to `shader`. |
| `brightness`, `contrast`, `saturation`, `hue`, `grayscale`, `invert`, `tint`, `vignette` | `@celesta/shader` | Color filters to place in a `shader` array among custom shaders. Every layer also takes them as props of the same names; see [Shapes & layout](/en/docs/layout/). |
| `Font` | `@celesta/react` | Loads a font file or web-font stylesheet through `src` so `Text` can use the family. See [Text & fonts](/en/docs/text-fonts/). |

**Time and motion**

| API | Package | Use it for |
| --- | --- | --- |
| `Sequence` | `@celesta/react` | Places children at a frame offset with `from` and an optional `durationInFrames`. Optional `lang` sets their default text language. |
| `FreezeFrame` | `@celesta/react` | Draws its children as the composition looked at an absolute `frame`, silently. |
| `Transition` | `@celesta/transitions` | A fade, slide, or scale (or several at once) at the start or end of the enclosing sequence. |
| `Series` / `computeSeries` | `@celesta/react` | Scenes back to back, by length. See [Scenes, cues & beats](/en/docs/motion-toolkit/). |
| `TransitionSeries` / `computeTransitionSeries` | `@celesta/transitions` | Scenes joined by cuts, cross-fades, slides, or wipes, with the overlaps and total length computed. See [Scenes, cues & beats](/en/docs/motion-toolkit/). |
| `Stagger` | `@celesta/react` | Starts each child a fixed number of frames after the previous one. |
| `progress` | `@celesta/react` | A clamped, eased 0–1 value for a span of frames. |
| `useBeat()` / `beatAt` | `@celesta/react` | Beats, bars, and a pulse for a tempo in BPM. |
| `useCue()` / `cueAt` | `@celesta/react` | The cue in effect from a list of `{ at, …data }`. |
| `TextReveal`, `useTypewriter()`, `useCountUp()` | `@celesta/text` | Masked line reveals, typing, and counting numbers. |
| `Camera` | `@celesta/layout` | Look at a point of a larger world, with zoom, rotation, and shake. |
| `Line` / `Polyline` / `Path` / `pointOnPolyline` | `@celesta/shapes` | Segments, curves, and filled shapes; polylines can draw themselves on. |
| `Circle` / `Ellipse` / `Arrow` | `@celesta/shapes` | Diagram shapes: a circle is a rounded `Rect`, ellipses and arrows are `Path`s; circles and ellipses are placed like a `Rect`. |
| `Triangle` / `Polygon` / `Star` / `Pie` / `Heart` | `@celesta/shapes` | Closed shapes drawn as `Path`s, with optional rounded corners; pie angles animate. |
| `trianglePath` / `polygonPath` / `starPath` / `piePath` / `heartPath` | `@celesta/shapes` | The same shapes as path commands, to pass to a `Path` or reshape. |
| `interpolate` / `Easings` | `@celesta/react` | Map a frame to a value, with easing and extrapolation. |
| `interpolateColor` | `@celesta/react` | Map a frame to a `#RRGGBBAA` color, blending with premultiplied alpha. |
| `frameKeyframes` | `@celesta/react` | frameKeyframes(keys, { fps, origin? }) builds volume or playbackRate keyframes from frame numbers. Times are (frame - origin) / fps on the enclosing sequence’s clock; origin defaults to 0. Use it for audio fades. The browser exporter only supports constant volume and playback rate. |
| `spring` | `@celesta/react` | A physics-based value that settles from 0 to 1. |
| `useCurrentFrame()` / `useCurrentTime()` | `@celesta/react` | The current frame, or the exact time, local to an enclosing sequence. |
| `useVideoConfig()` | `@celesta/react` | The composition’s (or sequence’s) dimensions, frame rate, and duration. |
| `timecodeToFrame()` / `frameToTimecode()` | `@celesta/react` | Convert a `MM:SS.mmm`-style timecode to a frame number, and format a frame as `HH:MM:SS:FF`. |

**Characters**

| API | Package | Use it for |
| --- | --- | --- |
| `Assets` / `Character` | `@celesta/react` `@celesta/character` | Declare a character’s portrait, expressions, lip-sync mouths, and subtitle style. |
| `CharacterView` | `@celesta/character` | Places a character’s portrait. Accepts `expression`, `mouth`, and `lipSync`. |
| `DialogueSeries` / `planDialogue` | `@celesta/character` | Plan voice-length timing and render a sequence of dialogue lines; see [Character dialogue](/en/docs/dialogue/). |
| `Dialogue` | `@celesta/character` | Shows a line as a subtitle, plays its `audio`, and changes the view’s expression while active. |
| `loadLipSync` / `useLipSync` | `@celesta/character` | Generate mouth shapes from a WAV recording, and read the current shape. |
| `loadPsdPreset` | `@celesta/character` | Load visible PSD layers from a PSDTool `.pfv` favorite. |

**Layout, data, and tools**

| API | Package | Use it for |
| --- | --- | --- |
| `TextBox` / `fitText` / `useFitText` / `measureText` / `useTextMetrics` | `@celesta/text` `@celesta/react` | Measure shaped text or choose a font size for a box; see [Text & fonts](/en/docs/text-fonts/). |
| `Center`, `SafeArea`, `Stack`, `Grid`, `Fit` | `@celesta/layout` | Arrange child layers. See [layout helpers](/en/docs/layout/). |
| `preloadMedia` / `mediaDurationInFrames` | `@celesta/media-utils` | Read a media file’s length and size in `prepare()`. |
| `parseSubtitles` / `formatSubtitles` / `Subtitles` | `@celesta/react` | Read SRT and WebVTT files into frame-timed cues and write them out. `Subtitles` declares the cues `--subtitles` writes. See [Export](/en/docs/export/). |
| `DebugOverlay` / `DebugBounds` / `useDebugGuides()` / `useIsPreview()` | `@celesta/debug` `@celesta/react` | Debug guides drawn in the preview and in PNG exports with --debug-guides, never in a video. |

## Legacy JSON project APIs

The following APIs remain for compatibility with existing JSON timelines. New compositions can use React components and ordinary props directly.

**Legacy JSON project APIs**

| API | Package | Use it for |
| --- | --- | --- |
| `loadProject`, `ProjectProvider`, `useProjectProperty` | `@celesta/project` | Read values from a JSON project. |
| `ProjectTimeline` / `ProjectTrack` | `@celesta/project` | Draw a companion JSON project’s timeline, or one of its tracks. |
| `registerComponent` / `defineProjectProperties` | `@celesta/react` | Make components and properties available to JSON projects and the Inspector. |
