import { Easings, Group, Rect, Text, interpolate, progress, useCurrentFrame } from '@celesta/react';
import { C, FONT, solid } from '../components/theme';
import { CHAPTERS } from './chapters';
import { PORTRAIT } from './layout';
import { bedGain } from './mix';
import { BANDS, scoreAt } from './score';
import { DeltaTheme } from './theme';
import { CARD_LENGTH, MORE_AT, MUSIC_BAR, OPEN_LENGTH, OUTRO_AT, chapterAt } from './timeline';

// The short's edges, outside the safe area the platforms cover with their
// UI: nothing there is needed to follow the film, so they carry the
// broadcast instead. The ON AIR lamp at the top; a floor of the score's
// bands along the bottom, as the mix plays them (faded and ducked); the
// program's number cropped by the lower-right corner; its name running up
// the right edge. From the dive into the first program to the pull back.
const { width: WIDTH, height: HEIGHT, safe } = PORTRAIT;
const FLOOR = 220;
const BAR_COUNT = 36;

/** The scene at a frame: a program, a MORE card, or the outro. */
function sceneAt(frame: number) {
  if (frame >= OUTRO_AT) return { kind: 'outro' as const, start: OUTRO_AT };
  if (frame >= MORE_AT) {
    const card = Math.floor((frame - MORE_AT) / CARD_LENGTH);
    return { kind: 'more' as const, start: MORE_AT + card * CARD_LENGTH, card };
  }
  let index = 0;
  while (index + 1 < CHAPTERS.length && frame >= chapterAt(index + 1)) index++;
  return { kind: 'program' as const, start: chapterAt(index), index };
}

export function ShortFrame() {
  const frame = useCurrentFrame();
  const shown = Math.min(
    interpolate(frame, [OPEN_LENGTH, OPEN_LENGTH + 12], [0, 1], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' }),
    interpolate(frame, [OUTRO_AT + MUSIC_BAR - 12, OUTRO_AT + MUSIC_BAR], [1, 0], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' }),
  );
  if (shown <= 0) return null;
  const scene = sceneAt(frame);
  const f = frame - scene.start;
  const { bands, rms } = scoreAt(frame);
  const gain = bedGain(frame);
  // MORE's odd cards are orange: draw on them in ink.
  const onOrange = scene.kind === 'more' && scene.card % 2 === 1;
  const ink = onOrange ? C.ink : C.orange;
  const ghost = onOrange ? '#0C0D0F30' : '#FFFFFF1C';
  const enter = progress(f, 0, 12, Easings.easeOutCubic);
  const number = scene.kind === 'program' ? String(scene.index + 1).padStart(2, '0') : scene.kind === 'more' ? '+' : null;
  const name = scene.kind === 'program' ? CHAPTERS[scene.index].title : scene.kind === 'more' ? 'MORE' : 'CELESTA 0.9.0';
  const step = WIDTH / BAR_COUNT;
  return (
    <DeltaTheme>
      <Group opacity={shown}>
        <Group x={WIDTH / 2} y={96}>
          <Rect x={-74} y={-22} width={148} height={44} cornerRadius={4} fill="tally" />
          <Text anchorX={0.5} anchorY={0.5} y={2} style={{ fontFamily: FONT.display, fontSize: 36, fill: solid(C.paper) }}>ON AIR</Text>
        </Group>
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
            const level = bands[Math.min(BANDS - 1, Math.floor(fromMiddle * 24))] * gain;
            const h = 8 + FLOOR * level * (0.6 + 0.4 * rms);
            return <Rect key={i} x={i * step + 3} y={-h} width={step - 6} height={h + 10} cornerRadius={3} fill={ink} opacity={0.25 + 0.6 * level} />;
          })}
        </Group>
      </Group>
    </DeltaTheme>
  );
}
