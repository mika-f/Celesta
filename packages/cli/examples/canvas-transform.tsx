import { Composition, Group, Rect, Text, interpolate, useCurrentFrame } from '@celesta.studio/react';
import { Stack } from '@celesta.studio/layout';

// A scene for dragging layers on the canvas in the editor's Select layers
// mode. Each element shows a different way a transform can be written, so
// each one is editable, or locked with a reason:
//
//   - the blue Rect writes x, y, and rotation as literals: all editable;
//   - the green Rect has no x: dragging it writes `x` into the tag;
//   - the Text's x is keyframed: editable on frames 0, 30, and 60, read-only
//     between them;
//   - the Stack's children are editable: their x and y are offsets from
//     the slot the Stack gives each (the Stack itself has no Select);
//   - the three Rects from `.map()` are one element rendered three times:
//     locked as repeated;
//   - the orange Rect's x is an expression: locked;
//   - the Group has a literal x, and its two children move with it. Select a
//     child, then use Select next to the <Group> row of the Layer section to
//     pick the Group itself.
const W = 1280;

export default function Root() {
  const frame = useCurrentFrame();

  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={90}>
      <Rect width={1280} height={720} fill="#14181F" />

      <Rect x={80} y={60} rotation={8} width={200} height={110} fill="#3366CC" />

      <Rect y={220} width={200} height={110} fill="#2FA866" />

      <Text
        x={interpolate(frame, [0, 30, 60], [100, 600, 300])}
        y={400}
        style={{ fontSize: 48, fontWeight: 700, fill: { type: 'solid', color: '#FFFFFF' } }}
      >
        Keyframed
      </Text>

      <Stack x={940} y={60} spacing={90}>
        <Rect width={160} height={60} fill="#9B59B6" />
        <Rect width={160} height={60} fill="#C0508F" />
      </Stack>

      {[0, 1, 2].map((i) => (
        <Rect key={i} x={80 + i * 120} y={560} width={96} height={96} fill="#E0B341" />
      ))}

      <Rect x={W / 2} y={220} width={160} height={110} fill="#EF7B45" />

      <Group x={800} y={300}>
        <Rect width={140} height={90} fill="#4FB6C9" />
        <Rect x={160} width={140} height={90} fill="#2D7F90" />
      </Group>
    </Composition>
  );
}
