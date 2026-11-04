import type { ReactNode } from 'react';
import { Group, Rect, Sequence, Span, Text, Theme, useTheme } from '@celesta.studio/react';
import { Badge, Counter } from '@celesta.studio/components';
import type { DemoProps } from '../../components/ChapterShell';
import { CUT, STAGE, enter } from '../../components/ChapterShell';
import { BAR, BEAT } from '../timeline';

// One card, written once with names only: fontFamily="display",
// color="accent". Drawn twice, under the film's <Theme> and under an inner
// one that renames the colors and the display face; every <Text>, Badge and
// Counter picks the names up. The swatches read useTheme().
const PAPER = { colors: { bg: '#EFECE4', fg: '#0C0D0F', accent: '#2E5BF2', soft: '#0C0D0F99' }, fonts: { display: 'Space Grotesk' } };
const PANEL = { width: 410, height: 470 };

function Swatches() {
  const { colors } = useTheme();
  return (
    <Group>
      {(['bg', 'fg', 'accent', 'soft'] as const).map((name, i) => (
        <Group key={name} x={i * 92}>
          <Rect width={80} height={26} cornerRadius={4} fill={colors[name]} stroke="#FFFFFF33" strokeWidth={1} />
          <Text y={40} fontFamily="mono" fontSize={14} color="soft">{name}</Text>
        </Group>
      ))}
    </Group>
  );
}

/** The card: nothing in it names a font file or a hex color. */
function Card({ label }: { label: string }) {
  // <Rect> takes hex colors: it reads the names through useTheme().
  const { colors } = useTheme();
  return (
    <Group>
      <Rect width={PANEL.width} height={PANEL.height} cornerRadius={16} fill={colors.bg} stroke={colors.soft} strokeWidth={1} />
      <Group x={28} y={28}>
        <Badge fill="accent" cornerRadius="pill" fontFamily="mono" fontSize={16} fontWeight={700} color="bg">{label}</Badge>
        <Text y={70} fontFamily="display" fontSize={64} fontWeight={700} color="fg">
          Δ <Span color="accent">0.8.0</Span>
        </Text>
        <Text y={160} maxWidth={350} fontFamily="sans" fontSize={22} color="soft" style={{ lineHeight: 32 }}>
          Fonts and colors by name, from the nearest Theme.
        </Text>
        <Counter y={250} value={421} prefix="#" delay={BEAT} durationInFrames={3 * BEAT} fontFamily="display" fontSize={96} color="accent" />
        <Group y={370}><Swatches /></Group>
      </Group>
    </Group>
  );
}

function Column({ x, at, frame, children }: { x: number; at: number; frame: number; children: ReactNode }) {
  const p = enter(frame, at, 16);
  return <Group x={x} y={24 + 30 * (1 - p)} opacity={p}>{children}</Group>;
}

export function ThemeDemo({ frame }: DemoProps) {
  return (
    <Group>
      <Theme colors={{ bg: '#17191E', fg: '#EFECE4', accent: '#F26A2E', soft: '#8E929B' }}>
        <Sequence from={CUT}>
          <Column x={0} at={0} frame={frame}><Card label="<Theme> of the film" /></Column>
          <Theme {...PAPER}>
            <Column x={450} at={2 * BEAT} frame={frame}><Card label="an inner <Theme>" /></Column>
          </Theme>
        </Sequence>
        <Group y={540} opacity={enter(frame, BAR)}>
          <Text fontFamily="mono" fontSize={20} color="fg">
            {'<Text fontFamily="display" color="accent">'}
          </Text>
          <Text y={36} fontFamily="mono" fontSize={18} color="soft">{'<Theme colors={{ accent: … }}> changes it everywhere below'}</Text>
          <Text y={90} maxWidth={STAGE.width} fontFamily="mono" fontSize={18} color="soft">
            TextBox, TextReveal and Dialogue subtitles look the names up too (#422)
          </Text>
        </Group>
      </Theme>
    </Group>
  );
}
