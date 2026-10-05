import { Composition, Easings, Rect, Text, interpolateColor, useCurrentFrame } from '@celesta/react';

// interpolateColor() blends #RRGGBB / #RRGGBBAA colors over the frame like
// interpolate() blends numbers. Frames outside the range hold the end color.
export default function Root() {
  const frame = useCurrentFrame();

  // Three colors, one segment each half: night, then dawn, then day.
  const sky = interpolateColor(frame, [0, 45, 90], ['#101820', '#EF7B45', '#7FC8F8']);

  // A gradient whose stops change color on their own clocks; the top stop
  // also fades in from fully transparent.
  const glowTop = interpolateColor(frame, [0, 60], ['#FFD84D00', '#FFD84DFF'], { easing: Easings.easeOutCubic });
  const glowBottom = interpolateColor(frame, [30, 90], ['#3366CC', '#EF402B']);

  // Text fades in from transparent white, then shifts toward yellow.
  const title = interpolateColor(frame, [0, 20, 90], ['#FFFFFF00', '#FFFFFF', '#FFD84D']);

  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={90}>
      <Rect width={1280} height={720} fill={sky} />
      <Rect x={440} y={160} width={400} height={400} cornerRadius={200}
        fill={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: 400 },
          stops: [{ offset: 0, color: glowTop }, { offset: 1, color: glowBottom }] }} />
      <Text x={640} y={640} anchorX={0.5}
        style={{ fontSize: 64, fontWeight: 700, fill: { type: 'solid', color: title } }}>
        interpolateColor
      </Text>
    </Composition>
  );
}
