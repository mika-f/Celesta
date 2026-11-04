import {
  Composition,
  Easings,
  Group,
  MotionBlur,
  Rect,
  Text,
  Trail,
  defineProjectProperties,
  interpolate,
  useCurrentFrame,
} from '@celesta.studio/react';
import type { ProjectPropertySchema } from '@celesta.studio/react';
import { useProjectProperty } from '@celesta.studio/project';
import { Circle } from '@celesta.studio/shapes';

// Motion blur and a trail on fast movement. `samples` and `copies` come in
// as project properties, so the same scene renders without either:
//
//   celesta-exporter --react with-motion-blur.tsx out.mp4 --props '{"samples":1,"copies":0}'
//
// The movement repeats every two seconds; `frames` makes the video longer
// (for timing an export, say).
const schema: ProjectPropertySchema = {
  samples: { type: 'number', label: 'Samples', defaultValue: 8, min: 1, max: 64, step: 1 },
  shutterAngle: { type: 'number', label: 'Shutter angle', defaultValue: 180, min: 0, max: 720, step: 15 },
  copies: { type: 'number', label: 'Trail copies', defaultValue: 4, min: 0, max: 64, step: 1 },
  frames: { type: 'number', label: 'Frames', defaultValue: 60, min: 1, step: 1 },
};
defineProjectProperties(schema);

const WIDTH = 1920;
const HEIGHT = 1080;
const PERIOD = 60;

/** A card that dashes across in a second and a wheel that spins. */
function Dash() {
  const frame = useCurrentFrame() % PERIOD;
  const x = interpolate(frame, [4, 40], [-520, WIDTH + 40], {
    easing: Easings.easeInOutCubic,
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });
  return (
    <Group>
      <Group x={x} y={170}>
        <Rect width={480} height={180} cornerRadius={28} fill="#F2B134" />
        <Text x={240} y={90} anchorX={0.5} anchorY={0.5}
          style={{ fontSize: 76, fontWeight: 800, fill: { type: 'solid', color: '#1B1D24' } }}>
          DASH
        </Text>
      </Group>
      <Group x={1500} y={260} rotation={frame * 24} anchorX={0.5} anchorY={0.5}>
        {Array.from({ length: 6 }, (_, spoke) => (
          <Rect key={spoke} x={0} y={0} width={240} height={22} anchorX={0} anchorY={0.5}
            rotation={spoke * 60} cornerRadius={11} fill="#5BC0EB" />
        ))}
      </Group>
    </Group>
  );
}

/** A ball looping around an ellipse. */
function Orbit() {
  const frame = useCurrentFrame();
  const angle = (frame / 30) * Math.PI * 2;
  return (
    <Circle x={WIDTH / 2 + Math.cos(angle) * 640} y={760 + Math.sin(angle) * 180}
      anchorX={0.5} anchorY={0.5} radius={46} fill="#E4572E" />
  );
}

export default function Root() {
  const samples = useProjectProperty<number>('samples');
  const shutterAngle = useProjectProperty<number>('shutterAngle');
  const copies = useProjectProperty<number>('copies');
  const frames = useProjectProperty<number>('frames');
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={30} durationInFrames={frames}>
      <Rect width={WIDTH} height={HEIGHT} fill="#1B1D24" />
      <Text x={80} y={1000} style={{ fontSize: 32, fill: { type: 'solid', color: '#8A8F9C' } }}>
        {`MotionBlur samples=${samples} shutterAngle=${shutterAngle} · Trail copies=${copies}`}
      </Text>
      <MotionBlur samples={samples} shutterAngle={shutterAngle}>
        <Dash />
      </MotionBlur>
      <Trail copies={copies} lagInFrames={1.5} trailOpacity={0.5}>
        <Orbit />
      </Trail>
    </Composition>
  );
}
