import { Assets, Audio, Composition, Font, Group, Rect, Text, useCurrentFrame } from '@celesta.studio/react';
import { FPS, FRAMES, HEIGHT, INK, PAPER, WIDTH, tunnelAt } from './motion';

function Label({ children, x, y, size = 18, align = 'left' }: {
  children: string; x: number; y: number; size?: number; align?: 'left' | 'right';
}) {
  return <Text x={x} y={y} anchorX={align === 'right' ? 1 : 0} style={{
    fontFamily: 'IBM Plex Mono', fontSize: size,
    fill: { type: 'solid', color: INK },
  }}>{children}</Text>;
}

function Tunnel() {
  const frame = useCurrentFrame();
  const { center, shells } = tunnelAt(frame);
  return <Group x={54} y={260} clip={{ x: 0, y: 0, width: 972, height: 972 }}>
    <Rect width={972} height={972} fill={INK} />
    {shells.map(({ index, radius, rotation, round, color, detail }) => (
      <Group key={index} x={center.x} y={center.y} rotation={rotation}>
        <Rect width={radius * 2} height={radius * 2}
          anchorX={0.5} anchorY={0.5} cornerRadius={round ? radius : 0} fill={color} />
        {radius > 24 && radius < 1800 && <>
          <Rect width={radius * 1.87} height={radius * 1.87}
            anchorX={0.5} anchorY={0.5} cornerRadius={round ? radius : 0}
            stroke={detail} strokeWidth={Math.max(1.1, radius * 0.004)} opacity={0.48} />
          {round && [0, 90, 180, 270].map((angle) => (
            <Group key={angle} rotation={angle}>
              <Rect x={radius * 0.875} width={radius * 0.024} height={radius * 0.024}
                anchorX={0.5} anchorY={0.5} cornerRadius={radius} fill={detail} />
            </Group>
          ))}
        </>}
      </Group>
    ))}
  </Group>;
}

function Poster() {
  const frame = useCurrentFrame();
  const angle = frame / FRAMES * 360;
  return <>
    <Rect width={WIDTH} height={HEIGHT} fill={PAPER} />
    <Label x={54} y={34}>CELESTA / MOTION STUDY</Label>
    <Label x={1026} y={34} align="right">VOL. 01</Label>
    <Text x={48} y={68} style={{ fontFamily: 'Bebas Neue', fontSize: 166,
      fill: { type: 'solid', color: INK } }}>PERFECT LOOP</Text>
    <Label x={54} y={218}>A LITTLE FURTHER. AGAIN.</Label>
    <Group x={994} y={222} rotation={angle}>
      <Rect width={26} height={26} anchorX={0.5} anchorY={0.5} stroke={INK} strokeWidth={2} />
      <Rect width={8} height={8} cornerRadius={4} anchorX={0.5} anchorY={0.5} fill="#F55138" />
    </Group>
    <Tunnel />
    <Rect x={54} y={1265} width={972} height={1.5} fill={INK} />
    <Label x={54} y={1284}>NO BEGINNING. NO END.</Label>
    <Label x={1026} y={1284} align="right">16 SECONDS / FOREVER</Label>
  </>;
}

export default function PerfectLoop() {
  return <Composition width={WIDTH} height={HEIGHT} fps={FPS} durationInFrames={FRAMES}>
    <Assets>
      <Font src="./assets/fonts/BebasNeue-Regular.ttf" />
      <Font src="./assets/fonts/IBMPlexMono-Regular.ttf" />
    </Assets>
    <Poster />
    <Audio src="./assets/score.wav" />
  </Composition>;
}
