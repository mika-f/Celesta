import { Composition, useCurrentFrame } from '@celesta/react';
import { Code } from '@celesta/code';
const source = process.env.CELESTA_CODE_BENCH_MULTILINE
  ? Array(10).fill('const n = 1; '.repeat(10)).join('\n')
  : 'const n = 1; '.repeat(100);
export default function Root() {
  const frame = useCurrentFrame();
  return <Composition width={1920} height={1080} fps={30} durationInFrames={240}>
    <Code language="ts" x={20} y={20} style={{ fontFamily: 'Menlo', fontSize: 16, lineHeight: 24 }}
      visibleCharacters={frame === 0 || frame >= 120 ? Infinity : Math.floor(Array.from(source).length * frame / 120)}
      highlightLines={[frame % 2 ? 2 : 1]}>{source}</Code>
  </Composition>;
}
