import { Group, Rect, Text, useVideoConfig } from '@celesta.studio/react';
import { W } from '../constants';
import { tapeAdvance } from '../measure';
import type { TapeKind } from '../tapes';
import { tapeRun, tapeStyle } from '../tapes';

// Half the band's visible length: half the frame, with room for the slant.
const REACH = 600;

export type TapeProps = {
  y: number;
  rotation: number;
  text: string;
  kind: TapeKind;
  bg: string;
  fg: string;
  // Pixels per frame; negative scrolls right.
  speed: number;
  frame: number;
};

// A slanted band of scrolling copy. Purely decorative, so it may run through
// the platform UI at the bottom of the frame. The text only moves, so its
// raster is drawn once and reused.
export function Tape({ y, rotation, text, kind, bg, fg, speed, frame }: TapeProps) {
  const { durationInFrames } = useVideoConfig();
  const style = tapeStyle(kind);
  const height = (style.fontSize ?? 54) * 1.7;
  // Just enough repeats to cover the band for the whole scene: the text starts
  // at the band's left end (or far enough left to travel right) and still
  // reaches its right end on the last frame. A longer run would only grow the
  // text's raster towards the GPU's texture size limit, past which it is
  // drawn shrunk and blurred.
  const travel = Math.abs(speed) * durationInFrames;
  const start = -REACH - Math.max(0, -speed) * durationInFrames;
  const count = Math.ceil((2 * REACH + travel) / tapeAdvance(kind, text)) + 1;
  return (
    <Group x={W / 2} y={y} rotation={rotation}>
      <Rect x={-W} y={-height / 2} width={W * 2} height={height} fill={bg} />
      <Text x={start - frame * speed} anchorY={0.5} style={{ ...style, fill: { type: 'solid', color: fg } }}>
        {tapeRun(kind, text).repeat(count)}
      </Text>
    </Group>
  );
}
