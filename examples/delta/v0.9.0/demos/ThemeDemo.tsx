import { Group, Rect, Text, Theme } from '@celesta/react';
import { Heart, Star } from '@celesta/shapes';
import { Path } from '@celesta/shapes';
import { C, FONT } from '../../components/theme';
import type { DemoProps } from '../program';
import { STAGE } from '../program';
import { mono } from './common';

// THEME: Rect and Path paints look up <Theme> color names now (#481), and
// so every shape built on them. One <Poster> written with names only,
// drawn under three themes; the error a misspelt name throws, as written.

const THEMES = {
  delta: { bg: C.ink, surface: C.raised, accent: C.orange, second: C.amber, text: C.paper },
  night: { bg: '#0E1A2B', surface: '#16263D', accent: '#5AA9FF', second: '#9B87F5', text: '#E6EEF8' },
  paper: { bg: '#EFECE4', surface: '#FFFFFF', accent: '#0C0D0F', second: '#F26A2E', text: '#0C0D0F' },
} as const;

const PANEL = { width: 290, height: 330 };

/** The same JSX in every theme: only color names. */
function Poster({ frame, name }: { frame: number; name: string }) {
  const wave: [number, number][] = Array.from({ length: 25 }, (_, i) => [20 + i * 10.4, 250 + Math.sin(i / 3 + frame / 6) * 18]);
  return (
    <Group>
      <Rect width={PANEL.width} height={PANEL.height} cornerRadius={6} fill="bg" stroke="surface" strokeWidth={3} />
      <Rect x={20} y={20} width={PANEL.width - 40} height={150} cornerRadius={4} fill="surface" />
      <Star x={90} y={95} anchorX={0.5} anchorY={0.5} points={5} radius={50} innerRatio={0.45} rotation={frame * 2} fill="accent" />
      <Heart x={205} y={98} anchorX={0.5} anchorY={0.5} width={84} height={76} fill="second" stroke="text" strokeWidth={3} />
      <Path points={wave} stroke="accent" strokeWidth={5} join="round" cap="round" />
      <Text x={20} y={292} fontFamily="mono" fontSize={20} fontWeight={700} color="text">{name}</Text>
    </Group>
  );
}

export function ThemeDemo({ frame }: DemoProps) {
  const names = Object.keys(THEMES) as (keyof typeof THEMES)[];
  return (
    <Group>
      {names.map((name, i) => (
        <Group key={name} x={40 + i * (PANEL.width + 25)} y={36}>
          <Theme colors={THEMES[name]} fonts={{ mono: FONT.mono }}>
            <Poster frame={frame + i * 9} name={name} />
          </Theme>
        </Group>
      ))}
      <Text x={40} y={404} style={mono(17, C.grey)}>{'<Rect fill="surface" />   <Star fill="accent" />   <Path stroke="accent" />'}</Text>
      <Text x={40} y={448} maxWidth={STAGE.width - 80}
        style={{ ...mono(16, C.red), lineHeight: 24 }}>
        {'<Rect> fill "inc" is neither a #RRGGBB or #RRGGBBAA color nor a <Theme> color name (bg, surface, accent, …)'}
      </Text>
    </Group>
  );
}
