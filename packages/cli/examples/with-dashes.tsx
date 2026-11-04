import { Composition, Rect, interpolate, useCurrentFrame } from '@celesta.studio/react';
import { Arrow, Circle, Ellipse, Line, Path, Polyline } from '@celesta.studio/shapes';

// Dashed and dotted strokes. `dash` lists the lengths of the dashes and the
// gaps between them, in turn; `dashOffset` shifts them along the line, so
// animating it makes them flow. Zero-length dashes with round caps are dots.
export default function Root() {
  const frame = useCurrentFrame();
  const draw = interpolate(frame, [0, 60], [0, 1], { extrapolateRight: 'clamp' });
  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={90}>
      <Rect width={1280} height={720} fill="#101820" />

      {/* Lines: dashes, dots, square dots, dashes that flow towards the end. */}
      <Line x1={80} y1={80} x2={560} y2={80} strokeWidth={6} cap="butt" dash={[28, 14]} />
      <Line x1={80} y1={140} x2={560} y2={140} strokeWidth={10} stroke="#FFD84D" dash={[0, 22]} />
      <Line x1={80} y1={200} x2={560} y2={200} strokeWidth={8} stroke="#4DD8C0" cap="square" dash={[0, 20]} />
      <Line x1={80} y1={260} x2={560} y2={260} strokeWidth={6} stroke="#EF402B" cap="butt"
        dash={[40, 10, 6, 10]} dashOffset={-frame * 3} />

      {/* Drawn on with `progress`: the dashes stay where they are as it grows. */}
      <Polyline points={[[660, 60], [820, 200], [960, 90], [1200, 260]]} strokeWidth={6} stroke="#8E6CFF"
        cap="butt" join="round" dash={[24, 12]} progress={draw} />
      <Path stroke="#FFFFFF" strokeWidth={4} cap="round" dash={[0, 14, 30, 14]} dashOffset={frame * 2}
        commands={[{ type: 'moveTo', x: 660, y: 300 }, { type: 'cubicTo', x1: 780, y1: 160, x2: 1060, y2: 420, x: 1200, y: 300 }]} />

      {/* Closed shapes run their dashes clockwise. */}
      <Circle x={180} y={480} anchorX={0.5} anchorY={0.5} radius={90} stroke="#FFD84D" strokeWidth={10}
        cap="round" dash={[0, 26]} rotation={frame} />
      <Ellipse x={420} y={480} anchorX={0.5} anchorY={0.5} width={220} height={140} fill="#3366CC"
        stroke="#FFFFFF" strokeWidth={6} dash={[20, 10]} dashOffset={-frame} />
      <Rect x={560} y={400} width={220} height={160} cornerRadius={24} fill="#EF402B40"
        stroke="#EF402B" strokeWidth={6} dash={[30, 12]} />
      {/* Marching ants around a selection. */}
      <Rect x={820} y={400} width={180} height={160} stroke="#FFFFFF" strokeWidth={2} dash={[8, 8]}
        dashOffset={frame} />

      {/* An arrow dashes its shaft; its head stays whole. */}
      <Arrow x1={1040} y1={620} x2={1220} y2={420} strokeWidth={6} stroke="#4DD8C0" dash={[18, 10]}
        dashOffset={-frame * 2} />
      <Arrow x1={80} y1={650} x2={1000} y2={650} strokeWidth={4} heads="both" stroke="#FFFFFFB0" dash={[12, 8]} />
    </Composition>
  );
}
