import { Group, Rect } from '@celesta/react';
import { H, SAFE, W } from '../constants';

// The areas a Shorts / TikTok player covers with its own UI. Off by default;
// turn it on with the `guides` project property to check a frame:
//   Celesta-export --react film.tsx --props '{"guides":true}' --frame 300 check.png
export function SafeGuide() {
  const fill = '#00C2FF55';
  return (
    <Group>
      <Rect width={W} height={SAFE.top} fill={fill} />
      <Rect y={H - SAFE.bottom} width={W} height={SAFE.bottom} fill={fill} />
      <Rect x={W - SAFE.right} y={SAFE.top} width={SAFE.right} height={H - SAFE.top - SAFE.bottom} fill={fill} />
      <Rect width={SAFE.left} y={SAFE.top} height={H - SAFE.top - SAFE.bottom} fill="#00C2FF22" />
    </Group>
  );
}
