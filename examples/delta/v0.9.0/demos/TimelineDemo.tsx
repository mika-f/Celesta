import { Easings, Group, Rect, Text, interpolate } from '@celesta/react';
import { C, solid } from '../../components/theme';
import type { DemoProps } from '../program';
import { STAGE } from '../program';
import { BAR } from '../timeline';
import { Pointer, mono } from './common';

// TIMELINE: timing edited on the timeline and written into the TSX (#552).
// Drawn: the price tag's band, a <Sequence> of its own, has its start edge
// pulled to the playhead (from and durationInFrames, so the end stays) and
// is then moved by its middle (from); Main's end is dragged on the Chapters
// row (its defineScenes entry's durationInFrames, Outro moving after it);
// and the mark, drawn for its whole scene, gets Set timing, which wraps it
// in a <Sequence> from the playhead.

const TOTAL = 330;
const LANE_X = 150;
const LANE_W = STAGE.width - LANE_X - 16;
const px = (frame: number) => LANE_X + (frame / TOTAL) * LANE_W;
const INTRO = 90;
const OUTRO = 90;
const PLAYHEAD = 45;
const ROW = 60;

const ease = (frame: number, from: number, to: number) =>
  interpolate(frame, [from, to], [0, 1], { easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
const lerp = (a: number, b: number, t: number) => Math.round(a + (b - a) * t);

function Band({ y, from, length, label, selected, grips, fill = 'raised' }:
  { y: number; from: number; length: number; label: string; selected?: boolean; grips?: boolean; fill?: string }) {
  const x = px(from);
  const w = px(from + length) - x;
  return (
    <Group y={y}>
      <Rect x={x} y={8} width={w - 2} height={ROW - 16} fill={fill} stroke={selected ? 'paper' : 'line'} strokeWidth={selected ? 2 : 1} />
      <Text x={x + 10} y={ROW / 2} anchorY={0.5} style={mono(14, selected ? C.ink : C.grey, 700)}>{label}</Text>
      {grips ? (
        <>
          <Rect x={x - 4} y={4} width={8} height={ROW - 8} fill="paper" stroke="ink" strokeWidth={1} />
          <Rect x={x + w - 6} y={4} width={8} height={ROW - 8} fill="paper" stroke="ink" strokeWidth={1} />
        </>
      ) : null}
    </Group>
  );
}

export function TimelineDemo({ frame }: DemoProps) {
  // Bar 0: the start edge. Bar 1: the middle. Bar 2: Main's end. Bar 3: Set timing.
  const edge = ease(frame, 6, BAR - 8);
  const middle = ease(frame, BAR + 6, 2 * BAR - 8);
  const sceneEnd = ease(frame, 2 * BAR + 6, 3 * BAR - 8);
  const wrapped = frame >= 3 * BAR + 16;
  const from = frame < BAR ? lerp(30, PLAYHEAD, edge) : lerp(PLAYHEAD, 60, middle);
  const length = frame < BAR ? lerp(60, 45, edge) : 45;
  const main = lerp(120, 150, sceneEnd);
  const mainAt = INTRO;
  const playhead = mainAt + PLAYHEAD;

  const label = frame < BAR ? `from 30 → ${from} · durationInFrames 60 → ${length}`
    : frame < 2 * BAR ? `from ${PLAYHEAD} → ${from}`
      : frame < 3 * BAR ? `durationInFrames 120 → ${main}`
        : wrapped ? 'Set timing: from 45' : '';
  const labelAt = frame < 2 * BAR ? px(mainAt + from) : frame < 3 * BAR ? px(mainAt + main) : px(playhead);
  const dragging = (frame >= 6 && frame < BAR - 4) || (frame >= BAR + 6 && frame < 2 * BAR - 4) || (frame >= 2 * BAR + 6 && frame < 3 * BAR - 4);

  const tracksY = 236;
  const pointer = frame < BAR ? { x: px(mainAt + from), y: tracksY + ROW * 2 + ROW / 2 }
    : frame < 2 * BAR ? { x: px(mainAt + from + length / 2), y: tracksY + ROW * 2 + ROW / 2 }
      : frame < 3 * BAR ? { x: px(mainAt + main), y: tracksY + ROW / 2 }
        : { x: px(playhead) + 44, y: tracksY + ROW * 3 + ROW / 2 };

  const source = frame < 2 * BAR
    ? { file: 'Main.tsx', lines: [`<Sequence from={${from}} durationInFrames={${length}}>`, '  <PriceTag label="Sale" />', '</Sequence>'] }
    : frame < 3 * BAR
      ? { file: 'timeline.ts', lines: ['defineScenes([', `  { id: 'main', durationInFrames: ${main}, Scene: Main },`, '])'] }
      : wrapped
        ? { file: 'Main.tsx', lines: [`<Sequence from={${PLAYHEAD}} durationInFrames={${main - PLAYHEAD}}>`, '  <Mark />', '</Sequence>'] }
        : { file: 'Main.tsx', lines: ['<Mark />'] };
  const hot = frame < 2 * BAR ? 0 : frame < 3 * BAR ? 1 : wrapped ? 0 : -1;
  return (
    <Group>
      <Rect width={STAGE.width} height={36} fill="raised" />
      <Text x={14} y={18} anchorY={0.5} style={mono(15, C.grey)}>film.tsx — Celesta</Text>
      <Text x={STAGE.width - 14} y={18} anchorX={1} anchorY={0.5} style={mono(15, C.dim)}>Timeline</Text>

      <Group y={36}>
        <Rect width={STAGE.width} height={150} fill="#08090B" />
        <Text x={20} y={26} anchorY={0.5} style={mono(14, C.dim)}>{source.file}</Text>
        {source.lines.map((text, i) => (
          <Text key={i} x={20} y={62 + i * 32} anchorY={0.5} style={mono(19, i === hot ? C.orange : C.paper, i === hot ? 700 : 400)}>{text}</Text>
        ))}
      </Group>

      <Group y={196}>
        {Array.from({ length: TOTAL / 30 }, (_, i) => (
          <Group key={i} x={px(i * 30)}>
            <Rect width={1} height={8} fill="line" />
            <Text x={4} y={20} anchorY={0.5} style={mono(12, C.dim)}>{String(i * 30)}</Text>
          </Group>
        ))}
      </Group>

      {['Chapters', 'Title', 'PriceTag', 'Mark'].map((name, i) => (
        <Group key={name} y={tracksY + i * ROW}>
          <Rect width={STAGE.width} height={1} fill="line" />
          <Text x={14} y={ROW / 2} anchorY={0.5} style={mono(15, i === 2 && frame < 2 * BAR ? C.paper : C.grey, 700)}>{name}</Text>
        </Group>
      ))}

      <Band y={tracksY} from={0} length={INTRO} label="Intro" />
      <Band y={tracksY} from={mainAt} length={main} label="Main" selected={frame >= 2 * BAR && frame < 3 * BAR} fill={frame >= 2 * BAR && frame < 3 * BAR ? 'orange' : 'raised'} />
      <Band y={tracksY} from={mainAt + main} length={OUTRO} label="Outro" />
      <Band y={tracksY + ROW} from={0} length={TOTAL} label="<Title>" />
      <Band y={tracksY + ROW * 2} from={mainAt + from} length={length} label="<Sequence>" selected={frame < 2 * BAR}
        grips={frame < 2 * BAR} fill={frame < 2 * BAR ? 'orange' : 'raised'} />
      {wrapped
        ? <Band y={tracksY + ROW * 3} from={playhead} length={main - PLAYHEAD} label="<Sequence>" selected grips fill="orange" />
        : <Band y={tracksY + ROW * 3} from={mainAt} length={main} label="" selected={frame >= 3 * BAR} fill={frame >= 3 * BAR ? '#F26A2E55' : 'raised'} />}
      {frame >= 3 * BAR && !wrapped ? (
        <Group x={px(playhead) + 6} y={tracksY + ROW * 3 + 12}>
          <Rect width={110} height={ROW - 24} fill="paper" />
          <Text x={55} y={(ROW - 24) / 2} anchorX={0.5} anchorY={0.5} style={mono(13, C.ink, 700)}>Set timing</Text>
        </Group>
      ) : null}

      <Group x={px(playhead)} y={196}>
        <Rect x={-1} width={3} height={STAGE.height - 196} fill="red" />
        <Rect x={-6} width={13} height={10} fill="red" />
      </Group>

      {label !== '' ? (
        <Group x={Math.min(labelAt + 12, STAGE.width - 16 - (label.length * 8.4 + 24))} y={tracksY + ROW * 4 + 14}>
          <Rect width={label.length * 8.4 + 24} height={34} fill="ink" stroke="orange" strokeWidth={1} />
          <Text x={12} y={17} anchorY={0.5} style={{ ...mono(14, C.orange, 700), fill: solid(C.orange) }}>{label}</Text>
        </Group>
      ) : null}
      <Pointer x={pointer.x} y={pointer.y} pressed={dragging || (frame >= 3 * BAR + 12 && frame < 3 * BAR + 16)} />
    </Group>
  );
}
