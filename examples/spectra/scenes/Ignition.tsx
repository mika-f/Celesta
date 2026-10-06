// Chapter 1: blurred, screen-blended light, a starfield, rings, and a title
// that rises in behind a mask and glows.
import { Circle, Easings, Ellipse, Group, Text, TextReveal, interpolate, spring, useCurrentFrame } from '@celesta/react';

import { DIM, DISPLAY, FPS, H, INK, MONO, PALETTE, W, rand, style } from '../shared';

function Lights({ t }: { t: number }) {
  return PALETTE.map((color, j) => {
    const r = 220 + 50 * Math.sin(t * 0.9 + j);
    return (
      <Circle key={j} x={960 + Math.cos(t * 0.4 + j * 1.047) * 560} y={540 + Math.sin(t * 0.55 + j * 1.3) * 280}
        anchorX={0.5} anchorY={0.5} radius={r} fill={{
          type: 'radial', center: { x: r, y: r }, radius: r,
          stops: [{ offset: 0, color }, { offset: 1, color: `${color}00` }],
        }} opacity={0.7} blur={36} blendMode="screen" />
    );
  });
}

function Stars({ t }: { t: number }) {
  return Array.from({ length: 700 }, (_, i) => {
    const depth = 0.2 + rand(i, 1) * 0.8;
    const x = (rand(i, 2) * W + t * 120 * depth) % W;
    const y = rand(i, 3) * H;
    const size = 1 + depth * 3;
    return (
      <Circle key={i} x={x} y={y} anchorX={0.5} anchorY={0.5} radius={size}
        fill={i % 7 === 0 ? PALETTE[i % PALETTE.length] : '#FFFFFF'}
        opacity={0.25 + 0.6 * depth * (0.5 + 0.5 * Math.sin(t * 3 + i))} />
    );
  });
}

function Rings({ t }: { t: number }) {
  return (
    <Group x={960} y={520} rotation={-12} scaleY={0.42}>
      {Array.from({ length: 16 }, (_, k) => {
        const r = 260 + k * 36 + 12 * Math.sin(t * 2 + k);
        return (
          <Ellipse key={k} anchorX={0.5} anchorY={0.5} width={r * 2} height={r * 2}
            stroke={PALETTE[k % PALETTE.length]} strokeWidth={2 + (k % 3)}
            opacity={0.15 + 0.25 * (0.5 + 0.5 * Math.sin(t * 2.5 - k * 0.5))} />
        );
      })}
    </Group>
  );
}

export function Ignition() {
  const frame = useCurrentFrame();
  const t = frame / FPS;
  const pop = spring({ frame: frame - 20, fps: FPS, config: { damping: 12, stiffness: 120 } });
  const letterSpacing = interpolate(frame, [0, 120], [60, 18], { easing: Easings.easeOutCubic, extrapolateRight: 'clamp' });
  return (
    <Group>
      <Stars t={t} />
      <Lights t={t} />
      <Rings t={t} />
      <Group x={960} y={430} scale={0.85 + 0.15 * pop} glow={{ color: '#7B5CFFC0', blur: 28 }}
        shadow={{ color: '#000000A0', blur: 18, offsetX: 0, offsetY: 14 }}>
        <TextReveal x={0} y={-120} align={0.5} lineHeight={240} from={8} durationInFrames={36}
          style={style(DISPLAY, 260, INK, { letterSpacing })}>
          SPECTRA
        </TextReveal>
      </Group>
      <TextReveal x={960} y={600} align={0.5} lineHeight={40} from={30} stagger={8}
        style={style(MONO, 26, DIM, { letterSpacing: 6 })}>
        {'A CELESTA PERFORMANCE REEL\nBLUR · BLEND · CLIP · PATH · TEXT · CODE · IMAGE'}
      </TextReveal>
      <Text x={960} y={760} anchorX={0.5} anchorY={0.5} opacity={interpolate(frame, [60, 90], [0, 1], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' })}
        style={style(MONO, 18, '#5C6694', { letterSpacing: 4 })}>
        {`T+${t.toFixed(3)}s`}
      </Text>
    </Group>
  );
}
