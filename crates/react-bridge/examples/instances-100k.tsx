// 100,000 particles with every column, all moving every frame: the React
// side of the Instances benchmark (docs/performance/instances.md). Run with
// `cargo run --release -p celesta-react-bridge --example protocol-bench --
// crates/react-bridge/examples/instances-100k.tsx 120`.
import { Composition, Instances, useCurrentFrame } from '@celesta.studio/react';

const N = 100_000;
const COLORS = [
  [0xff, 0x3d, 0x7f],
  [0x3d, 0xa5, 0xff],
  [0x7b, 0x5c, 0xff],
  [0x00, 0xe0, 0xb8],
];
const xs = new Float32Array(N);
const ys = new Float32Array(N);
const widths = new Float32Array(N);
const heights = new Float32Array(N);
const rotations = new Float32Array(N);
const colors = new Uint8Array(N * 4);

function Field() {
  const t = useCurrentFrame() / 60;
  for (let i = 0; i < N; i++) {
    const a = i * 0.618 + t;
    xs[i] = 960 + Math.cos(a) * (i % 900);
    ys[i] = 540 + Math.sin(a) * (i % 500);
    widths[i] = 3 + (i % 3);
    heights[i] = 3 + (i % 2);
    rotations[i] = a * 57;
    const [r, g, b] = COLORS[i & 3];
    colors[i * 4] = r;
    colors[i * 4 + 1] = g;
    colors[i * 4 + 2] = b;
    colors[i * 4 + 3] = 204;
  }
  return (
    <Instances count={N} x={xs} y={ys} width={widths} height={heights} rotation={rotations} color={colors} />
  );
}

export default function Film() {
  return (
    <Composition width={1920} height={1080} fps={60} durationInFrames={600}>
      <Field />
    </Composition>
  );
}
