import { Audio, Group, Rect, Sequence, Subtitles, Text, interpolate, useCurrentFrame } from '@celesta.studio/react';
import { FrameChrome } from '@celesta.studio/components';
import { Delta } from '../components/Delta';
import { C, FONT, H, W, solid } from '../components/theme';
import { CAPTIONS, CAPTION_LANGUAGES } from './captions';
import { CHAPTERS } from './chapters';
import { CARDS, More } from './More';
import { Open } from './open';
import { Outro } from './outro';
import { Program } from './program';
import { SCORE } from './score';
import { CABLE, DeltaTheme } from './theme';
import { BAR, BLOCKS, CHAPTER_LENGTH, FEATURES, MORE_AT, MORE_CARDS, OPEN_LENGTH, OUTRO_AT, SCENE_LIST, TITLES, blockOf, chapterAt } from './timeline';

// The release as a rack being patched. The open fills the rack with the
// fourteen modules, patches them one into the next, and goes into the
// first; each module is then patched in on its own downbeat, its demo on
// its screen; a faceplate slides across the frame between them, in the
// color of the block coming in; MORE runs twelve cards; the outro pulls
// back out of the core's screen onto the rack and powers it down.

if (CHAPTERS.length !== FEATURES || CARDS.length !== MORE_CARDS || CHAPTERS.some((chapter, i) => chapter.title !== TITLES[i])) {
  throw new Error('v0.10.0/timeline.ts does not match chapters.tsx and More.tsx');
}

/** The whole rack, powered down: the poster. */
export const POSTER_AT = OUTRO_AT + 4 * BAR - 1;

export function Reel() {
  const frame = useCurrentFrame();
  return (
    <DeltaTheme>
      {frame < OPEN_LENGTH ? <Open /> : null}
      {CHAPTERS.map((chapter, i) => (
        <Sequence key={chapter.title} from={chapterAt(i)} durationInFrames={CHAPTER_LENGTH}>
          <Program chapter={chapter} index={i} />
        </Sequence>
      ))}
      <More />
      {frame >= OUTRO_AT ? <Outro /> : null}
      <Stinger />
    </DeltaTheme>
  );
}

/** The cuts: every chapter's downbeat, MORE, and the outro, each with the color coming in. */
const CUTS: readonly (readonly [number, string])[] = [
  ...Array.from({ length: FEATURES }, (_, i) => [chapterAt(i), CABLE[blockOf(i).cable]] as const),
  [MORE_AT, C.paper],
  [OUTRO_AT, C.orange],
];
const STING = 7;

/**
 * A faceplate sliding across the frame, covering it on the downbeat: the
 * block's color, a column of vents, rack screws down both edges, the
 * module number it brings in.
 */
function Stinger() {
  const frame = useCurrentFrame();
  const found = CUTS.findIndex(([at]) => Math.abs(frame - at) <= STING);
  if (found < 0) return null;
  const [cut, color] = CUTS[found];
  const width = 2500;
  const x = interpolate(frame - cut, [-STING, STING], [W + 60, -width - 60]);
  const label = found < FEATURES ? String(found + 1).padStart(2, '0') : found === FEATURES ? 'MORE' : 'CORE';
  const block = found < FEATURES ? BLOCKS.find((b) => b.cable === blockOf(found).cable)?.name ?? '' : '';
  return (
    <Group x={x}>
      <Rect x={-36} width={36} height={H} fill="ink" />
      <Rect width={width} height={H} fill={color} />
      <Rect x={width} width={24} height={H} fill="paper" />
      {Array.from({ length: 14 }, (_, i) => (
        <Group key={i}>
          <Rect x={28} y={40 + i * 76} width={22} height={22} cornerRadius={11} fill="#00000044" />
          <Rect x={width - 50} y={40 + i * 76} width={22} height={22} cornerRadius={11} fill="#00000044" />
        </Group>
      ))}
      {Array.from({ length: 18 }, (_, i) => <Rect key={i} x={150 + i * 40} y={120} width={14} height={840} cornerRadius={7} fill="#00000026" />)}
      {found < FEATURES ? <Delta x={900} y={H / 2 - 20} size={200} fill={C.ink} /> : null}
      <Text x={1040} y={H / 2} anchorY={0.5} style={{ fontFamily: FONT.display, fontSize: 560, fill: solid(C.ink) }}>{label}</Text>
      <Text x={1040} y={H / 2 + 260} style={{ fontFamily: FONT.mono, fontSize: 40, fontWeight: 700, fill: solid(C.ink) }}>{block}</Text>
    </Group>
  );
}

/** Over the programs: corner marks, the timecode, the chapter by name, progress. */
export function Chrome() {
  const frame = useCurrentFrame();
  const shown = Math.min(
    interpolate(frame, [OPEN_LENGTH, OPEN_LENGTH + 12], [0, 1], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' }),
    interpolate(frame, [OUTRO_AT + BAR - 12, OUTRO_AT + BAR], [1, 0], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' }),
  );
  if (shown <= 0) return null;
  return (
    <DeltaTheme>
      <Group opacity={shown}>
        <FrameChrome margin={44} corners color={C.paper} blendMode="difference"
          fontFamily={['JetBrains Mono', 'Noto Sans JP']} fontSize={18}
          title="CELESTA 0.10.0" subtitle="/ PATCH" timecode timecodeHours={false}
          chapters scenes={SCENE_LIST} formatChapter={(scene) => (scene.index === 0 || scene.index > FEATURES + 1
            ? scene.name : `${scene.index <= FEATURES ? `Δ${String(scene.index).padStart(2, '0')}` : '+'}  ${scene.name}`)}
          caption="1920×1080 · 30 FPS · 120 BPM" progress />
      </Group>
    </DeltaTheme>
  );
}

/** Every language at once; --subtitles 'delta.{lang}.srt' writes one file each. */
export function Captions() {
  return <>{CAPTION_LANGUAGES.map((lang) => <Subtitles key={lang} lang={lang} cues={CAPTIONS[lang]} />)}</>;
}

/** The score, faded in over the first bar and out over the last two. */
export function Sound() {
  return <Audio src={SCORE.src} fadeIn={SCORE.fadeIn} fadeOut={SCORE.fadeOut} />;
}
