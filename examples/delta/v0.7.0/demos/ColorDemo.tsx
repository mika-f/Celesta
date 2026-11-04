import { Group, Rect, Text } from '@celesta.studio/react';
import type { GroupProps } from '@celesta.studio/react';
import { grayscale, tint, vignette } from '@celesta.studio/shader';
import type { DemoProps } from '../../components/ChapterShell';
import { STAGE, enter } from '../../components/ChapterShell';
import { ripple } from '../../components/shaders';
import { C, FONT, solid } from '../../components/theme';
import { BAR, BEAT } from '../timeline';
import { Picture } from './Picture';

// Bars 1-2: the picture under each color prop, one a beat.
// Bars 3-4: one shader array, built up a beat at a time: a WGSL ripple,
// then the built-in grayscale, tint and vignette, in that order.
const GRADES: readonly (readonly [string, Partial<GroupProps>])[] = [
  ['original', {}],
  ['brightness={1.5}', { brightness: 1.5 }],
  ['contrast={1.8}', { contrast: 1.8 }],
  ['saturation={0}', { saturation: 0 }],
  ['hue={150}', { hue: 150 }],
  ['grayscale={1}', { grayscale: 1 }],
  ['invert={1}', { invert: 1 }],
  ["tint={{ color: '#5AA9FF' }}", { tint: { color: '#5AA9FF', amount: 0.5 } }],
  ['vignette={0.9}', { vignette: { amount: 0.9, radius: 0.3 } }],
];
const CELL = { width: 276, height: 196, gap: 16 };
const CHAIN = ['ripple({ time })', 'grayscale()', "tint({ color: '#F26A2E' })", 'vignette(0.7)'];

function Grid({ frame }: { frame: number }) {
  return (
    <Group>
      {GRADES.map(([label, props], i) => {
        const col = i % 3;
        const row = Math.floor(i / 3);
        const p = enter(frame, i * (BEAT * 0.8), 10);
        return (
          <Group key={label} x={col * (CELL.width + CELL.gap)} y={row * (CELL.height + 52)} opacity={p} scale={0.9 + 0.1 * p}>
            <Group clip={{ width: CELL.width, height: CELL.height, cornerRadius: 6 }} {...props}>
              <Picture width={CELL.width} height={CELL.height} />
            </Group>
            <Text y={CELL.height + 26} anchorY={0.5} style={{ fontFamily: FONT.mono, fontSize: 17, fill: solid(i === 0 ? C.grey : C.paper) }}>{label}</Text>
          </Group>
        );
      })}
    </Group>
  );
}

function Chain({ frame }: { frame: number }) {
  const f = frame - 2 * BAR;
  const steps = Math.max(1, Math.min(CHAIN.length, Math.floor(f / BEAT / 1.5) + 1));
  const shaders = [
    ripple({ time: f / 20, amplitude: 10, wavelength: 110 }),
    grayscale(),
    tint({ color: '#F26A2E', amount: 0.55 }),
    vignette({ amount: 0.85, radius: 0.35 }),
  ].slice(0, steps);
  const size = { width: STAGE.width, height: 500 };
  return (
    <Group>
      <Group clip={{ width: size.width, height: size.height, cornerRadius: 8 }}>
        <Group shader={shaders}>
          <Picture width={size.width} height={size.height} t={frame} />
        </Group>
      </Group>
      <Group y={size.height + 28}>
        <Text style={{ fontFamily: FONT.mono, fontSize: 22, fill: solid(C.grey) }}>{'shader={['}</Text>
        {CHAIN.map((call, i) => (
          <Group key={call} y={36 + i * 36} opacity={i < steps ? 1 : 0.25}>
            <Rect x={24} y={2} width={6} height={26} fill={i < steps ? C.orange : C.line} />
            <Text x={44} style={{ fontFamily: FONT.mono, fontSize: 22, fill: solid(i === steps - 1 ? C.orange : C.paper) }}>{`${call},`}</Text>
          </Group>
        ))}
        <Text x={340} y={36 + 3 * 36} style={{ fontFamily: FONT.mono, fontSize: 22, fill: solid(C.grey) }}>{']}'}</Text>
      </Group>
    </Group>
  );
}

export function ColorDemo({ frame }: DemoProps) {
  if (frame < 2 * BAR) return <Grid frame={frame} />;
  return <Chain frame={frame} />;
}
