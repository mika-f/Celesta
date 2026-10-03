import { Easings, Group, Rect } from '@celesta/react';
import { C, TOOL, W, type ToolId } from '../constants';
import { progress } from '../helpers';
import { Label } from './Label';

// Kicker and headline at the top left of a scene. The headline rises from
// behind a mask, one line at a time.
export function Header({ f, n, title, sub, color = TOOL.celesta.color }: {
  f: number; n: string; title: string; sub?: string; color?: string;
}) {
  const lines = title.split('\n');
  return (
    <Group x={96} y={92}>
      <Group opacity={progress(f, 0, 12)}>
        <Rect y={6} width={Math.max(1, 36 * progress(f, 0, 16, Easings.easeOutExpo))} height={4} fill={color} />
        <Label x={52} size={18} font="mono" weight={700} color={color} spacing={3}>{n}</Label>
      </Group>
      {lines.map((line, i) => {
        const p = progress(f, 4 + i * 4, 22, Easings.easeOutExpo);
        return (
          <Group key={i} y={40 + i * 76} clip={{ x: -8, y: -10, width: 1800, height: 92 }}>
            <Label y={80 * (1 - p)} size={72} weight={700} spacing={-2}>{line}</Label>
          </Group>
        );
      })}
      {sub ? (
        <Label y={52 + lines.length * 76} size={24} weight={400} color={C.soft} opacity={progress(f, 14, 16)}>{sub}</Label>
      ) : null}
    </Group>
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
      {Array.from({ length: 24 * 14 }, (_, i) => (
        <Rect key={i} x={20 + (i % 24) * 80} y={20 + Math.floor(i / 24) * 80} width={2} height={2} fill="#FFFFFF0D" />
      ))}
    </>
  );
}
