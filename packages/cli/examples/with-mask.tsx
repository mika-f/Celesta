// Two uses of a group mask: video seen only through letters, then a
// soft-edged circular wipe from one scene to the next. Put a clip.mp4 next
// to this file, as for with-video.tsx.
import { Composition, Group, Mask, Rect, Sequence, Text, Video, interpolate, useCurrentFrame } from '@celesta/react';
import { Circle } from '@celesta/shapes';

const WIDTH = 1920;
const HEIGHT = 1080;

function Letters() {
  return (
    <>
      <Rect width={WIDTH} height={HEIGHT} fill="#101018" />
      <Group
        mask={
          <Text x={WIDTH / 2} y={HEIGHT / 2} anchorX={0.5} anchorY={0.5} style={{ fontSize: 360, fontWeight: 900 }}>
            CELESTA
          </Text>
        }
      >
        <Video src="./clip.mp4" />
      </Group>
    </>
  );
}

function Wipe() {
  const frame = useCurrentFrame();
  const radius = interpolate(frame, [0, 45], [0, Math.hypot(WIDTH, HEIGHT) / 2 + 80], { extrapolateRight: 'clamp' });
  return (
    <>
      <Rect width={WIDTH} height={HEIGHT} fill="#1D1B2E" />
      <Group>
        <Mask>
          <Circle x={WIDTH / 2} y={HEIGHT / 2} anchorX={0.5} anchorY={0.5} radius={radius} fill="#FFFFFF" blur={24} />
        </Mask>
        <Rect width={WIDTH} height={HEIGHT} fill="#F2C14E" />
      </Group>
    </>
  );
}

export default function WithMask() {
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={30} durationInFrames={150}>
      <Sequence durationInFrames={75}>
        <Letters />
      </Sequence>
      <Sequence from={75} durationInFrames={75}>
        <Wipe />
      </Sequence>
    </Composition>
  );
}
