import type { ReactNode } from 'react';
import { Easings, Group, Rect, interpolate, spring, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { C, PANEL, SAFE, SAFE_W, W } from '../constants';
import { Label } from './Label';

// The content area under a panel's heading, in canvas coordinates. Text
// stays between SAFE.left and SAFE.right; pictures may reach past it.
export const BODY = { x: SAFE.left, y: SAFE.top + 120, w: SAFE_W, h: PANEL.h - SAFE.top - 120 - 48 } as const;

// The top half's surface, edge to edge, drawn once for the whole video: a
// paper sheet whose bottom edge is a scalloped ink line.
export function PanelSurface({ accent }: { accent: string }) {
  const bumps = 9;
  const r = W / bumps / 2;
  const scallops = (dy: number, fill: string) => Array.from({ length: bumps }, (_, i) => (
    <Rect key={`${dy}-${i}`} x={r + i * r * 2} y={PANEL.h + dy} anchorX={0.5} anchorY={0.5} width={r * 2} height={r * 2}
      cornerRadius={r} fill={fill} />
  ));
  return (
    <>
      {scallops(12, '#1B1E2B1F')}
      <Rect width={W} height={PANEL.h} fill={C.paper} />
      {scallops(0, C.paper)}
      <Rect y={0} width={W} height={14} fill={accent} />
    </>
  );
}

// Children drawn in canvas coordinates, scaled about the point (x, y).
export function ScaleAbout({ x, y, scale, opacity = 1, rotation = 0, children }: {
  x: number; y: number; scale: number; opacity?: number; rotation?: number; children: ReactNode;
}) {
  return (
    <Group x={x} y={y} scale={scale} rotation={rotation} opacity={opacity}>
      <Group x={-x} y={-y}>{children}</Group>
    </Group>
  );
}

type PanelProps = {
  // "01", "02"… shown in the colored chip; omit for a panel without a heading.
  index?: string;
  title?: string;
  accent: string;
  children: ReactNode;
};

// One scene's explainer: the heading slides in, the content pops in on the
// scene's first frames and drops away on its last few, so scenes cut on the voices.
export function Panel({ index, title, accent, children }: PanelProps) {
  const frame = useCurrentFrame();
  const { fps, durationInFrames } = useVideoConfig();
  const pop = spring({ frame, fps, config: { damping: 13, stiffness: 190 } });
  const out = interpolate(frame, [durationInFrames - 5, durationInFrames], [0, 1], {
    easing: Easings.easeInCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp',
  });
  const head = interpolate(frame, [0, 8], [-60, 0], {
    easing: Easings.easeOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp',
  });
  return (
    <Group opacity={1 - out}>
      {title && (
        <Group x={head} opacity={Math.min(1, frame / 4)}>
          <Rect x={SAFE.left} y={SAFE.top} width={112} height={68} cornerRadius={34} fill={accent} />
          <Label x={SAFE.left + 56} y={SAFE.top + 34} ax={0.5} ay={0.5} size={38} font="mono" weight={800} color={C.paper}>{index ?? ''}</Label>
          <Label x={SAFE.left + 136} y={SAFE.top + 36} ay={0.5} size={54} font="display" weight={400}>{title}</Label>
        </Group>
      )}
      <ScaleAbout x={W / 2} y={BODY.y + BODY.h / 2} scale={(0.9 + 0.1 * pop) * (1 - 0.05 * out)}
        opacity={Math.min(1, pop * 1.5)}>
        {children}
      </ScaleAbout>
    </Group>
  );
}
