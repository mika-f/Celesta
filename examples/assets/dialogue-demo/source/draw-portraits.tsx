// Draws the dialogue demo's two original characters, one image per frame.
// The renderer always paints an opaque background, so every image is drawn
// twice, first on black and then on white; `generate.sh` exports both and
// `matte.py` recovers the transparent PNG from the pair, named after the
// entry in IMAGES below. Mouths are separate images of the same size, so a
// lip-sync shape lines up when drawn over either portrait.
import * as React from 'react';

import { Composition, Group, Path, Rect, useCurrentFrame } from '@celesta/react';
import type { PathCommand } from '@celesta/react';

export const WIDTH = 360;
export const HEIGHT = 400;
const MOUTH_X = 180;
const MOUTH_Y = 298;

type Part = 'normal' | 'smile' | 'a' | 'i' | 'u' | 'e' | 'o' | 'closed';

/** File names, in frame order. */
export const IMAGES = [
  'shizuku-normal', 'shizuku-smile', 'komugi-normal', 'komugi-smile',
  'mouth-a', 'mouth-i', 'mouth-u', 'mouth-e', 'mouth-o', 'mouth-closed',
] as const;

const K = 0.5523;
function ellipse(cx: number, cy: number, rx: number, ry: number): PathCommand[] {
  return [
    { type: 'moveTo', x: cx + rx, y: cy },
    { type: 'cubicTo', x1: cx + rx, y1: cy + ry * K, x2: cx + rx * K, y2: cy + ry, x: cx, y: cy + ry },
    { type: 'cubicTo', x1: cx - rx * K, y1: cy + ry, x2: cx - rx, y2: cy + ry * K, x: cx - rx, y: cy },
    { type: 'cubicTo', x1: cx - rx, y1: cy - ry * K, x2: cx - rx * K, y2: cy - ry, x: cx, y: cy - ry },
    { type: 'cubicTo', x1: cx + rx * K, y1: cy - ry, x2: cx + rx, y2: cy - ry * K, x: cx + rx, y: cy },
    { type: 'close' },
  ];
}

const INK = '#2b2340';

function Eyes({ smile, y }: { smile: boolean; y: number }) {
  if (smile) {
    // Closed, happy eyes: two upward arcs.
    return (
      <>
        {[135, 225].map((x) => (
          <Path key={x} stroke={INK} strokeWidth={8} cap="round"
            commands={[{ type: 'moveTo', x: x - 18, y: y + 6 }, { type: 'quadTo', x1: x, y1: y - 18, x: x + 18, y: y + 6 }]} />
        ))}
      </>
    );
  }
  return (
    <>
      {[135, 225].map((x) => (
        <React.Fragment key={x}>
          <Path fill={INK} commands={ellipse(x, y, 13, 19)} />
          <Path fill="#ffffff" commands={ellipse(x + 4, y - 7, 5, 6)} />
        </React.Fragment>
      ))}
    </>
  );
}

function Cheeks({ color }: { color: string }) {
  return (
    <>
      <Path fill={color} commands={ellipse(104, 284, 22, 12)} />
      <Path fill={color} commands={ellipse(256, 284, 22, 12)} />
    </>
  );
}

/** A water-drop girl. */
function Shizuku({ smile }: { smile: boolean }) {
  return (
    <>
      <Path
        fill={{ type: 'linear', start: { x: 0, y: 30 }, end: { x: 0, y: 385 }, stops: [{ offset: 0, color: '#bfe9ff' }, { offset: 1, color: '#5fb4f0' }] }}
        stroke="#2f6fa8"
        strokeWidth={7}
        join="round"
        commands={[
          { type: 'moveTo', x: 180, y: 28 },
          { type: 'cubicTo', x1: 225, y1: 110, x2: 315, y2: 165, x: 315, y: 250 },
          { type: 'cubicTo', x1: 315, y1: 325, x2: 255, y2: 384, x: 180, y: 384 },
          { type: 'cubicTo', x1: 105, y1: 384, x2: 45, y2: 325, x: 45, y: 250 },
          { type: 'cubicTo', x1: 45, y1: 165, x2: 135, y2: 110, x: 180, y: 28 },
          { type: 'close' },
        ]}
      />
      <Path fill="#ffffffb0" commands={ellipse(108, 182, 16, 30)} />
      {/* A ribbon on the side. */}
      <Path fill="#ff7aa8" stroke="#c2406f" strokeWidth={4} join="round" commands={[
        { type: 'moveTo', x: 244, y: 150 }, { type: 'lineTo', x: 212, y: 128 }, { type: 'lineTo', x: 214, y: 172 }, { type: 'close' },
        { type: 'moveTo', x: 244, y: 150 }, { type: 'lineTo', x: 278, y: 134 }, { type: 'lineTo', x: 272, y: 178 }, { type: 'close' }]} />
      <Path fill="#ff9cc0" stroke="#c2406f" strokeWidth={4} commands={ellipse(244, 152, 10, 10)} />
      <Eyes smile={smile} y={248} />
      <Cheeks color="#ff8fb180" />
    </>
  );
}

/** A round bread-bun boy with a wheat sprout. */
function Komugi({ smile }: { smile: boolean }) {
  return (
    <>
      {/* Sprout. */}
      <Path stroke="#5b8a2e" strokeWidth={7} cap="round" commands={[{ type: 'moveTo', x: 180, y: 112 }, { type: 'quadTo', x1: 176, y1: 80, x: 190, y: 52 }]} />
      <Path fill="#8cc152" stroke="#5b8a2e" strokeWidth={4} commands={[
        { type: 'moveTo', x: 186, y: 70 }, { type: 'quadTo', x1: 220, y1: 40, x: 250, y: 58 }, { type: 'quadTo', x1: 222, y1: 86, x: 186, y: 70 }, { type: 'close' }]} />
      <Path fill="#8cc152" stroke="#5b8a2e" strokeWidth={4} commands={[
        { type: 'moveTo', x: 182, y: 84 }, { type: 'quadTo', x1: 150, y1: 58, x: 122, y: 74 }, { type: 'quadTo', x1: 150, y1: 102, x: 182, y: 84 }, { type: 'close' }]} />
      <Path
        fill={{ type: 'linear', start: { x: 0, y: 105 }, end: { x: 0, y: 385 }, stops: [{ offset: 0, color: '#f6c47a' }, { offset: 1, color: '#e08a3c' }] }}
        stroke="#9a5420"
        strokeWidth={7}
        commands={ellipse(180, 246, 152, 138)}
      />
      {/* Baked crust highlight and score marks. */}
      <Path fill="#fff1d0a0" commands={ellipse(180, 150, 90, 26)} />
      {[140, 180, 220].map((x) => (
        <Path key={x} stroke="#b8692a" strokeWidth={6} cap="round"
          commands={[{ type: 'moveTo', x: x - 14, y: 172 }, { type: 'lineTo', x: x + 14, y: 160 }]} />
      ))}
      {/* Eyebrows. */}
      {[135, 225].map((x) => (
        <Path key={`b${x}`} stroke="#7a3f15" strokeWidth={6} cap="round"
          commands={[{ type: 'moveTo', x: x - 15, y: 212 }, { type: 'lineTo', x: x + 15, y: 208 }]} />
      ))}
      <Eyes smile={smile} y={246} />
      <Cheeks color="#ff7b5c70" />
    </>
  );
}

const MOUTH_FILL = '#8a2b3a';
const TONGUE = '#ff8a9a';

function Mouth({ shape }: { shape: Part }) {
  const x = MOUTH_X;
  const y = MOUTH_Y;
  if (shape === 'closed') {
    return <Path stroke={INK} strokeWidth={6} cap="round"
      commands={[{ type: 'moveTo', x: x - 18, y: y - 3 }, { type: 'quadTo', x1: x, y1: y + 12, x: x + 18, y: y - 3 }]} />;
  }
  const size = { a: [24, 22], i: [28, 9], u: [11, 12], e: [24, 14], o: [17, 22] }[shape as 'a'];
  const [rx, ry] = size;
  return (
    <Group>
      <Path fill={MOUTH_FILL} stroke={INK} strokeWidth={4} commands={ellipse(x, y, rx, ry)} />
      {ry >= 14 ? <Path fill={TONGUE} commands={ellipse(x, y + ry * 0.45, rx * 0.6, ry * 0.4)} /> : null}
    </Group>
  );
}

function Image({ name }: { name: string }) {
  const [who, part] = name.split('-') as [string, Part];
  if (who !== 'mouth') {
    return who === 'shizuku' ? <Shizuku smile={part === 'smile'} /> : <Komugi smile={part === 'smile'} />;
  }
  return <Mouth shape={part} />;
}

function Frame() {
  const frame = useCurrentFrame();
  const onWhite = frame >= IMAGES.length;
  return (
    <>
      <Rect width={WIDTH} height={HEIGHT} fill={onWhite ? '#ffffff' : '#000000'} />
      <Image name={IMAGES[frame % IMAGES.length]} />
    </>
  );
}

export default function Root() {
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={30} durationInFrames={IMAGES.length * 2}>
      <Frame />
    </Composition>
  );
}
