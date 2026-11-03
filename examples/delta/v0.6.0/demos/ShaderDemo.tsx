import { Group, Rect, Text } from '@celesta.studio/react';
import { Delta } from '../../components/Delta';
import { enter, STAGE } from '../../components/ChapterShell';
import type { DemoProps } from '../../components/ChapterShell';
import { glitch, pixelate, ripple } from '../../components/shaders';
import { C, FPS, FONT, solid } from '../../components/theme';

// Three WGSL filters, each a few lines in components/shaders.ts, running live:
// a ripple on the headline, and a glitch and a pixelation on the cards.
const CARD = { width: 250, height: 190 };

function Card({ x, label, frame, shaded }: { x: number; label: string; frame: number; shaded: (t: number) => object }) {
  const p = enter(frame, 20 + x / 30);
  return (
    <Group x={x} y={496 + 40 * (1 - p)} opacity={p}>
      <Group {...shaded(frame / FPS)}>
        <Rect width={CARD.width} height={CARD.height} cornerRadius={12}
          fill={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: CARD.width, y: CARD.height },
            stops: [{ offset: 0, color: C.orange }, { offset: 1, color: C.violet }] }} />
        <Delta x={CARD.width / 2} y={CARD.height / 2} size={90} fill={C.paper} />
      </Group>
      <Text x={0} y={CARD.height + 30} anchorY={0.5} style={{ fontFamily: FONT.mono, fontSize: 20, fill: solid(C.grey) }}>
        {label}
      </Text>
    </Group>
  );
}

export function ShaderDemo({ frame }: DemoProps) {
  const t = frame / FPS;
  const p = enter(frame, -4, 24);
  const gap = (STAGE.width - 3 * CARD.width) / 4;
  return (
    <Group>
      <Rect width={STAGE.width} height={STAGE.height} fill={C.ink} />
      <Group x={STAGE.width / 2} y={250} opacity={p} scale={0.9 + 0.1 * p}
        shader={ripple({ time: t, amplitude: 4 + 3 * Math.sin(t * 2), wavelength: 110 })}>
        <Text anchorX={0.5} anchorY={0.5}
          style={{ fontFamily: FONT.display, fontSize: 300, letterSpacing: { em: 0.04 }, fill: solid(C.paper) }}>
          WGSL
        </Text>
      </Group>
      <Text x={STAGE.width / 2} y={420} anchorX={0.5} anchorY={0.5} opacity={p}
        style={{ fontFamily: FONT.mono, fontSize: 20, fill: solid(C.dim) }}>
        {`ripple({ time: ${t.toFixed(2)} })`}
      </Text>
      <Card x={gap} label="ripple()" frame={frame}
        shaded={(time) => ({ shader: ripple({ time: time * 1.3, amplitude: 6, wavelength: 40 }) })} />
      <Card x={gap * 2 + CARD.width} label="glitch()" frame={frame}
        shaded={(time) => ({ shader: glitch({ amount: 26 * Math.max(0, Math.sin(time * 3)), split: 8, seed: Math.floor(time * 10) }) })} />
      <Card x={gap * 3 + CARD.width * 2} label="pixelate()" frame={frame}
        shaded={(time) => ({ shader: pixelate({ size: 2 + 14 * (0.5 + 0.5 * Math.sin(time * 2.4)) }) })} />
    </Group>
  );
}
