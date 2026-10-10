import { Group, Rect, Text } from '@celesta/react';
import { FONT, W } from '../constants';

export type TapeProps = {
  y: number;
  rotation: number;
  text: string;
  bg: string;
  fg: string;
  // Pixels per frame; negative scrolls right.
  speed: number;
  frame: number;
  size?: number;
  mono?: boolean;
};

// A slanted band of scrolling copy. Purely decorative, so it may run through
// the platform UI at the bottom of the frame. The text only moves, so its
// raster is drawn once and reused.
export function Tape({ y, rotation, text, bg, fg, speed, frame, size = 54, mono = false }: TapeProps) {
  const height = size * 1.7;
  // Long enough to cover the band for a whole scene at either direction.
  const line = Array.from({ length: 10 }, () => text).join('  ●  ');
  return (
    <Group x={W / 2} y={y} rotation={rotation}>
      <Rect x={-W} y={-height / 2} width={W * 2} height={height} fill={bg} />
      <Text x={-W - 1200 - frame * speed} anchorY={0.5}
        style={{ fontFamily: mono ? FONT.mono : FONT.ja, fontSize: size, fontWeight: mono ? 700 : 900,
          fill: { type: 'solid', color: fg } }}>
        {line}
      </Text>
    </Group>
  );
}
