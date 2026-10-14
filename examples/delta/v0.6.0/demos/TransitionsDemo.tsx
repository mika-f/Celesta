import { Easings, Group, Rect, Sequence, Text } from '@celesta/react';
import { TransitionSeries, computeTransitionSeries } from '@celesta/transitions';
import type { TransitionSeriesTransitionProps, TransitionSeriesType } from '@celesta/transitions';
import { CUT, STAGE, enter } from '../../components/ChapterShell';
import type { DemoProps } from '../../components/ChapterShell';
import { C, FONT, H, W, solid } from '../../components/theme';

// A real <TransitionSeries>, drawn at full size and scaled into the player,
// over the timeline that computeTransitionSeries() gives for it.
const SCENE = 34;
const SCENES = [
  { name: 'A', fill: C.orange, ink: C.ink },
  { name: 'B', fill: C.paper, ink: C.ink },
  { name: 'C', fill: C.violet, ink: C.ink },
  { name: 'D', fill: C.green, ink: C.ink },
];
// Named types only, so each label can print its type.
const TRANSITIONS: (TransitionSeriesTransitionProps & { type: TransitionSeriesType })[] = [
  { type: 'crossfade', durationInFrames: 10 },
  { type: 'slide', from: 'right', durationInFrames: 10, easing: Easings.easeInOutCubic },
  { type: 'wipe', from: 'bottom', durationInFrames: 10, easing: Easings.easeInOutCubic },
];
const timing = computeTransitionSeries(SCENES.flatMap((_, i) => (
  i === 0 ? [{ durationInFrames: SCENE }] : [{ type: TRANSITIONS[i - 1].type, durationInFrames: 10 }, { durationInFrames: SCENE }]
)));

const PLAYER = { x: 30, y: 26, width: 800 };
const SCALE = PLAYER.width / W;
const TRACK = { x: 30, y: 540, width: 800, lane: 44 };
const px = (frame: number) => TRACK.x + (frame / timing.durationInFrames) * TRACK.width;

function Scene({ index }: { index: number }) {
  const scene = SCENES[index];
  return (
    <Group>
      <Rect width={W} height={H} fill={scene.fill} />
      <Text x={W / 2} y={H / 2} anchorX={0.5} anchorY={0.5}
        style={{ fontFamily: FONT.display, fontSize: 520, fill: solid(scene.ink) }}>{scene.name}</Text>
      <Text x={W / 2} y={H - 150} anchorX={0.5} anchorY={0.5}
        style={{ fontFamily: FONT.mono, fontSize: 56, fill: solid(scene.ink) }}>
        {index === 0 ? 'scene' : `after ${TRANSITIONS[index - 1].type}`}
      </Text>
    </Group>
  );
}

export function TransitionsDemo({ frame }: DemoProps) {
  const t = Math.max(0, Math.min(frame, timing.durationInFrames - 1));
  const lower = enter(frame, 6);
  const mono = { fontFamily: FONT.mono, fontSize: 18 };
  return (
    <Group>
      <Rect width={STAGE.width} height={STAGE.height} fill={C.ink} />
      <Group x={PLAYER.x} y={PLAYER.y} clip={{ width: PLAYER.width, height: H * SCALE, cornerRadius: 8 }}>
        <Rect width={PLAYER.width} height={H * SCALE} fill={C.raised} />
        <Group scale={SCALE}>
          <Sequence from={CUT} durationInFrames={timing.durationInFrames}>
            <TransitionSeries>
              {SCENES.flatMap((scene, i) => [
                ...(i > 0 ? [<TransitionSeries.Transition key={`t${i}`} {...TRANSITIONS[i - 1]} />] : []),
                <TransitionSeries.Sequence key={scene.name} durationInFrames={SCENE}><Scene index={i} /></TransitionSeries.Sequence>,
              ])}
            </TransitionSeries>
          </Sequence>
        </Group>
      </Group>
      <Group opacity={lower}>
        <Text x={TRACK.x} y={TRACK.y - 40} anchorY={0.5} style={{ ...mono, fill: solid(C.dim) }}>
          {`computeTransitionSeries()  →  ${timing.durationInFrames} frames`}
        </Text>
        {timing.sequences.map((seq, i) => (
          <Group key={i} x={px(seq.from)} y={TRACK.y + (i % 2) * TRACK.lane}>
            <Rect width={px(seq.from + seq.durationInFrames) - px(seq.from) - 2} height={TRACK.lane - 8}
              cornerRadius={6} fill={SCENES[i].fill} opacity={0.9} />
            <Text x={12} y={(TRACK.lane - 8) / 2} anchorY={0.5}
              style={{ fontFamily: FONT.display, fontSize: 28, fill: solid(SCENES[i].ink) }}>{SCENES[i].name}</Text>
          </Group>
        ))}
        {timing.transitions.map((tr, i) => (
          <Group key={i}>
            <Rect x={px(tr.from)} y={TRACK.y - 6} width={px(tr.from + tr.durationInFrames) - px(tr.from)}
              height={TRACK.lane * 2 + 4} fill="#FFFFFF1A" stroke="#FFFFFF55" strokeWidth={2} />
            <Text x={(px(tr.from) + px(tr.from + tr.durationInFrames)) / 2} y={TRACK.y + TRACK.lane * 2 + 30}
              anchorX={0.5} anchorY={0.5} style={{ ...mono, fill: solid(C.grey) }}>{TRANSITIONS[i].type}</Text>
          </Group>
        ))}
        <Rect x={px(t) - 1} y={TRACK.y - 16} width={3} height={TRACK.lane * 2 + 24} fill={C.red} />
        <Text x={TRACK.x} y={STAGE.height - 50} anchorY={0.5} style={{ ...mono, fill: solid(C.grey) }}>
          Every cut in this film is one TransitionSeries, too.
        </Text>
      </Group>
    </Group>
  );
}
