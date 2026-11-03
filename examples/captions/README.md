# Captions

Captions from an SRT file, and the video's subtitles written back out as a
file. Twelve seconds at 30 fps, 1920 × 1080.

`captions.srt` is imported as text and read with `parseSubtitles()` into
frame-timed cues. `<CaptionTrack>` (from `@celesta.studio/components`) draws the
cue on each frame in a band, with two words of cue 2 emphasized by grapheme
range, and declares the cues for the exported file. Each cue's
`<Sequence>` shows its SRT block as `formatSubtitles()` writes it, beside a
timeline of every cue with the playhead. The file leaves 8–10 s empty for a
`<Dialogue>` line, whose `subtitle.render` draws the same band with a
one-cue `<CaptionTrack>`; it goes into the exported file too.

The composition is `film.tsx`. The fonts come from Google Fonts, so the
first export needs a network connection. From the repository root:

```sh
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" \
  --react examples/captions/film.tsx captions.mp4 --subtitles captions.srt
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" \
  --react examples/captions/film.tsx cut.mp4 --from 5 --to 10.5 --subtitles cut.vtt
```

The first writes five cues: the four of `captions.srt` and the dialogue
line at 00:00:08,000. The second starts its times at 5 s, so the cue that
began at 3 s is cut to `00:00:00.000 --> 00:00:01.000` and the last one to
`00:00:05.000 --> 00:00:05.500`.

The portrait comes from `../assets/dialogue-demo/portraits/`.
