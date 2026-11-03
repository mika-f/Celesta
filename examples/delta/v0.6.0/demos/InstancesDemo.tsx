import { Easings, Group, Instances, Rect, Text, progress } from '@celesta.studio/react';
import { random } from '@celesta.studio/math';
import { useCountUp } from '@celesta.studio/text';
import { trianglePoints } from '../../components/Delta';
import { enter, STAGE } from '../../components/ChapterShell';
import type { DemoProps } from '../../components/ChapterShell';
import { C, FONT, solid } from '../../components/theme';

// 30,000 particles in one <Instances> layer: a swirling disc that folds into
// the Δ's outline. The columns are typed arrays filled in place every frame.
const N = 30_000;
const CX = STAGE.width / 2;
const CY = 400;
const radius = new Float32Array(N);
const angle = new Float32Array(N);
const tx = new Float32Array(N);
const ty = new Float32Array(N);
const lag = new Float32Array(N);
const xs = new Float32Array(N);
const ys = new Float32Array(N);
const colors = new Uint8Array(N * 4);

const corners = trianglePoints(560);
for (let i = 0; i < N; i++) {
  radius[i] = 30 + Math.sqrt(random(`inst-r-${i}`)) * 330;
  angle[i] = random(`inst-a-${i}`) * Math.PI * 2;
  // A point on the Δ's outline, pushed a little inward or outward.
  const s = random(`inst-s-${i}`) * 3;
  const side = Math.floor(s);
  const [ax, ay] = corners[side];
  const [bx, by] = corners[(side + 1) % 3];
  const along = s - side;
  const spread = (random(`inst-j-${i}`) - 0.5) * 26;
  tx[i] = CX + ax + (bx - ax) * along + spread;
  ty[i] = CY + 20 + ay + (by - ay) * along + spread;
  lag[i] = random(`inst-l-${i}`) * 20;
  const warm = 1 - (radius[i] - 30) / 330;
  colors.set([
    Math.round(242 + 13 * warm), Math.round(106 + 130 * warm), Math.round(46 + 180 * warm),
    Math.round(130 + 110 * random(`inst-c-${i}`)),
  ], i * 4);
}

export function InstancesDemo({ frame }: DemoProps) {
  for (let i = 0; i < N; i++) {
    const r = radius[i];
    const a = angle[i] + frame * (0.006 + 2.4 / r);
    const sx = CX + Math.cos(a) * r;
    const sy = CY + Math.sin(a) * r * 0.82;
    const p = progress(frame, 36 + lag[i], 34, Easings.easeInOutCubic);
    xs[i] = sx + (tx[i] - sx) * p;
    ys[i] = sy + (ty[i] - sy) * p;
  }
  const count = useCountUp(N, { delay: 0, durationInFrames: 34 });
  const label = enter(frame, 0);
  const foot = enter(frame, 40);
  return (
    <Group>
      <Rect width={STAGE.width} height={STAGE.height} fill={C.ink} />
      <Instances count={N} x={xs} y={ys} color={colors} shape="circle" width={3} height={3} opacity={enter(frame, -6, 14)} />
      <Group x={32} y={30} opacity={label}>
        <Text y={0} style={{ fontFamily: FONT.display, fontSize: 96, fill: solid(C.paper) }}>
          {Math.round(count).toLocaleString('en-US')}
        </Text>
        <Text y={102} style={{ fontFamily: FONT.mono, fontSize: 20, fill: solid(C.grey) }}>particles  ·  1 layer</Text>
      </Group>
      <Group x={32} y={STAGE.height - 50} opacity={foot}>
        <Text anchorY={0.5} style={{ fontFamily: FONT.mono, fontSize: 20, fill: solid(C.grey) }}>
          {'100k particles:  ≈180 ms as <Rect>s  →  ≈10 ms as <Instances>'}
        </Text>
      </Group>
    </Group>
  );
}
