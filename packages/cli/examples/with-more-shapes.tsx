import { Composition, Rect, interpolate, useCurrentFrame } from '@celesta.studio/react';
import { Heart, Path, Pie, Polygon, Star, Triangle, piePath, starPath } from '@celesta.studio/shapes';
import type { PathCommand } from '@celesta.studio/shapes';

// Triangles, polygons, stars, pie slices, and hearts, each one `<Path>`
// layer. Like `<Rect>`, `x`/`y` place a shape's anchor point (its box's
// top-left unless `anchorX`/`anchorY` say otherwise). Polygons, stars, and
// pies are sized by `radius` like a `<Circle>`; triangles and hearts fill a
// `width` × `height` box. The stroke is centred on the outline, as for
// `<Path>`. Each shape also has a function returning its path commands.

/** `commands` with every point passed through `move`. */
function mapPoints(commands: readonly PathCommand[], move: (x: number, y: number) => [number, number]): PathCommand[] {
  return commands.map((command) => {
    switch (command.type) {
      case 'close':
        return command;
      case 'moveTo':
      case 'lineTo': {
        const [x, y] = move(command.x, command.y);
        return { ...command, x, y };
      }
      case 'quadTo': {
        const [x1, y1] = move(command.x1, command.y1);
        const [x, y] = move(command.x, command.y);
        return { ...command, x1, y1, x, y };
      }
      case 'cubicTo': {
        const [x1, y1] = move(command.x1, command.y1);
        const [x2, y2] = move(command.x2, command.y2);
        const [x, y] = move(command.x, command.y);
        return { ...command, x1, y1, x2, y2, x, y };
      }
    }
  });
}

const slices = [
  { share: 0.45, color: '#3366CC' },
  { share: 0.3, color: '#EF402B' },
  { share: 0.25, color: '#FFD84D' },
];

export default function Root() {
  const frame = useCurrentFrame();
  const sweep = interpolate(frame, [0, 60], [0, 360], { extrapolateRight: 'clamp' });
  const spin = frame * 2;
  const beat = 1 + 0.12 * Math.max(0, Math.sin((frame / 30) * Math.PI * 2)) ** 4;
  const star = starPath({ radius: 45, points: 5, innerRatio: 0.45, cornerRadius: 4 });
  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={90}>
      <Rect width={1280} height={720} fill="#101820" />

      {/* Triangles: isosceles, right-angled, rounded with a gradient, and turning about its centre. */}
      <Triangle x={60} y={60} width={140} height={120} fill="#4DD8C0" />
      <Triangle x={240} y={60} width={140} height={120} apex={0} stroke="#FFFFFF" strokeWidth={4} join="round" />
      <Triangle x={420} y={60} width={140} height={120} cornerRadius={16}
        fill={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: 120 },
          stops: [{ offset: 0, color: '#FFD84D' }, { offset: 1, color: '#EF402B' }] }} />
      <Triangle x={670} y={120} anchorX={0.5} anchorY={0.5} width={120} height={104} rotation={spin}
        cornerRadius={10} fill="#8E6CFF" />

      {/* Polygons and stars, sized by radius like a circle. */}
      <Polygon x={60} y={230} radius={70} sides={6} fill="#3366CC" />
      <Polygon x={220} y={230} radius={70} sides={5} cornerRadius={14} stroke="#4DD8C0" strokeWidth={6} />
      <Star x={380} y={230} radius={70} points={5} fill="#FFD84D" />
      <Star x={540} y={230} radius={70} points={8} innerRatio={0.7} cornerRadius={6} fill="#EF402B"
        stroke="#FFFFFF" strokeWidth={3} join="round" />
      <Star x={770} y={300} anchorX={0.5} anchorY={0.5} radius={70} points={6} innerRatio={0.4} rotation={-spin}
        fill="#FFFFFF" glow={{ color: '#FFD84D', blur: 18 }} />

      {/* Pies: a chart from consecutive angles, a slice sweeping open, rounded, and counter-clockwise. */}
      {slices.map(({ share, color }, i) => {
        const start = slices.slice(0, i).reduce((sum, slice) => sum + slice.share * 360, 0);
        return <Pie key={color} x={60} y={420} radius={110} startAngle={start} endAngle={start + share * 360}
          fill={color} stroke="#101820" strokeWidth={4} join="round" />;
      })}
      <Pie x={290} y={420} radius={110} endAngle={sweep} fill="#4DD8C0" />
      <Pie x={520} y={420} radius={110} startAngle={-60} endAngle={60} cornerRadius={16} fill="#8E6CFF" />
      <Pie x={750} y={420} radius={110} endAngle={-sweep} cornerRadius={12} stroke="#FFD84D" strokeWidth={6} />

      {/* Hearts: beating about the centre, and outlined. */}
      <Heart x={1110} y={140} anchorX={0.5} anchorY={0.5} width={160} height={140} scale={beat} fill="#EF402B" />
      <Heart x={1040} y={260} width={140} height={124} stroke="#FFFFFF" strokeWidth={4} join="round" />

      {/* Path commands from a generator, drawn as is, sheared, and as a dial. */}
      <Path x={1010} y={420} commands={star} fill="#FFD84D" />
      <Path x={1150} y={420} commands={mapPoints(star, (x, y) => [x + (45 - y) * 0.5, y])} stroke="#FFD84D"
        strokeWidth={3} join="round" />
      <Path x={1060} y={540} commands={piePath({ radius: 60, startAngle: -120, endAngle: -120 + (sweep * 2) / 3 })}
        fill="#4DD8C0" />
    </Composition>
  );
}
