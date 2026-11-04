import { Assets, Audio, Composition, Font, Scenes, Sequence } from '@celesta.studio/react';
import { Character } from '@celesta.studio/character';
import { CHAPTERS } from './chapters';
import { Hud } from './components/Hud';
import { Shutter } from './components/Shutter';
import { C, DW, FONT, FPS, H, S, W } from './constants';
import { JA_FONTS, LATIN_FONTS } from './fonts';
import { CHAPTER_AT, SCENES } from './timeline';
import { MOUTH, PSD, akane, poseLayers } from './voice';

// "Feature Tour." A 52-second tour of Celesta's features, cut to a 120 BPM
// score (one beat = 15 frames, one bar = 60 frames at 30 fps). Nine
// chapters, each a headline, a Japanese caption, the API that does it, and a
// live demo of it.
// Open this file in Celesta, or export it with:
//   Celesta-export --react examples/feature-tour/film.tsx feature-tour.mp4
//
//   timeline.tsx   every scene in order (defineScenes), played by <Scenes>, read by the HUD,
//                  and listed as the preview's chapters
//   scenes/        Open, Index (table of contents) and Outro
//   chapters/      the nine feature chapters: index.ts lists them, one demo per file
//   components/    Label (text), Swap, ChapterShell (shared chapter layout), Shutter, Hud
//   voice.ts       the dialogue chapter's PSD / voice assets and prepare()
//   fonts.ts       font URLs (Japanese faces are subset to the glyphs used)
//   constants.ts   palette, timing               helpers.ts     pad(), zero-padded counters

// Loads the PSD pose preset, the lip-sync track and the voice envelope before the first frame.
export { prepare } from './voice';

export default function Root() {
  return (
    <Composition width={W} height={H} fps={FPS} durationInFrames={SCENES.durationInFrames} chapters={SCENES}>
      <Assets>
        <Font src={LATIN_FONTS} />
        <Font src={JA_FONTS} />
        <Character ref={akane} name="Kotonoha Akane"
          portrait={{
            type: 'psd', src: PSD, layers: poseLayers, lipSync: {
              a: `${MOUTH}/あいうえお/*あ`, i: `${MOUTH}/あいうえお/*い`, u: `${MOUTH}/あいうえお/*う`,
              e: `${MOUTH}/あいうえお/*え`, o: `${MOUTH}/あいうえお/*お`, closed: `${MOUTH}/*-`,
            },
          }}
          subtitle={{
            x: 1400, y: 905, anchorX: 0.5, anchorY: 0.5, maxWidth: DW,
            style: {
              fontFamily: FONT.ja, fontWeight: 700, fontSize: 40, align: 'center',
              fill: { type: 'solid', color: C.paper },
              stroke: { paint: { type: 'solid', color: C.ink }, width: 8 },
            },
          }} />
      </Assets>

      <Scenes scenes={SCENES} />

      {/* Chapter 01 opens out of the index, so its cut has no shutter. */}
      {[S.index, ...CHAPTER_AT.slice(1)].map((cut, i) => {
        const into = CHAPTERS[i]?.bg;
        const color = cut === S.index ? C.paper : into === C.blue || CHAPTERS[i - 1]?.bg === C.blue ? C.paper : C.blue;
        return (
          <Sequence key={`cut-${cut}`} from={cut - 9} durationInFrames={20}><Shutter color={color} /></Sequence>
        );
      })}
      <Hud />

      {/* The score dips under the dialogue chapter's voice, a <Dialogue> line in "voice". */}
      <Audio src="./score.wav" duck={{ under: 'voice', attack: 0.4, release: 0.8 }} />
    </Composition>
  );
}
