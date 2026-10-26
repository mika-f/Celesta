Celesta is split into packages, like Remotion. The core APIs come from `@celesta/react`, and the Package column names the package for the rest. Random numbers, noise, and other math helpers come from `@celesta/math` (see [its guide](/en/docs/math/)), and syntax-highlighted code from `@celesta/code` (see [its guide](/en/docs/code/)). Celesta’s TypeScript declarations provide the complete prop types and editor completions.

**Layers**

| API | Package | Use it for |
| --- | --- | --- |
| `Composition` | `@celesta/react` | Set `width`, `height`, `fps`, and `durationInFrames`. Optional `lang` supplies the default text language, and `fontFallback="loaded"` falls back to `<Font>` fonts before system fonts. Return exactly one from your default export. |
| `Rect` | `@celesta/react` | A rectangle with `width`, `height`, and optional `fill`, `stroke`, `strokeWidth`, `cornerRadius`, and a dashed stroke with `dash`, `dashOffset`, and `cap`. |
| `Text` | `@celesta/react` | Text from string or number children, styled with `style` and wrapped with `maxWidth`. Optional `lang` selects its language; `style.lang` takes priority. See [Text & fonts](/en/docs/text-fonts/). |
| `Group` | `@celesta/react` | Applies a shared position, scale, rotation, skew, 3D rotation (`rotationX`, `rotationY`, `perspective`), and opacity to child layers, turning about its origin or the point `originX`/`originY` set. Optional `lang` sets their default text language, `fontFallback` their font fallback, `clip` a rectangle and `mask` any drawing to show them through. See [Shapes & layout](/en/docs/layout/). |
| `Mask` | `@celesta/react` | Sets how a `Group`’s mask is read: `mode="luminance"` and `invert`. |
| `Image` / `Video` / `Audio` | `@celesta/react` | Local files or HTTP(S) media through `src`. `Image` and `Video` take a display `width` and `height`; one alone keeps the aspect ratio. See [Images, video & sound](/en/docs/media/). |
| `defineShader` | `@celesta/shader` | Define a custom WGSL filter; call it with its parameters and pass the result to `shader`. |
| `brightness`, `contrast`, `saturation`, `hue`, `grayscale`, `invert`, `tint`, `vignette` | `@celesta/shader` | Color filters to place in a `shader` array among custom shaders. Every layer also takes them as props of the same names; see [Shapes & layout](/en/docs/layout/). |
| `pixelate`, `halftone`, `chromaticAberration`, `wave`, `noise`, `scanlines`, `zoomBlur`, `chromaKey` | `@celesta/shader` | Stylize effects and a chroma key to place in a `shader` array. Every layer also takes them as props of the same names; see [Shapes & layout](/en/docs/layout/). |
| `Font` | `@celesta/react` | Loads a font file or web-font stylesheet through `src` so `Text` can use the family. See [Text & fonts](/en/docs/text-fonts/). |

**Time and motion**

| API | Package | Use it for |
| --- | --- | --- |
| `Sequence` | `@celesta/react` | Places children at a frame offset with `from` and an optional `durationInFrames`. Optional `lang` sets their default text language, and `fontFallback` their font fallback. |
| `Loop` | `@celesta/react` | Repeats its children every `durationInFrames` frames from `from`, `times` times or until the enclosing window ends; their clock, video, and audio restart each cycle. |
| `FreezeFrame` | `@celesta/react` | Draws its children as the composition looked at an absolute `frame`, silently. |
| `MotionBlur` | `@celesta/react` | Averages its children over a shutter (`shutterAngle`, 180 by default) around the frame, at `samples` moments (8). |
| `Trail` | `@celesta/react` | Draws `copies` earlier, fainter copies of its children beneath them, each `lagInFrames` older. |
| `Transition` | `@celesta/transitions` | A fade, slide, or scale (or several at once) at the start or end of the enclosing sequence. |
| `Series` / `computeSeries` | `@celesta/react` | Scenes back to back, by length. See [Scenes, cues & beats](/en/docs/motion-toolkit/). |
| `defineScenes` / `Scenes` / `useScene()` / `sceneAt` | `@celesta/react` | Named chapters back to back, and the current one with its progress. |
| `<Composition chapters>` / `listChapters` | `@celesta/react` | Named spans the editor preview lists and seeks to, from a scene list, a dialogue plan, or plain spans. See [Scenes, cues & beats](/en/docs/motion-toolkit/). |
| `TransitionSeries` / `computeTransitionSeries` | `@celesta/transitions` | Scenes joined by cuts, cross-fades, slides, wipes, irises, clock wipes, zooms, dissolves, blurs, flips, or cubes, with the overlaps and total length computed. See [Scenes, cues & beats](/en/docs/motion-toolkit/). |
| `defineTransition` / `iris`, `clockWipe`, `zoom`, `dissolve`, `blur`, `flip`, `cube`, `crossfade`, `slide`, `wipe` | `@celesta/transitions` | Define a transition for `TransitionSeries` from a function, or set a built-in one’s options. |
| `Stagger` | `@celesta/react` | Starts each child a fixed number of frames after the previous one. |
| `progress` | `@celesta/react` | A clamped, eased 0–1 value for a span of frames. |
| `useBeat()` / `beatAt` | `@celesta/react` | Beats, bars, and a pulse for a tempo in BPM. |
| `useCue()` / `cueAt` | `@celesta/react` | The cue in effect from a list of `{ at, …data }`. |
| `TextReveal`, `useTypewriter()`, `useCountUp()` | `@celesta/text` | Masked line reveals, typing, and counting numbers. |
| `Camera` | `@celesta/layout` | Look at a point of a larger world, with zoom, rotation, and shake. |
| `Line` / `Polyline` / `Path` / `pointOnPolyline` | `@celesta/shapes` | Segments, curves, and filled shapes; polylines and paths can draw themselves on. |
| `pointOnPath` / `tangentOnPath` / `measurePath` / `getLength` / `getPointAtLength` / `getTangentAtLength` | `@celesta/shapes` | Points, directions, and lengths along path commands, curves included. |
| `interpolatePath` / `parseSvgPath` / `toSvgPath` | `@celesta/shapes` | Morphing between paths with different commands, and SVG path data to commands and back. |
| `Circle` / `Ellipse` / `Arrow` | `@celesta/shapes` | Diagram shapes: a circle is a rounded `Rect`, ellipses and arrows are `Path`s; circles and ellipses are placed like a `Rect`. |
| `Triangle` / `Polygon` / `Star` / `Pie` / `Heart` | `@celesta/shapes` | Closed shapes drawn as `Path`s, with optional rounded corners; pie angles animate. |
| `trianglePath` / `polygonPath` / `starPath` / `piePath` / `heartPath` | `@celesta/shapes` | The same shapes as path commands, to pass to a `Path` or reshape. |
| `interpolate` / `Easings` | `@celesta/react` | Map a frame to a value, with easing and extrapolation. |
| `interpolateColor` | `@celesta/react` | Map a frame to a `#RRGGBBAA` color, blending with premultiplied alpha. |
| `frameKeyframes` | `@celesta/react` | frameKeyframes(keys, { fps, origin? }) builds volume, playbackRate, or pitch keyframes from frame numbers. Times are (frame - origin) / fps on the enclosing sequence’s clock; origin defaults to 0. Use it for audio fades. The browser exporter supports keyframed volume and playback rate, but not pitch. |
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
| `importVoiceFolder` | `@celesta/character` | Read a folder of paired `.wav` + `.txt` voice-tool exports into dialogue lines; see [Character dialogue](/en/docs/dialogue/). |
| `matchVoiceLevels` / `analyzeVoiceLevel` / `scaleVolume` | `@celesta/character` | Measure local WAV voices in `prepare()` and multiply each line's volume toward a common speech level; see [Character dialogue](/en/docs/dialogue/). |
| `duckingVolume` / `speechIntervals` | `@celesta/character` | duckingVolume(plan, duck, { levels?, origin? }) builds a bed's volume that dips under a dialogue plan's voices, trimmed to the speech in a `matchVoiceLevels` report when given; speechIntervals(plan, { levels? }) returns the intervals in seconds. The curve is the one `<Audio duck>` plays. |
| `duckEnvelope` | `@celesta/media-utils` | duckEnvelope(intervals, duck, { origin? }) returns the ducking curve over intervals in seconds as volume keyframes, with `<Audio duck>`'s defaults; `origin` is in seconds (duckingVolume's is in frames), and `duck.volume` scales the curve. |
| `Dialogue` | `@celesta/character` | Shows a line as a subtitle, plays its `audio`, and changes the view’s expression while active, optionally at cue times within the line. |
| `expressionAt` | `@celesta/character` | The expression a line’s `expression` (a name or `{ at, expression }` cues) picks at a time into the line. |
| `loadLipSync` / `useLipSync` | `@celesta/character` | Generate mouth shapes from a WAV recording, and read the current shape. |
| `paginateSubtitles` | `@celesta/character` | Split long dialogue subtitles into timed pages from the measured text, aligned to speech timing through an explicit segment mapping or shared out approximately; lines also take `subtitlePages` and `emphasis` by hand. |
| `loadDialogueLipSync` / `spokenTextOf` | `@celesta/character` | Give planned lines waveform lip sync from their local WAVs and their `spokenText` (what the line says, apart from the subtitle `text`). |
| `loadPsdPreset` | `@celesta/character` | Load visible PSD layers from a PSDTool `.pfv` favorite. |

**Layout, data, and tools**

| API | Package | Use it for |
| --- | --- | --- |
| `TextBox` / `fitText` / `useFitText` / `measureText` / `useTextMetrics` | `@celesta/text` `@celesta/react` | Measure shaped text or choose a font size for a box; see [Text & fonts](/en/docs/text-fonts/). |
| `emphasisContent` / `resolveEmphasis` / `sliceEmphasis` / `graphemes` | `@celesta/text` | Emphasize part of a text given as data, by grapheme-cluster ranges with a color and weight, drawn as `<Span>`s. |
| `phrases` / `phraseBoundaries` | `@celesta/text` | Split Japanese into the phrases (文節) `lineBreak: 'phrase'` keeps on one line, with the same BudouX model. |
| `useTextMeasurer` | `@celesta/text` | Measure any number of texts during render, as `useTextMetrics()` measures one. |
| `resolveTimedRanges` / `timedRangeAt` | `@celesta/text` | Check things shown on half-open frame ranges (captions, subtitle pages) for gaps, overlaps, and range, and find the one on a frame. |
| `FrameChrome` | `@celesta/components` | Corner brackets, a title, the timecode, the current chapter, and a progress bar around the picture, laid out from the canvas size. |
| `PatternBackground` | `@celesta/components` | Dots, a grid, stripes, or marks of your own repeated over the canvas or an area, clipped to it, and moved with the frame. |
| `Panel` | `@celesta/components` | A box of a given size with a background, a border, rounded corners, and a shadow; its children lay out inside its padding, optionally clipped to it. |
| `Badge` | `@celesta/components` | A label on a `Panel` sized from its measured text, with an optional decoration before the text, round ends with `cornerRadius="pill"`, and wrapping at a `maxWidth`. |
| `Counter` / `StatCard` / `formatNumber` | `@celesta/components` | A counting number formatted with a fixed locale (`'en-US'` by default), decimals, a prefix or suffix, or a formatter; on its own, or on a card with a label and a note. |
| `BarChart` / `Ranking` | `@celesta/components` | Horizontal bars that grow in turn while their values count up from the same progress; `Ranking` adds ranks and states whether rows are sorted. |
| `Callout` | `@celesta/components` | A note box sized from its measured title and text, with a straight or elbowed line and an arrowhead to a target point in the same coordinates. See [Text effects, camera & lines](/en/docs/text-camera-lines/). |
| `CaptionTrack` | `@celesta/components` | Timed captions from data alone (`parseSubtitles()` cues or your own), the one on each frame on a band: wrapped to a line limit, with emphasis, fading in and out, or drawn by a `render` callback on the same cue clock. `highlight` lights the word being spoken (color, a band, growth) from word timings, and `paging` shows a cue a page at a time, by word count, lines, sentences, and pauses; Japanese splits by phrase. |
| `MediaFrame` | `@celesta/components` | An image or a video (`kind`) fitted into a framed `Panel`, contain or cover, with rounded corners, a border, and a caption; a video keeps its playback and sound. See [Images, video & sound](/en/docs/media/). |
| `Motion` | `@celesta/components` | One element that fades, moves, or scales in, holds exactly as written, and leaves, in the enclosing sequence's window or one of its own. For transitions between whole scenes, use `TransitionSeries`. |
| `TitleCard` | `@celesta/components` | A title screen: a chapter number, kicker, title, and subtitle in one block, placed left, centered, or right and top, middle, or bottom inside the canvas or a safe area. The title keeps its line breaks and shrinks to fit; the pieces enter one after another or together. |
| `LowerThird` | `@celesta/components` | A name and an optional role on a band sized to them, with an accent bar, in a corner of the canvas or a safe area. Long lines wrap and shrink to fit; the band slides in and the lines follow, and it leaves at the end of the enclosing sequence. |
| `TypewriterText` | `@celesta/components` | Text typed out with a line or block caret placed on the measured layout, in any font, across `\n` and wrapped lines. |
| `Center`, `SafeArea`, `Stack`, `Grid`, `Fit`, `LayoutBounds` | `@celesta/layout` | Arrange child layers. See [layout helpers](/en/docs/layout/). |
| `preloadMedia` / `mediaDurationInFrames` | `@celesta/media-utils` | Read a media file’s length and size in `prepare()`. |
| `loadAudioSpectrum` / `useAudioSpectrum` / `spectrumAt` | `@celesta/media-utils` | Analyze an audio file in `prepare()` and read each frame’s frequency bands and loudness. See [Images, video & sound](/en/docs/media/). |
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
