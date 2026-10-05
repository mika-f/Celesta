import {
  Composition, Font, Group, Rect, Text, registerComponent,
  useCurrentFrame, useTextMetrics,
} from '@celesta/react';
import type { TextStyle } from '@celesta/react';

const style: TextStyle = { fontFamily: 'IBM Plex Mono', fontSize: 40 };

function Heading() {
  const first = '速い、';
  const accent = 'Easy';
  const last = '、頼もしい。';
  const a = useTextMetrics(first, style);
  const b = useTextMetrics(accent, style);
  return <Group x={64} y={100}>
    <Text anchorY="baseline" style={style}>{first}</Text>
    <Text x={a.width} anchorY="baseline"
      style={{ ...style, fill: { type: 'solid', color: '#28A34A' } }}>{accent}</Text>
    <Text x={a.width + b.width} anchorY="baseline" style={style}>{last}</Text>
  </Group>;
}

export function MeasuredPill({ label }: { label?: string }) {
  const frame = useCurrentFrame();
  const text = label ?? `最新 ${frame}`;
  const m = useTextMetrics(text, style);
  const width = m.width + 48;
  const height = m.height + 24;
  return <Group>
    <Rect id="pill-background" width={width} height={height}
      cornerRadius={height / 2} fill="#28A34A" />
    <Text id="pill-label" x={24} y={12 + m.ascent} anchorY="baseline"
      style={{ ...style, fill: { type: 'solid', color: '#ffffff' } }}>{text}</Text>
  </Group>;
}

function ChipRow() {
  const frame = useCurrentFrame();
  const first = `SFP+ 10GbE ×${frame + 1}`;
  const second = 'RJ45 2.5GbE ×4';
  const third = 'USB-C';
  const a = useTextMetrics(first, style);
  const b = useTextMetrics(second, style);
  const c = useTextMetrics(third, style);
  const gap = 16;
  const widths = [a.width + 48, b.width + 48, c.width + 48];
  const total = widths.reduce((sum, width) => sum + width, 0) + 2 * gap;
  return <Group x={(1600 - total) / 2} y={300}>
    <MeasuredPill label={first} />
    <Group x={widths[0] + gap}><MeasuredPill label={second} /></Group>
    <Group x={widths[0] + widths[1] + 2 * gap}><MeasuredPill label={third} /></Group>
  </Group>;
}

registerComponent('MeasuredPill', MeasuredPill);

export default function Root() {
  return <Composition width={1600} height={600} fps={30} durationInFrames={120} lang="ja-JP">
    <Font src="../../../examples/prism/assets/fonts/IBMPlexMono-Regular.ttf" />
    <Heading />
    <Group x={64} y={160}><MeasuredPill /></Group>
    <ChipRow />
  </Composition>;
}
