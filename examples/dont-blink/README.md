# DON'T BLINK

An original 18-second kinetic typography film. Warm white, near-black, and
signal red; oversized Bebas Neue lettering inside a restrained editorial
frame. Five horizontal slices turn BLINK into solid strips, regroup, and
uncover LOOK. Its two O counters briefly look back and blink. AGAIN expands
beyond the frame, splits apart, and reveals a small AGAIN inside itself.
An eyelid-like closure opens on a quiet YOU, followed by a red BLINKED. stamp.

The story reads with sound off. The original mechanical sound design uses
short clicks, pitched scrapes, low impacts, and deliberate silence from
14.3 to 17 seconds. No external footage, samples, shaders, blur, or glow.

## Deliverables

- `dont-blink.mp4`: 1080 × 1920, 9:16, 30 fps, 540 frames, exactly 18 seconds;
  H.264 video and 48 kHz stereo AAC audio.
- `film.tsx`: entry point; open using Celesta's **File → Open…**.
- `scenes.tsx`: six named scenes and their deterministic choreography.
- `type.tsx`: measured typography, clipped strips, geometric bars, and captions.
- `motion.ts`: palette, dimensions, easing helpers, and scene timings.
- `assets/fonts/`: local Bebas Neue and IBM Plex Mono fonts and their OFL licenses,
  reused from the repository's AFTERIMAGE example.
- `assets/score.wav` and `make-score.py`: sound design and its dependency-free,
  seeded Python generator.
- `rendered/frame-*.png`: 14 full-resolution representative PNGs.
- `rendered/contact-sheet.png`: all requested representative frames.
- `rendered/transitions.png`: 52 additional samples around complex transitions.
- `rendered/*.json`: exporter reports and final `ffprobe` metadata.

The generated WAV, MP4, PNGs, and reports are present locally and ignored by
Git. Regenerate them with the commands below after cloning. Fonts are tracked;
preview and export need no network access once TypeScript tooling is available.

## Commands used

Run from the repository root. These use the installed macOS Celesta 0.5.0
executables; substitute source-built executables on another setup.
Initialization refreshes `.celesta/` declarations and preserves existing source.

```sh
/Applications/Celesta.app/Contents/MacOS/Celesta --init examples/dont-blink
python3 examples/dont-blink/make-score.py
npx --yes --package typescript@5.9.3 tsc --noEmit -p examples/dont-blink
mkdir -p examples/dont-blink/rendered

/Applications/Celesta.app/Contents/MacOS/Celesta-export --json --overwrite --react \
  examples/dont-blink/film.tsx \
  --frames 0,15,30,60,90,120,180,240,300,360,420,480,510,539 \
  examples/dont-blink/rendered/frame.png \
  > examples/dont-blink/rendered/frames.json

/Applications/Celesta.app/Contents/MacOS/Celesta-export --json --overwrite --react \
  examples/dont-blink/film.tsx \
  --frames 0,15,30,60,90,120,180,240,300,360,420,480,510,539 \
  --contact-sheet --columns 5 --tile-width 270 \
  examples/dont-blink/rendered/contact-sheet.png \
  > examples/dont-blink/rendered/contact-sheet.json

/Applications/Celesta.app/Contents/MacOS/Celesta-export --json --overwrite --react \
  examples/dont-blink/film.tsx \
  --frames 58,59,60,61,126,127,128,129,154,155,156,157,174,179,180,181,209,222,231,234,240,260,279,280,281,298,299,300,301,341,342,355,359,377,389,397,398,406,412,413,414,415,419,420,421,431,509,510,511,517,519,539 \
  --contact-sheet --columns 8 --tile-width 240 \
  examples/dont-blink/rendered/transitions.png \
  > examples/dont-blink/rendered/transitions.json

/Applications/Celesta.app/Contents/MacOS/Celesta-export --json --overwrite \
  --react --crf 16 examples/dont-blink/film.tsx \
  examples/dont-blink/dont-blink.mp4 \
  > examples/dont-blink/rendered/export.json

ffprobe -v error -count_frames \
  -show_entries format=duration,size:stream=codec_name,codec_type,width,height,r_frame_rate,nb_frames,nb_read_frames,duration,sample_rate,channels \
  -of json examples/dont-blink/dont-blink.mp4 \
  > examples/dont-blink/rendered/ffprobe.json
ffmpeg -v error -i examples/dont-blink/dont-blink.mp4 -f null -
```

The native scene inspection command was also attempted:

```sh
node skills/celesta/scripts/inspect.mjs examples/dont-blink/film.tsx \
  --frames 0,15,30,60,90,120,180,240,300,360,420,480,510,539 \
  --native /Applications/Celesta.app/Contents/MacOS/Celesta-export
```

## Verification and limits

- TypeScript passes. Every selected PNG and the complete MP4 export succeeds
  with `warnings: []`; all output paths were checked to exist.
- Both contact sheets and full-size frames were visually inspected. Iteration
  removed clip-edge seams during holds, preserved rounded-letter overshoot,
  hid outgoing DON'T completely before the next scene, and joined the
  climax to the YOU reveal with a closing aperture.
- `ffprobe -count_frames` confirms all 540 frames decode, 1080 × 1920,
  30/1 fps, and 18.000-second video and audio streams. A complete FFmpeg
  video/audio decode reports no errors.
- WAV peak is −2.05 dBFS with no clipped samples; samples from 14.3 to
  17 seconds are exactly silent.
- The final MP4 was opened in QuickTime. Playback was observed advancing
  through 6.26 and 17.02 seconds; a screenshot during the punchline reveal
  was inspected. This was sampled playback verification, not a continuous
  real-time review of every movement. The sound was not auditioned.
- Native `inspect.mjs` could not complete: this installed exporter rejects
  `--inspect`. `useTextMetrics` requires native inspection, so the pure Node
  inspector cannot replace it. Actual native PNG/MP4 rendering evaluated the
  composition instead. No changes to Celesta core were needed.
- These are clipped-text and geometric-overlay transformations, not true
  glyph-outline morphing. The ending deliberately holds the punchline and
  does not form a seamless loop. Important words stay inside x=84…884 and
  y≈482…1304; intentional oversized fragments may leave that area during
  the climax. Platform-specific social UI overlays were not tested.
