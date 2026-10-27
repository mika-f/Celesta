import { Audio, Group, Rect, Sequence, Subtitles, Text, interpolate, useCurrentFrame } from '@celesta/react';
import { FrameChrome } from '@celesta/components';
import { Path } from '@celesta/shapes';
import { C, FONT, H, W, solid } from '../components/theme';
import { CAPTIONS, CAPTION_LANGUAGES } from './captions';
import { CHAPTERS } from './chapters';
import { CARDS, More } from './More';
import { BED } from './mix';
import { Open } from './open';
import { Outro } from './outro';
import { LANDSCAPE, LayoutContext } from './layout';
import type { Layout } from './layout';
import { Program } from './program';
import { ShortProgram } from './ShortProgram';
import { SCORE_SRC } from './score';
import { VOICE_TEXTS } from './strings';
import { DeltaTheme } from './theme';
import { CHAPTER_LENGTH, FEATURES, MORE_AT, MORE_CARDS, MUSIC_BAR, OPEN_LENGTH, OUTRO_AT, SCENE_LIST, TITLES, chapterAt } from './timeline';
import { voices } from './voices';

// The release as a broadcast. The open lights a control room's
// multiviewer, the twelve features its sources, and dives into the program
// monitor; each feature then goes to air as a program with its lower third
// and a caption lit word by word, cut to the next by a stinger on the
// downbeat; MORE runs twelve cards; the outro pulls back out to the wall and
// goes off air.

if (CHAPTERS.length !== FEATURES || CARDS.length !== MORE_CARDS || CHAPTERS.some((chapter, i) => chapter.title !== TITLES[i])) {
  throw new Error('v0.9.0/timeline.ts does not match chapters.tsx and More.tsx');
}

/** The whole wall, off air: the poster. */
export const POSTER_AT = OUTRO_AT + 4 * MUSIC_BAR - 1;

/** The picture, laid out for the film (16:9) or the short (9:16). */
export function Reel({ layout = LANDSCAPE }: { layout?: Layout }) {
  const frame = useCurrentFrame();
  const Page = layout.portrait ? ShortProgram : Program;
  return (
    <LayoutContext.Provider value={layout}>
      <DeltaTheme>
        {frame < OPEN_LENGTH ? <Open /> : null}
        {CHAPTERS.map((chapter, i) => (
          <Sequence key={chapter.title} from={chapterAt(i)} durationInFrames={CHAPTER_LENGTH}>
            <Page chapter={chapter} index={i} />
          </Sequence>
        ))}
        <More />
        {frame >= OUTRO_AT ? <Outro /> : null}
        {layout.portrait ? (
          // The film's stinger, scaled to the portrait frame's height and centred.
          <Group x={-(W * (layout.height / H) - layout.width) / 2} scale={layout.height / H}><Stinger /></Group>
        ) : <Stinger />}
      </DeltaTheme>
    </LayoutContext.Provider>
  );
}

/** The cuts between programs: an orange band crossing the frame, covering it on the downbeat. */
const CUTS = [...Array.from({ length: FEATURES - 1 }, (_, i) => chapterAt(i + 1)), MORE_AT, OUTRO_AT];
const STING = 7;

function Stinger() {
  const frame = useCurrentFrame();
  const cut = CUTS.find((c) => Math.abs(frame - c) <= STING);
  if (cut === undefined) return null;
  const x = interpolate(frame - cut, [-STING, STING], [-2800, 2400]);
  const band = (left: number, width: number, fill: string) => (
    <Path points={[[left, 0], [left + width, 0], [left + width - 320, H], [left - 320, H]]} closed fill={fill} />
  );
  return (
    <Group>
      {band(x + 2560, 70, C.ink)}
      {band(x, 2500, C.orange)}
      {band(x - 60, 40, C.paper)}
    </Group>
  );
}

/** Over the programs: corner marks, the timecode, the chapter by name, progress, and the ON AIR lamp. */
export function Chrome() {
  const frame = useCurrentFrame();
  const shown = Math.min(
    interpolate(frame, [OPEN_LENGTH, OPEN_LENGTH + 12], [0, 1], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' }),
    interpolate(frame, [OUTRO_AT + MUSIC_BAR - 12, OUTRO_AT + MUSIC_BAR], [1, 0], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' }),
  );
  if (shown <= 0) return null;
  return (
    <DeltaTheme>
      <Group opacity={shown}>
        <FrameChrome margin={44} corners color={C.paper} blendMode="difference"
          fontFamily={['JetBrains Mono', 'Noto Sans JP']} fontSize={18}
          title="CELESTA 0.9.0" subtitle="/ ON AIR" timecode timecodeHours={false}
          chapters scenes={SCENE_LIST} formatChapter={(scene) => (scene.index === 0 || scene.index > FEATURES + 1
            ? scene.name : `${scene.index <= FEATURES ? `Δ${String(scene.index).padStart(2, '0')}` : '+'}  ${scene.name}`)}
          caption="1920×1080 · 30 FPS · 100 BPM" progress />
        <Group x={W / 2} y={60}>
          <Rect x={-74} y={-22} width={148} height={44} cornerRadius={4} fill="tally" />
          <Text anchorX={0.5} anchorY={0.5} y={2} style={{ fontFamily: FONT.display, fontSize: 36, fill: solid(C.paper) }}>ON AIR</Text>
        </Group>
      </Group>
    </DeltaTheme>
  );
}

/**
 * Every language at once; --subtitles 'delta.{lang}.srt' writes one file
 * each. The Japanese file also carries what the voices say, over their
 * chapters' captions: DIALOGUE's pages come from its <DialogueSeries>, the
 * other two lines from here.
 */
export function Captions() {
  const { captions, ducking } = voices();
  const spoken = [captions, ducking].map((v) => ({ from: v.from, durationInFrames: v.durationInFrames, text: v.line.text }));
  return (
    <>
      {CAPTION_LANGUAGES.map((lang) => <Subtitles key={lang} lang={lang} cues={CAPTIONS[lang]} />)}
      <Subtitles lang="ja" cues={spoken} />
    </>
  );
}

/**
 * The sound: the score, faded and ducked under the voices, and the two
 * voice lines CAPTIONS and DUCKING play. DIALOGUE's line plays through its
 * <DialogueSeries>, in the "voice" group by default.
 */
export function Sound() {
  const { captions, dialogue, ducking } = voices();
  if ([captions, dialogue, ducking].some((v, i) => v.line.text !== VOICE_TEXTS[i])) {
    throw new Error('v0.9.0/strings.ts VOICE_TEXTS does not match the voice folder');
  }
  return (
    <>
      <Audio src={SCORE_SRC} fadeIn={BED.fadeIn} fadeOut={BED.fadeOut} duck={BED.duck} />
      {[captions, ducking].map((voice) => (
        <Sequence key={voice.role} from={voice.from} durationInFrames={voice.durationInFrames}>
          <Audio src={voice.line.audio} volume={voice.line.volume} duckGroup="voice" />
        </Sequence>
      ))}
    </>
  );
}
