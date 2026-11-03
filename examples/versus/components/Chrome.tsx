import { PatternBackground, TitleCard } from '@celesta.studio/components';
import { Easings, Group, Rect } from '@celesta.studio/react';
import { C, FONT, TOOL, W, type ToolId } from '../constants';
import { Label } from './Label';

// Kicker, headline, and subtitle at the top left of a scene, rising in one
// after another.
export function Header({ n, title, sub, color = TOOL.celesta.color }: {
  n: string; title: string; sub?: string; color?: string;
}) {
  return (
    <TitleCard kicker={n} kickerRule title={title} subtitle={sub} verticalAlign="top"
      padding={{ top: 90, right: 96, bottom: 96, left: 96 }} gap={11}
      fontFamily={FONT.sans} color={C.ink} subtitleColor={C.soft} accentColor={color}
      titleSize={72} titleLineHeight={76 / 72} titleStyle={{ letterSpacing: -2 }}
      subtitleSize={24} subtitleStyle={{ lineHeight: 32 }}
      kickerSize={18} kickerStyle={{ fontFamily: FONT.mono, letterSpacing: 3 }}
      enter={{ type: 'fade-up', durationInFrames: 18, distance: 24, easing: Easings.easeOutExpo }} exit={null} stagger={5} />
  );
}

// Tool name with its color dot.
export function ToolTag({ id, x = 0, y = 0, size = 30, sub = true }: { id: ToolId; x?: number; y?: number; size?: number; sub?: boolean }) {
  const t = TOOL[id];
  return (
    <Group x={x} y={y}>
      <Rect y={size * 0.18} width={size * 0.5} height={size * 0.5} cornerRadius={size * 0.25} fill={t.color} />
      <Label x={size * 0.8} y={0} size={size} weight={700}>{t.name}</Label>
      {sub ? <Label x={size * 0.8} y={size * 1.3} size={Math.max(15, size * 0.5)} font="mono" weight={400} color={C.grey}>{t.stack}</Label> : null}
    </Group>
  );
}

export function Footnote({ children, opacity = 1 }: { children: string; opacity?: number }) {
  return <Label x={W - 96} y={1000} ax={1} size={16} font="mono" weight={400} color={C.grey} opacity={opacity}>{children}</Label>;
}

// Deep space behind every scene: a vignette and a faint dot grid.
export function Backdrop() {
  return (
    <>
      <Rect width={W} height={1080} fill={C.bg} />
      <Rect width={W} height={1080} fill={{
        type: 'radial', center: { x: 1560, y: 120 }, radius: 1400,
        stops: [{ offset: 0, color: '#1A1640' }, { offset: 0.55, color: '#0A0B1A' }, { offset: 1, color: '#05060C00' }],
      }} />
      <PatternBackground pattern="dots" dotShape="square" spacing={80} size={2} offsetX={-19} offsetY={-19} color="#FFFFFF0D" />
    </>
  );
}
