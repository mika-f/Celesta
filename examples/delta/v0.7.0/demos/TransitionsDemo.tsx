import { Group, Rect, Sequence, Text } from '@celesta.studio/react';
import { blur, clockWipe, cube, dissolve, flip, iris, zoom, TransitionSeries } from '@celesta.studio/transitions';
import type { TransitionDefinition } from '@celesta.studio/transitions';
import type { DemoProps } from '../../components/ChapterShell';
import { CUT, STAGE, enter } from '../../components/ChapterShell';
import { Delta } from '../../components/Delta';
import { C, FONT, solid } from '../../components/theme';
import { BEAT } from '../timeline';

// A real TransitionSeries in the preview, sized to it with width/height:
// one new type every two beats, each scene naming the cut that brought it in.
const SIZE = { width: STAGE.width, height: 520 };
const TYPES: readonly (readonly [string, string, TransitionDefinition])[] = [
  ['iris', 'iris({ feather: 30 })', iris({ feather: 30 })],
  ['clock-wipe', 'clockWipe()', clockWipe()],
  ['zoom', "zoom({ direction: 'in' })", zoom()],
  ['dissolve', 'dissolve({ cellSize: 24 })', dissolve({ cellSize: 24 })],
  ['blur', 'blur({ amount: 40 })', blur({ amount: 40 })],
  ['flip', 'flip()', flip()],
  ['cube', 'cube()', cube()],
];
const COLORS = [C.raised, C.orange, C.blue, C.violet, C.green, C.amber, C.red, C.paper];
const SCENE = 2 * BEAT;
const OVERLAP = BEAT;

function Scene({ i }: { i: number }) {
  const bg = COLORS[i];
  const ink = i === 0 ? C.paper : C.ink;
  const [name, call] = i === 0 ? ['TransitionSeries', 'width={860} height={520}'] : TYPES[i - 1];
  return (
    <Group>
      <Rect width={SIZE.width} height={SIZE.height} fill={bg} />
      <Delta x={SIZE.width - 150} y={SIZE.height / 2 - 20} size={200} fill={i === 0 ? C.orange : '#0C0D0F22'} />
      <Text x={44} y={60} style={{ fontFamily: FONT.mono, fontSize: 24, fill: solid(ink), fontWeight: 700 }}>
        {i === 0 ? 'scene 1' : `scene ${i + 1}  ·  in by ${name}`}
      </Text>
      <Text x={40} y={SIZE.height - 70} anchorY={1} style={{ fontFamily: FONT.display, fontSize: 150, fill: solid(ink) }}>
        {name.toUpperCase()}
      </Text>
      <Text x={44} y={SIZE.height - 40} style={{ fontFamily: FONT.mono, fontSize: 22, fill: solid(ink) }}>{call}</Text>
    </Group>
  );
}

export function TransitionsDemo({ frame }: DemoProps) {
  const current = Math.max(0, Math.min(TYPES.length, Math.floor((frame + OVERLAP / 2) / SCENE)));
  return (
    <Group>
      <Group clip={{ width: SIZE.width, height: SIZE.height, cornerRadius: 8 }}>
        {/* The chapter's frame 0 is CUT frames into its sequence. */}
        <Sequence from={CUT - OVERLAP}>
          <TransitionSeries width={SIZE.width} height={SIZE.height}>
            {COLORS.flatMap((_, i) => [
              ...(i > 0 ? [<TransitionSeries.Transition key={`t${i}`} type={TYPES[i - 1][2]} durationInFrames={OVERLAP} from="right" />] : []),
              <TransitionSeries.Sequence key={i} durationInFrames={SCENE + OVERLAP}><Scene i={i} /></TransitionSeries.Sequence>,
            ])}
          </TransitionSeries>
        </Sequence>
      </Group>
      <Group y={560}>
        {['crossfade', 'slide', 'wipe', ...TYPES.map(([name]) => name), 'defineTransition'].map((name, i) => {
          const isNew = i >= 3 && i < 10;
          const on = i === current + 2 && current > 0;
          const col = i % 4;
          const row = Math.floor(i / 4);
          const p = enter(frame, i * 3, 12);
          return (
            <Group key={name} x={col * 216} y={row * 58} opacity={p}>
              <Rect width={204} height={46} cornerRadius={23} fill={on ? C.orange : isNew ? '#F26A2E1F' : C.raised}
                stroke={i === 10 ? C.orange : undefined} strokeWidth={2} />
              <Text x={102} y={23} anchorX={0.5} anchorY={0.5}
                style={{ fontFamily: FONT.mono, fontSize: 18, fontWeight: 700, fill: solid(on ? C.ink : isNew || i === 10 ? C.orange : C.grey) }}>
                {name}
              </Text>
            </Group>
          );
        })}
      </Group>
    </Group>
  );
}
