import { Group, Rect, Text, interpolate } from '@celesta.studio/react';
import type { GroupProps } from '@celesta.studio/react';
import { Circle } from '@celesta.studio/shapes';
import type { DemoProps } from '../../components/ChapterShell';
import { STAGE, enter } from '../../components/ChapterShell';
import { Delta } from '../../components/Delta';
import { C, FONT, solid } from '../../components/theme';
import { BEAT } from '../timeline';
import { Picture } from './Picture';

// One effect every two beats on the same picture, each a plain layer prop
// animated over its two beats. The last two beats key a green screen.
type Effect = readonly [string, (t: number) => Partial<GroupProps>];

const EFFECTS: readonly Effect[] = [
  ['halftone={14}', (t) => ({ halftone: { size: 6 + 12 * t, angle: 45 } })],
  ['pixelate={24}', (t) => ({ pixelate: 4 + 28 * t })],
  ['chromaticAberration={12}', (t) => ({ chromaticAberration: { offset: 4 + 18 * t, angle: 360 * t } })],
  ['wave={{ amplitude: 18 }}', (t) => ({ wave: { amplitude: 18, wavelength: 140, phase: 720 * t } })],
  ['noise={0.5}', (t) => ({ noise: { amount: 0.25 + 0.4 * t, seed: Math.floor(t * 8) } })],
  ['scanlines={0.6}', (t) => ({ scanlines: { amount: 0.6, spacing: 6, offset: 30 * t }, brightness: 1.2 })],
  ['zoomBlur={0.4}', (t) => ({ zoomBlur: { amount: 0.1 + 0.35 * t, centerX: 0.68, centerY: 0.42 } })],
];
const WINDOW = 2 * BEAT;
const PIC = { width: STAGE.width, height: 600 };

function GreenScreen({ t }: { t: number }) {
  const key = t > 0.25;
  return (
    <Group>
      {/* What shows through: the picture again. */}
      <Picture width={PIC.width} height={PIC.height} t={40} />
      <Group chromaKey={key ? { color: '#00B140', tolerance: 0.2, softness: 0.1 } : undefined}>
        <Rect width={PIC.width} height={PIC.height} fill="#00B140" />
        <Circle x={PIC.width / 2} y={PIC.height + 120} anchorX={0.5} anchorY={0.5} radius={300} fill="#1B1F2A" />
        <Delta x={PIC.width / 2} y={PIC.height - 300} size={180} fill={C.paper} />
        <Text x={PIC.width / 2} y={PIC.height - 90} anchorX={0.5} anchorY={0.5}
          style={{ fontFamily: FONT.display, fontSize: 90, fill: solid(C.paper) }}>ON AIR</Text>
      </Group>
    </Group>
  );
}

export function EffectsDemo({ frame }: DemoProps) {
  const index = Math.max(0, Math.floor(frame / WINDOW));
  const t = (Math.max(0, frame) % WINDOW) / WINDOW;
  const keyed = index >= EFFECTS.length;
  const [label, props] = keyed ? ["chromaKey={'#00B140'}", () => ({})] as const : EFFECTS[index];
  const p = enter(frame, 0);
  return (
    <Group opacity={p}>
      <Group clip={{ width: PIC.width, height: PIC.height, cornerRadius: 8 }}>
        {keyed ? <GreenScreen t={t} /> : (
          <Group {...props(interpolate(t, [0, 1], [0, 1]))}>
            <Picture width={PIC.width} height={PIC.height} t={frame} />
          </Group>
        )}
      </Group>
      <Group y={PIC.height + 24}>
        <Text style={{ fontFamily: FONT.mono, fontSize: 30, fontWeight: 700, fill: solid(C.orange) }}>{label}</Text>
        <Group y={64}>
          {[...EFFECTS.map(([name]) => name.split('=')[0]), 'chromaKey'].map((name, i) => (
            <Rect key={name} x={i * 106} width={98} height={8} cornerRadius={4}
              fill={i === index ? C.orange : i < index ? C.grey : C.line} />
          ))}
        </Group>
      </Group>
    </Group>
  );
}
