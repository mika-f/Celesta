import { Easings, Group, Rect, Text, progress, useCurrentFrame } from '@celesta.studio/react';
import { C, FONT, solid } from '../components/theme';
import { CHAPTERS } from './chapters';
import { PORTRAIT } from './layout';
import { BANDS, scoreAt } from './score';
import { BAR, CHAPTER_COUNT, OUTRO_AT, chapterAt } from './timeline';

// The short's edges, outside the safe area the platforms cover with their
// UI: nothing there is needed to follow the film, so it carries motion
// instead. A floor of the score's bands along the bottom, the chapter's
// number cropped by the lower-right corner, and its name running up the
// right edge. Drawn over the cut from the first chapter on.
const { width: WIDTH, height: HEIGHT, safe } = PORTRAIT;
const FLOOR = 220;
const BAR_COUNT = 36;

/** The scene at a frame: a chapter, a MORE card, or the outro. */
function sceneAt(frame: number) {
  if (frame >= OUTRO_AT) return { kind: 'outro' as const, start: OUTRO_AT };
  const more = chapterAt(CHAPTER_COUNT - 1);
  if (frame >= more) {
    const card = Math.floor((frame - more) / BAR);
    return { kind: 'more' as const, start: more + card * BAR, card };
  }
  let index = 0;
  while (index + 1 < CHAPTERS.length && frame >= chapterAt(index + 1)) index++;
  return { kind: 'chapter' as const, start: chapterAt(index), index };
}

export function ShortFrame() {
  const frame = useCurrentFrame();
  if (frame < chapterAt(0)) return null;
  const scene = sceneAt(frame);
  const f = frame - scene.start;
  const { bands, rms } = scoreAt(frame);
  // MORE's odd cards are orange: draw on them in ink.
  const onOrange = scene.kind === 'more' && scene.card % 2 === 1;
  const ink = onOrange ? C.ink : C.orange;
  const ghost = onOrange ? '#0C0D0F30' : '#FFFFFF1C';
  const enter = progress(f, 0, 12, Easings.easeOutCubic);
  const number = scene.kind === 'chapter' ? String(scene.index + 1).padStart(2, '0') : scene.kind === 'more' ? '+' : null;
  const name = scene.kind === 'chapter' ? CHAPTERS[scene.index].title : scene.kind === 'more' ? 'MORE' : 'CELESTA 0.7.0';
  const step = WIDTH / BAR_COUNT;
  return (
    <Group>
      {number ? (
        <Text x={WIDTH + 30 - 60 * (1 - enter)} y={HEIGHT + 90} anchorX={1} anchorY={1} opacity={enter}
          style={{ fontFamily: FONT.display, fontSize: 620, fill: solid('#00000000'), stroke: { paint: solid(ghost), width: 4 } }}>
          {number}
        </Text>
      ) : null}
      <Group x={WIDTH - safe.right / 2} y={safe.top} rotation={90} opacity={0.5 * enter}>
        <Text x={-((frame * 2) % 400)} anchorY={0.5}
          style={{ fontFamily: FONT.mono, fontSize: 22, fontWeight: 700, letterSpacing: { em: 0.3 }, fill: solid(onOrange ? C.ink : C.grey) }}>
          {`${name}  Δ  ${name}  Δ  ${name}  Δ  ${name}  Δ  ${name}`}
        </Text>
      </Group>
      {/* The floor: mirrored bands, lows in the middle, rising from the bottom edge. */}
      <Group y={HEIGHT}>
        {Array.from({ length: BAR_COUNT }, (_, i) => {
          const fromMiddle = Math.abs(i - (BAR_COUNT - 1) / 2) / ((BAR_COUNT - 1) / 2);
          const level = bands[Math.min(BANDS - 1, Math.floor(fromMiddle * 24))];
          const h = 8 + FLOOR * level * (0.6 + 0.4 * rms);
          return <Rect key={i} x={i * step + 3} y={-h} width={step - 6} height={h + 10} cornerRadius={3} fill={ink} opacity={0.25 + 0.6 * level} />;
        })}
      </Group>
    </Group>
  );
}
