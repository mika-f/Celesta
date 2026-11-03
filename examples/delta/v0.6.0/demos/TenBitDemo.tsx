import { Easings, Group, Rect, Text, interpolate, interpolateColor } from '@celesta.studio/react';
import { STAGE, enter } from '../../components/ChapterShell';
import type { DemoProps } from '../../components/ChapterShell';
import { C, FONT, solid } from '../../components/theme';

// A dark sky gradient. Left of the divider: an illustration of the bands an
// 8-bit render without dither leaves (drawn as steps). Right: the gradient
// itself, as 0.6.0 renders it: composited in half float, dithered once.
// Below: the banding #252 measured, and the flag for 10-bit H.264.
const SKY = { x: 30, y: 26, width: 800, height: 380 };
const TOP = '#08090C';
const BOTTOM = '#5A2C4C';
const BANDS = 7;

const MEASURED = [
  { label: 'PNG', before: 54, after: 16 },
  { label: 'MP4', before: 154, after: 40 },
];

export function TenBitDemo({ frame }: DemoProps) {
  const divider = interpolate(frame, [8, 40], [SKY.width, SKY.width / 2],
    { easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  const mono = { fontFamily: FONT.mono, fontSize: 18 };
  const stats = enter(frame, 40);
  return (
    <Group>
      <Rect width={STAGE.width} height={STAGE.height} fill={C.ink} />
      <Group x={SKY.x} y={SKY.y} clip={{ width: SKY.width, height: SKY.height, cornerRadius: 8 }}>
        {Array.from({ length: BANDS }, (_, i) => (
          <Rect key={i} y={(i * SKY.height) / BANDS} width={SKY.width} height={SKY.height / BANDS + 1}
            fill={interpolateColor(i, [0, BANDS - 1], [TOP, BOTTOM])} />
        ))}
        <Group clip={{ x: divider, width: SKY.width - divider, height: SKY.height }}>
          <Rect width={SKY.width} height={SKY.height}
            fill={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: SKY.height },
              stops: [{ offset: 0, color: TOP }, { offset: 1, color: BOTTOM }] }} />
        </Group>
        <Rect x={divider - 1} width={3} height={SKY.height} fill={C.paper} />
        <Text x={20} y={30} anchorY={0.5} style={{ ...mono, fill: solid(C.grey) }}>8-bit, no dither (illustrated)</Text>
        <Text x={SKY.width - 20} y={30} anchorX={1} anchorY={0.5} opacity={enter(frame, 30)}
          style={{ ...mono, fontWeight: 700, fill: solid(C.paper) }}>0.6.0: dithered</Text>
      </Group>
      <Group x={30} y={450} opacity={stats}>
        <Text anchorY={0.5} style={{ ...mono, fill: solid(C.dim) }}>longest flat run, shorts-tips frame 20</Text>
        {MEASURED.map((m, i) => {
          const grow = enter(frame, 46 + i * 6, 24);
          const unit = 3.2;
          return (
            <Group key={m.label} y={40 + i * 58}>
              <Text anchorY={0.5} y={18} style={{ ...mono, fontWeight: 700, fill: solid(C.grey) }}>{m.label}</Text>
              <Rect x={70} y={2} width={m.before * unit * grow} height={14} fill={C.dim} />
              <Rect x={70} y={20} width={m.after * unit * grow} height={14} fill={C.orange} />
              <Text x={80 + m.before * unit} y={9} anchorY={0.5} style={{ ...mono, fontSize: 16, fill: solid(C.dim) }}>{`${m.before} px`}</Text>
              <Text x={80 + m.after * unit} y={27} anchorY={0.5} style={{ ...mono, fontSize: 16, fill: solid(C.orange) }}>{`${m.after} px`}</Text>
            </Group>
          );
        })}
      </Group>
      <Group x={30} y={640} opacity={enter(frame, 66)}>
        <Rect width={STAGE.width - 60} height={64} cornerRadius={10} fill={C.raised} stroke={C.orange} strokeWidth={2} />
        <Text x={24} y={32} anchorY={0.5} style={{ ...mono, fontSize: 21, fontWeight: 700, fill: solid(C.orange) }}>--bit-depth 10</Text>
        <Text x={STAGE.width - 84} y={32} anchorX={1} anchorY={0.5} style={{ ...mono, fill: solid(C.grey) }}>
          H.264 High 10  ·  yuv420p10le  ·  MP4
        </Text>
      </Group>
    </Group>
  );
}
