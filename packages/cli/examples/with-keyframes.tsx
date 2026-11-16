import { Composition, Easings, Rect, Text, keyframes, useCurrentFrame } from '@celesta.studio/react';
import { Camera } from '@celesta.studio/layout';
import { Circle } from '@celesta.studio/shapes';

// A camera that moves through keys: a push-in, a hold, a cut to another
// part of the map, and a slow drift. Each key's `easing` shapes the move
// that arrives at it.
//
//   celesta-exporter --react with-keyframes.tsx --frames 0,60,119,120,179 frame.png

const WIDTH = 1920;
const HEIGHT = 1080;

const CAMERA = [
  { frame: 0, value: { x: 960, y: 540, zoom: 1 } },
  { frame: 60, value: { x: 520, y: 380, zoom: 2.2 }, easing: Easings.easeInOutCubic },
  // The same value again holds the view for a second.
  { frame: 90, value: { x: 520, y: 380, zoom: 2.2 } },
  // Two keys on one frame cut: frame 120 already shows the second.
  { frame: 120, value: { x: 560, y: 380, zoom: 2.2 }, easing: 'ease-in-quad' as const },
  { frame: 120, value: { x: 1300, y: 700, zoom: 1.8 } },
  // A slightly different value drifts through the rest of the shot.
  { frame: 180, value: { x: 1330, y: 690, zoom: 1.9 } },
];

const PLACES = [
  { x: 520, y: 380, name: 'Harbor', color: '#5FD3B5' },
  { x: 1300, y: 700, name: 'Observatory', color: '#F2B544' },
  { x: 1180, y: 260, name: 'Market', color: '#E96A6A' },
];

function Map() {
  return (
    <>
      <Rect width={WIDTH} height={HEIGHT} fill="#0F2A3D" />
      {Array.from({ length: 16 }, (_, i) => (
        <Rect key={`v${i}`} x={i * 120 + 60} width={2} height={HEIGHT} fill="#1D4055" />
      ))}
      {Array.from({ length: 9 }, (_, i) => (
        <Rect key={`h${i}`} y={i * 120 + 60} width={WIDTH} height={2} fill="#1D4055" />
      ))}
      {PLACES.map((place) => (
        <Circle key={place.name} x={place.x} y={place.y} anchorX={0.5} anchorY={0.5} radius={26} fill={place.color} />
      ))}
      {PLACES.map((place) => (
        <Text key={place.name} x={place.x} y={place.y + 44} anchorX={0.5} fontFamily="sans-serif" fontSize={28}
          fontWeight={700} color="#EAF4F2">
          {place.name}
        </Text>
      ))}
    </>
  );
}

export default function Root() {
  const frame = useCurrentFrame();
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={30} durationInFrames={180}>
      <Camera {...keyframes(frame, CAMERA)}>
        <Map />
      </Camera>
    </Composition>
  );
}
