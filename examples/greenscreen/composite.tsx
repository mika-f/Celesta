import { Composition, Group, Image, Rect, Text, Video, interpolate, useCurrentFrame } from '@celesta/react';

// Keys the green-screen footage (footage.tsx, exported to assets/) over a
// night skyline. The presenter's video is keyed with chromaKey; its spill
// suppression rises from 0 to 1 over the first two seconds, so the green
// cast on the presenter's edges fades as it does. The raw footage plays in
// the corner, and a still of it is keyed through <Image> on the right.

const KEY = '#00b140';

export default function Composite() {
  const frame = useCurrentFrame();
  const spill = interpolate(frame, [0, 60], [0, 1], { extrapolateRight: 'clamp' });
  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={90}>
      <Skyline frame={frame} />
      <Video
        src="./assets/footage.mp4"
        x={120}
        y={180}
        scale={1.5}
        chromaKey={{ color: KEY, tolerance: 0.2, softness: 0.1, spill }}
      />
      <Image src="./assets/still.jpg" x={980} y={420} width={256} chromaKey={KEY} />
      {/* The raw footage, for comparison. */}
      <Group x={32} y={32}>
        <Video src="./assets/footage.mp4" width={224} />
        <Text y={134} style={{ ...LABEL, fontSize: 18 }}>raw footage</Text>
      </Group>
      <Text x={1248} y={40} anchorX={1} style={LABEL}>{`chromaKey spill ${spill.toFixed(2)}`}</Text>
    </Composition>
  );
}

const LABEL = { fontFamily: 'sans-serif', fontSize: 24, fill: { type: 'solid' as const, color: '#e8e8f0' } };

/** A night sky and lit windows, drifting slowly. */
function Skyline({ frame }: { frame: number }) {
  const buildings = [
    [0, 380, 180], [170, 300, 140], [300, 420, 200], [490, 260, 160],
    [640, 360, 220], [850, 320, 150], [990, 400, 170], [1150, 290, 130],
  ];
  return (
    <>
      <Rect
        width={1280}
        height={720}
        fill={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: 720 }, stops: [{ offset: 0, color: '#0b1030' }, { offset: 1, color: '#3a2a5a' }] }}
      />
      <Rect x={980} y={80} width={90} height={90} cornerRadius={45} fill="#f4ecc8" glow={{ color: '#f4ecc880', blur: 30 }} />
      <Group x={-frame / 3}>
        {buildings.map(([x, height, width]) => (
          <Group key={x} x={x} y={720 - height}>
            <Rect width={width} height={height} fill="#141830" />
            {Array.from({ length: Math.floor(height / 40) * Math.floor(width / 36) }, (_, index) => {
              const columns = Math.floor(width / 36);
              const lit = (index * 7 + x) % 5 < 2;
              return lit ? (
                <Rect key={index} x={12 + (index % columns) * 36} y={16 + Math.floor(index / columns) * 40} width={16} height={20} fill="#f0c860" />
              ) : null;
            })}
          </Group>
        ))}
      </Group>
    </>
  );
}
