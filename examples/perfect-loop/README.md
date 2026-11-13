# PERFECT LOOP

A 16-second geometric loop: circles inside squares inside circles, opening
continuously toward the viewer. Vermilion, ultramarine, ivory, and ink; a
poster frame and a restrained original electronic score.

## Watch and edit

- `perfect-loop.mp4`: 1080 × 1350 (4:5), 60 fps, 16 seconds, H.264/AAC stereo.
- `film.tsx`: open with Celesta using **File → Open…**. Source edits reload automatically.
- `motion.ts`: palette, geometry, and the continuous zoom function.
- `poster.png`: the actual exported first frame.
- `make-score.py`: deterministic soundtrack generation using only Python's standard library.

All fonts and audio are local. Playback and export need no network access.
The generated WAV and MP4 are ignored by Git; regenerate the score after cloning.

## Reproduce

From the repository root, for the installed macOS app:

```sh
/Applications/Celesta.app/Contents/MacOS/Celesta --init examples/perfect-loop
python3 examples/perfect-loop/make-score.py
node skills/celesta/scripts/inspect.mjs examples/perfect-loop/film.tsx --frames 0,239,480,959
npx --yes --package typescript@5.9.3 tsc --noEmit -p examples/perfect-loop
/Applications/Celesta.app/Contents/MacOS/Celesta-export --json --react \
  --video-encoder ffmpeg-x264/libx264 --video-option crf=16 \
  examples/perfect-loop/film.tsx examples/perfect-loop/perfect-loop.mp4
```

The export uses x264's `crf`, so it needs the optional `ffmpeg-x264` plugin:
install the release download in your user plugin folder and allow it (in the
app, or with `--allow-plugin ffmpeg-x264`). In a source checkout, build it with
`node scripts/plugins/build.mts plugins/ffmpeg plugins/ffmpeg-x264` instead.

The initialization command refreshes the ignored `.celesta/` type
declarations without replacing the composition. For a built source checkout,
substitute `target/release/celesta-editor` for the app path and
`target/release/celesta-exporter` for the exporter path. To replace a
previously generated movie, add `--overwrite`.

## Loop and verification

Eight nested layers pass the camera in sixteen seconds. Shape and color
repeat every four layers; radius and rotation depend on relative depth.
The center and rotation modulation are periodic. The hypothetical frame
960 is therefore the same picture as frame 0, with matching velocity.
The export contains frames 0–959, so the repeated endpoint adds no pause.

The score is 32 beats at 120 BPM. Note releases and echoes wrap around the
sample buffer rather than being cut off. Continuous pad frequencies use
integer cycles across the loop, with no introductory or ending fade.

Verified with the installed Celesta 0.5.0 exporter:

- TypeScript and selected-frame scene evaluation pass.
- Ten rendered contact-sheet frames and the full-size poster were visually inspected; no export warnings.
- Exported frames 0 and hypothetical 960 have identical decoded RGBA checksums.
- MP4 has exactly 960 video frames and 16-second video and audio streams.
- WAV peaks at −2.05 dBFS without clipping. The loop boundary changes by
  22 PCM units, below ordinary within-score sample changes.

Audio structure and sample continuity were checked programmatically; the
score was not auditioned. Gapless audio playback also depends on the player
honoring AAC priming information.

## Assets

Visuals and music are original procedural work, with no external footage,
samples, or image generation. Fonts are reused from the repository's
AFTERIMAGE example under the SIL Open Font License. Full licenses are in
`assets/fonts/`: Bebas Neue by Ryoichi Tsunekawa and IBM Plex Mono by IBM.
