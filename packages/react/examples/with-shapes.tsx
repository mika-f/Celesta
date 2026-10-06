import { Arrow, Circle, Composition, Ellipse, Group, Rect, interpolate, useCurrentFrame } from '@celesta/react';

// Circles, ellipses, and arrows for a diagram. A circle is one rounded
// `<Rect>` layer; an ellipse or an arrow is one `<Path>` layer.
// Like `<Rect>`, `x`/`y` place a circle's or ellipse's anchor point (its
// top-left unless `anchorX`/`anchorY` say otherwise) and a stroke stays
// inside its box. An arrow runs from `x1`/`y1` to its head at `x2`/`y2`.
export default function Root() {
  const frame = useCurrentFrame();
  const grow = interpolate(frame, [0, 45], [0, 1], { extrapolateRight: 'clamp' });
  const spin = frame * 2;
  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={90}>
      <Rect width={1280} height={720} fill="#101820" />

      {/* Circles: fill, inner stroke, radial gradient, glow, and a growing radius. */}
      <Circle x={80} y={60} radius={60} fill="#3366CC" />
      <Rect x={240} y={60} width={120} height={120} stroke="#FFFFFF40" strokeWidth={1} />
      <Circle x={240} y={60} radius={60} stroke="#FFD84D" strokeWidth={12} />
      <Circle x={400} y={60} radius={60}
        fill={{ type: 'radial', center: { x: 40, y: 40 }, radius: 90,
          stops: [{ offset: 0, color: '#FFFFFF' }, { offset: 1, color: '#EF402B' }] }} />
      <Circle x={620} y={120} anchorX={0.5} anchorY={0.5} radius={50} fill="#4DD8C0"
        glow={{ color: '#4DD8C0', blur: 24 }} />
      <Circle x={780} y={120} anchorX={0.5} anchorY={0.5} radius={60 * grow} fill="#FFFFFF" opacity={0.8} />

      {/* Ellipses: sizes, rotation about the centre, linear gradient, shadow, blend. */}
      <Ellipse x={80} y={240} width={200} height={100} fill="#8E6CFF" />
      <Ellipse x={420} y={290} anchorX={0.5} anchorY={0.5} width={160} height={60} rotation={spin}
        stroke="#FFFFFF" strokeWidth={4} />
      <Ellipse x={540} y={240} width={80} height={140}
        fill={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: 140 },
          stops: [{ offset: 0, color: '#FFD84D' }, { offset: 1, color: '#FFD84D00' }] }} />
      <Ellipse x={660} y={250} width={180} height={90} fill="#EF402B"
        shadow={{ color: '#000000', blur: 12, offsetX: 6, offsetY: 8 }} />
      <Ellipse x={760} y={260} width={180} height={90} fill="#3366CC" blendMode="screen" />

      {/* Arrows: every direction, both kinds of heads, a gradient, and a growing tip. */}
      <Group x={180} y={540}>
        {Array.from({ length: 12 }, (_, i) => {
          const angle = (i * 30 * Math.PI) / 180;
          return <Arrow key={i} x1={0} y1={0} x2={Math.cos(angle) * 110} y2={Math.sin(angle) * 110}
            strokeWidth={3} stroke={i % 3 === 0 ? '#FFD84D' : '#FFFFFF'} />;
        })}
      </Group>
      <Arrow x1={360} y1={460} x2={620} y2={460} strokeWidth={6} heads="end" stroke="#4DD8C0" />
      <Arrow x1={360} y1={530} x2={620} y2={530} strokeWidth={6} heads="start" stroke="#8E6CFF" />
      <Arrow x1={360} y1={600} x2={620} y2={600} strokeWidth={6} heads="both" headLength={30} headWidth={18}
        stroke={{ type: 'linear', start: { x: 360, y: 0 }, end: { x: 620, y: 0 },
          stops: [{ offset: 0, color: '#3366CC' }, { offset: 1, color: '#EF402B' }] }} />
      <Arrow x1={680} y1={460} x2={680 + 400 * grow} y2={620 - 160 * grow} strokeWidth={10}
        stroke="#FFFFFFB0" />
      {/* Shorter than its heads: they shrink to fit, keeping their shape. */}
      <Arrow x1={1140} y1={460} x2={1160} y2={460} strokeWidth={6} heads="both" stroke="#FFD84D" />
      <Arrow x1={1140} y1={520} x2={1146} y2={520} strokeWidth={6} stroke="#FFD84D" />
    </Composition>
  );
}
