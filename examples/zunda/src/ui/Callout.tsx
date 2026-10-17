// いま足したもの（`<Rect />` など）の名前が、画面の真ん中にぽんと出て消える。

import { Group, Text, progress, spring } from '@celesta/react';

import { COLOR, FONT, FPS, solid } from '../theme.ts';

const SHOW_FRAMES = 70;

export function Callout({ frame, at, label }: { frame: number; at: number; label: string }) {
  const local = frame - at;
  if (local < 0 || local > SHOW_FRAMES) return null;
  const pop = spring({ frame: local, fps: FPS, config: { damping: 12 } });
  const fade = 1 - progress(local, SHOW_FRAMES - 18, 18);
  return (
    <Group x={960} y={430} scale={0.6 + 0.4 * pop} opacity={Math.min(pop, fade)}>
      <Text anchorX={0.5} anchorY={0.5}
        style={{ fontFamily: FONT.mono, fontSize: 64, fontWeight: 700, fill: solid(COLOR.white),
          stroke: { paint: solid('#00000088'), width: 4 } }}
        shadow={{ color: '#00000066', blur: 18, offsetX: 0, offsetY: 8 }}>
        {label}
      </Text>
    </Group>
  );
}
