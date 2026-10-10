import { Easings, Group, Rect, interpolate, spring, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { Arrow } from '@celesta/shapes';
import { Label } from '../components/Label';
import { BODY, Panel } from '../components/Panel';
import { CAST } from '../character';
import { C } from '../constants';
import { lineFrom, plan } from '../voice';

const LEGEND = 'バー1本＝台本の1行（音声の長さ）';
export const TEXT = ['声に合わせる', '台本', '音声', '映像', LEGEND];

const STEPS = [
  { label: 'script.ts', note: '台本' },
  { label: 'voices/', note: '音声' },
  { label: 'film.tsx', note: '映像' },
];

// "Edit the script, re-make the voices, and the picture follows": the
// pipeline, then this video's own timeline, every bar a line of the script
// at the length planDialogue() measured from its WAV. On the answer the bars
// spring from equal widths to their real lengths.
export function SyncScene() {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const absolute = frame + plan.scene('sync').from;
  const answer = lineFrom('sync', 'sync-2');
  const real = spring({ frame: frame - answer - 6, fps, config: { damping: 14, stiffness: 120 } });
  const total = plan.durationInFrames;
  const even = BODY.w / plan.lines.length;
  const trackY = BODY.y + 270;
  const barH = 70;
  let cursor = BODY.x;
  const bars = plan.lines.map((l) => {
    const width = even + ((l.spanInFrames / total) * BODY.w - even) * real;
    const bar = { x: cursor, width, line: l };
    cursor += width;
    return bar;
  });
  const playhead = BODY.x + bars.reduce((x, b) => {
    if (absolute >= b.line.from + b.line.spanInFrames) return x + b.width;
    if (absolute < b.line.from) return x;
    return x + b.width * ((absolute - b.line.from) / b.line.spanInFrames);
  }, 0);
  return (
    <Panel index="05" title="声に合わせる" accent={C.yellow}>
      {STEPS.map((step, i) => {
        const x = BODY.x + i * 300;
        const pop = spring({ frame: frame - 4 - i * 5, fps, config: { damping: 12 } });
        return (
          <Group key={step.label} opacity={Math.min(1, pop * 1.5)} y={30 * (1 - pop)}>
            <Rect x={x} y={BODY.y} width={250} height={150} cornerRadius={28} fill={C.paper} stroke={C.ink} strokeWidth={5} />
            <Label x={x + 125} y={BODY.y + 52} ax={0.5} ay={0.5} size={50} weight={900}>{step.note}</Label>
            <Label x={x + 125} y={BODY.y + 112} ax={0.5} ay={0.5} size={26} font="mono" weight={700} color={C.soft}>{step.label}</Label>
            {i < STEPS.length - 1 && (
              <Arrow x1={x + 256} y1={BODY.y + 75} x2={x + 294} y2={BODY.y + 75} stroke={C.ink} strokeWidth={8} />
            )}
          </Group>
        );
      })}
      <Label x={BODY.x} y={trackY - 40} ay={0.5} size={30} font="mono" weight={700} color={C.soft}>planDialogue()</Label>
      <Label x={BODY.x + BODY.w} y={trackY - 40} ax={1} ay={0.5} size={34} font="mono" weight={800}>
        {`${(total / fps).toFixed(1)} s`}
      </Label>
      {bars.map(({ x, width, line }) => (
        <Rect key={line.id} x={x + 2} y={trackY} width={Math.max(2, width - 4)} height={barH} cornerRadius={12}
          fill={CAST[line.line.speaker].color} opacity={line.line.scene === 'sync' ? 1 : 0.55} />
      ))}
      <Rect x={playhead} y={trackY - 14} anchorX={0.5} width={8} height={barH + 28} cornerRadius={4} fill={C.ink} />
      <Group opacity={interpolate(frame, [answer + 10, answer + 20], [0, 1], { easing: Easings.easeOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' })}>
        {(['kiritan', 'zunko'] as const).map((id, i) => (
          <Group key={id} x={BODY.x + i * 330} y={trackY + barH + 60}>
            <Rect width={36} height={36} cornerRadius={10} fill={CAST[id].color} />
            <Label x={52} y={18} ay={0.5} size={34} weight={800}>{CAST[id].displayName}</Label>
          </Group>
        ))}
      </Group>
      <Label x={BODY.x} y={BODY.y + BODY.h - 30} ay={0.5} size={36} weight={800} color={C.soft}
        opacity={interpolate(frame, [answer + 16, answer + 26], [0, 1], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' })}>
        {LEGEND}
      </Label>
    </Panel>
  );
}
