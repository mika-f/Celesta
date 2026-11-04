import { Group, Rect, Text } from '@celesta.studio/react';
import { Circle, Path } from '@celesta.studio/shapes';
import { Delta } from '../../components/Delta';
import { C, FONT, solid } from '../../components/theme';

// A small poster to put effects and grades on: a dusk sky, a sun, two
// ridges, the Δ and the version. Drawn, so it is the same at every size.
export function Picture({ width, height, t = 0 }: { width: number; height: number; t?: number }) {
  const sun = { x: width * 0.68, y: height * (0.42 - 0.04 * Math.sin(t / 20)) };
  return (
    <Group clip={{ width, height }}>
      <Rect width={width} height={height} fill={{
        type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: height },
        stops: [{ offset: 0, color: '#2B1B5A' }, { offset: 0.55, color: '#E2557A' }, { offset: 1, color: '#FFB547' }],
      }} />
      <Circle x={sun.x} y={sun.y} anchorX={0.5} anchorY={0.5} radius={height * 0.16} fill="#FFE3A3"
        glow={{ color: '#FFD27088', blur: 40 }} />
      <Path points={[[0, height * 0.72], [width * 0.22, height * 0.52], [width * 0.45, height * 0.7], [width * 0.7, height * 0.5],
        [width, height * 0.68], [width, height], [0, height]]} closed fill="#3A1F4E" />
      <Path points={[[0, height * 0.86], [width * 0.3, height * 0.7], [width * 0.6, height * 0.84], [width * 0.85, height * 0.74],
        [width, height * 0.8], [width, height], [0, height]]} closed fill="#170F24" />
      <Delta x={width * 0.27} y={height * 0.4} size={height * 0.3} fill={C.orange} glow={{ color: '#F26A2EAA', blur: 24 }} />
      <Text x={width * 0.06} y={height * 0.93} anchorY={1}
        style={{ fontFamily: FONT.display, fontSize: height * 0.16, fill: solid(C.paper) }}>0.7.0</Text>
    </Group>
  );
}
