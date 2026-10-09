// Chapter 4: a camera pushes in on an editor card typing highlighted code,
// beside a contact sheet of portraits drawn with cover and contain.
import { Code, codeThemes } from '@celesta/code';
import { Easings, Group, Image, Rect, Text, interpolate, useCurrentFrame } from '@celesta/react';
import { Camera } from '@celesta/layout';
import { useTypewriter } from '@celesta/text';

import { DIM, FPS, INK, MONO, PALETTE, style } from '../shared';

const SOURCE = `import { Composition, Rect, useCurrentFrame } from '@celesta/react';

export default function Film() {
  const frame = useCurrentFrame();
  const t = frame / 60;
  return (
    <Composition width={1920} height={1080} fps={60} durationInFrames={600}>
      {Array.from({ length: 1500 }, (_, i) => (
        <Rect key={i} x={960 + Math.cos(t + i) * i * 0.6}
          y={540 + Math.sin(t + i) * i * 0.3}
          width={6} height={6} cornerRadius={3} blur={i % 9} />
      ))}
    </Composition>
  );
}`;

const PORTRAITS = [
  '../assets/dialogue-demo/portraits/komugi-normal.png',
  '../assets/dialogue-demo/portraits/shizuku-normal.png',
  '../assets/dialogue-demo/portraits/komugi-smile.png',
  '../assets/dialogue-demo/portraits/shizuku-smile.png',
];

function Editor({ frame }: { frame: number }) {
  const { length } = useTypewriter(SOURCE, { from: 4, framesPerChar: 0.22 });
  const lines = SOURCE.slice(0, Math.min(length, SOURCE.length)).split('\n');
  const line = lines.length;
  const column = lines[lines.length - 1].length + 1;
  return (
    <Group x={120} y={150}>
      <Rect width={1080} height={760} cornerRadius={20} fill="#0D1117" stroke="#30363D" strokeWidth={2}
        shadow={{ color: '#000000C0', blur: 40, offsetX: 0, offsetY: 24 }} glow={{ color: '#3DA5FF50', blur: 20 }} />
      <Rect width={1080} height={48} cornerRadius={20} fill="#161B22" />
      {['#FF5F57', '#FEBC2E', '#28C840'].map((color, i) => (
        <Rect key={color} x={24 + i * 26} y={16} width={16} height={16} cornerRadius={8} fill={color} />
      ))}
      <Text x={540} y={32} anchorX={0.5} anchorY="baseline" style={style(MONO, 18, DIM)}>film.tsx</Text>
      <Code x={36} y={96} language="tsx" theme={codeThemes.dark} visibleCharacters={length}
        highlightLines={[line]} highlightWidth={1008} style={{ fontFamily: MONO, fontSize: 21, lineHeight: 36 }}>
        {SOURCE}
      </Code>
      <Text x={1056} y={742} anchorX={1} anchorY="baseline" style={style(MONO, 16, DIM)}>
        {`Ln ${line}, Col ${column} · frame ${frame}`}
      </Text>
    </Group>
  );
}

function Sheet({ t }: { t: number }) {
  return (
    <Group x={1280} y={150}>
      {PORTRAITS.map((src, i) => {
        const zoom = 1 + 0.08 * Math.sin(t * 1.4 + i);
        return (
          <Group key={i} x={(i % 2) * 260} y={Math.floor(i / 2) * 390}
            clip={{ width: 240, height: 360, cornerRadius: 18 }}>
            <Rect width={240} height={360} fill={{
              type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: 360 },
              stops: [{ offset: 0, color: PALETTE[i] }, { offset: 1, color: '#0B0E24' }],
            }} />
            <Image src={src} x={120} y={190} anchorX={0.5} anchorY={0.5} width={240} height={360}
              fit={i % 2 === 0 ? 'cover' : 'contain'} scale={zoom} rotation={2 * Math.sin(t + i)} />
            <Text x={14} y={344} anchorY="baseline" style={style(MONO, 16, INK)}>
              {i % 2 === 0 ? 'fit=cover' : 'fit=contain'}
            </Text>
          </Group>
        );
      })}
    </Group>
  );
}

export function Source() {
  const frame = useCurrentFrame();
  const t = frame / FPS;
  const zoom = interpolate(frame, [0, 140], [1.12, 0.94], { easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  return (
    <Camera x={interpolate(frame, [0, 140], [700, 960], { extrapolateRight: 'clamp' })} y={540} zoom={zoom}
      rotation={interpolate(frame, [0, 140], [-2, 0], { extrapolateRight: 'clamp' })} shake={3} seed="source">
      <Editor frame={frame} />
      <Sheet t={t} />
    </Camera>
  );
}
