`Image`, `Video`, and `Audio` read files through `src`. Relative paths start from the composition file, so keep media beside it in your project folder.

`src` can also be an `http://` or `https://` URL. Native preview, export, and `preloadMedia()` download it into a local cache on first use. Later runs reuse the cached file without revalidation; use a new URL or clear the cache when its content changes.

```tsx file=examples/media.tsx title="media.tsx"
```

- An `Image` or `Video` is drawn at its own pixel size unless you give it `width` or `height`. One of them alone keeps the picture's proportions; with both, a `Video` is stretched to that box, and an `Image` is too unless `fit="contain"` or `"cover"` fits it into the box. Position it with `x`/`y` and anchors; `scale` and a `Fit` resize it as they do any layer. Images can be PNG, JPEG, WebP, GIF, or PNM.
- An animated GIF, APNG, or WebP plays on the enclosing sequence's clock, each frame for its own delay, and loops. `startFrom` and `playbackRate` work as on `Video`; `loop={false}` holds the last frame. `preloadMedia()` reports its length as `animation.durationSeconds`.
- `Video` and `Audio` accept `startFrom` (seconds into the file), `playbackRate`, `volume`, and `muted`. A `Video` plays its own soundtrack, so do not add an `Audio` with the same file to hear it; set `muted` to keep only the picture.
- `loop` on a `Video` or `Audio` starts the file over from its beginning when playback reaches its end, picture and sound together, instead of holding the last frame in silence.
- Media placed inside a `Sequence` starts when the sequence does and stops when it ends.
- `volume` and an audio `playbackRate` can be a number or keyframes. Keyframe times are seconds from the start of the enclosing sequence, written as `{ value, timescale }`.

frameKeyframes(keys, { fps, origin? }) builds volume, playbackRate, or pitch keyframes from frame numbers. Times are (frame - origin) / fps on the enclosing sequence’s clock; origin defaults to 0. Use it for volume changes that a fade does not cover, such as lowering music under a voice. The browser exporter supports keyframed volume and playback rate, but not pitch.

## Show media in a frame

`MediaFrame` from `@celesta/components` fits an image or a video into a frame of a given `width` and `height`, drawn by `Panel` (background, border over the media, rounded corners, shadow, and a `padding` mat), with an optional `caption` under it. `kind` (`"image"` or `"video"`) says which the media is; it is never guessed from the file name. `fit="contain"` (the default) shows all of it, centred; `"cover"` fills the frame, cropped. A `Video` has no `fit`, so `MediaFrame` scales a video to fit itself and needs its size as `sourceWidth` and `sourceHeight`: read them once with `preloadMedia()` in `prepare()`. `startFrom`, `playbackRate`, `volume`, `muted`, and `loop` reach the `Video` unchanged, so it starts with its `Sequence` and sounds as a bare `Video` would. Only the plain frame exists for now.

```tsx title="A clip in a rounded frame"
let clip: MediaInfo;
export async function prepare() { clip = await preloadMedia("./demo.mp4"); }

<MediaFrame kind="video" src={clip.src} sourceWidth={clip.video!.width} sourceHeight={clip.video!.height}
  width={960} height={540} fit="cover" cornerRadius={24} caption="App preview" />
```

## Fade in and out

`fadeIn` and `fadeOut` take seconds. The clip rises from silence over the first `fadeIn` seconds it is heard and falls back to silence over the last `fadeOut` seconds.

```tsx
<Sequence durationInFrames={300}>
  <Audio src="bgm.mp3" volume={0.6} fadeIn={1.5} fadeOut={3} />
</Sequence>
```

- The fades are timed from where the clip starts and stops being heard: the edges of its `Sequence`, or of an outer one that cuts it shorter. When the sequence gets longer or shorter, the fade-out moves with its end.
- Each fade multiplies `volume`, whether it is a number or keyframes. The fades do not replace it.
- The fades are linear in amplitude. No other curve is available yet.
- When `fadeIn` and `fadeOut` add up to more than the clip, they overlap and multiply, so the clip never reaches full volume.
- The fades are timed on the timeline, not on the file: `loop` does not repeat them, and `playbackRate` and `pitch` do not stretch them. When the file ends before the clip does, the fade-out falls after the sound has stopped.
- Only `Audio` takes these props, and in a project JSON file an `audio` item takes them as times. The soundtrack of a `Video` and the voices of a `Dialogue` do not. A negative value is an error. The browser exporter applies them too.

## Turn music down under voices

`duckGroup` puts a clip in a group, and `duck` turns a clip down while a clip of the groups it names plays. Dialogue voices are in the group `"voice"` already, so BGM under a dialogue needs only `duck`.

```tsx
<Audio src="bgm.mp3" volume={0.6} duck={{ under: 'voice' }} />
<Video src="game.mp4" duck={{ under: 'voice', to: { db: -12 }, release: 0.8 }} />
```

- `to` is the gain under the voices, as a multiplier from 0 to 1 or as `{ db }` at or below 0. It defaults to 0.3 and multiplies `volume` and the fades.
- `attack` is how many seconds the gain takes to come down, ending as the voice starts (0.15 by default). `release` is how many it takes to come back up after the voice ends (0.4 by default). With 0, the gain switches instantly, which can click on some material.
- Silences shorter than `holdGap` seconds (0.6 by default) stay ducked, so the music does not bob up between lines.
- A voice counts from its start until its file runs out or its `Sequence` ends, whichever comes first, so a subtitle held after the voice does not keep the music down. A looping voice counts for its whole sequence. Muted clips do not count.
- Preview, export, and the browser exporter play the same curve. In a project JSON file, `audio` and `video` items take `duckGroup` and `duck`, and `dialogue` items take `duckGroup`.

## Change the pitch

By default an `Audio`'s pitch follows its speed, like tape: `playbackRate={1.25}` is faster and higher. Two props separate them.

```tsx
<Audio src="bgm.mp3" playbackRate={1.25} preservePitch pitch={-2} />
```

- `pitch` transposes the clip in semitones. It defaults to 0 and ranges from −24 to 24; a fraction is cents, so `0.5` is 50 cents. It can be a number or keyframes, built with `frameKeyframes` like `volume`. Outside the range, the React path clamps to ±24 and a project JSON file fails validation.
- `preservePitch` defaults to `false`: `playbackRate` changes speed and pitch together, as before. When `true`, `playbackRate` changes only how long the clip takes, and `pitch` alone sets the pitch.
- The pitch you hear is multiplied by `2^(pitch / 12)`, times `playbackRate` unless `preservePitch` is set. `pitch={12}` is one octave up.
- Only `Audio` takes these props. The soundtrack of a `Video` and the voices of a `Dialogue` do not. The preview and the export sound the same, but the browser exporter in `@celesta/web` rejects a clip that uses them, with the error "Audio pitch and preservePitch are not supported by browser export yet."

## Size a scene to its media

The optional `prepare()` export runs once before rendering and may be `async`. In the example, `preloadMedia()` reads the clip’s length and `mediaDurationInFrames()` converts it to frames, so the end card always follows the footage.

## Move with the sound

`loadAudioSpectrum()` decodes an audio file, or a video’s soundtrack, in `prepare()` and analyzes all of it. `useAudioSpectrum()` then returns the current frame’s frequency bands and loudness, so bars, rings, and glows can follow the music or a voice.

```tsx file=examples/audio-spectrum.tsx title="audio-spectrum.tsx"
```

- `bands` lists every band from low to high frequency, each from 0 to 1. A band’s level is its mean FFT power in dB, relative to a full-scale sine, mapped from `minDecibels` (default −80) to `maxDecibels`. `maxDecibels` defaults to `'auto'`, the loudest band level anywhere in the file, so only the loudest moments reach 1 however loud the file was mastered. Pass a number, such as `maxDecibels: -6`, for a fixed ceiling, for example to give several files the same scale. `rms` is the root mean square of the samples, about 0.707 for a full-scale sine.
- `fftSize` sets the samples in each FFT (a power of two, default 2048). `bands` sets the number of bands (default 32), and `scale` how the range from `minFrequency` (20 Hz) to `maxFrequency` (16 kHz) is divided: `'log'` (the default) gives each octave the same number of bands, `'linear'` the same width in Hz, and `'mel'` follows perceived pitch.
- `smoothing` takes seconds, or `{ attack, release }` for rising and falling values separately: after that long a value has made about 63% of a change. The default 0 follows the sound exactly.
- The file is analyzed once, in order, every `hopSeconds` (1/240 s by default), with a Hann window centered on each moment, and frames read between two analyses interpolate them. The value of a frame therefore does not depend on the fps or on the order frames are rendered in, and is the same in the preview and in the export.
- The hook follows the enclosing `Sequence`’s clock, like `Audio`. Pass `startFrom` when the `Audio` has one. `playbackRate` is not followed. Before and after the file every value is 0.
- `spectrumAt(spectrum, seconds)` returns the same values outside a component, and `analyzeAudioSpectrum({ sampleRate, samples }, options)` analyzes samples you already have.
- The channels are averaged, and the file is analyzed at its own sample rate. Relative paths and URLs resolve as for `preloadMedia()`.
