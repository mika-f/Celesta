import { Easings, Group, Rect, useCurrentFrame } from '@celesta/react';
import { Label } from '../components/Label';
import { C, DURATION, DW, DX } from '../constants';
import { clamp, progress } from '../helpers';

// ── 09 · Agents: this film, as a transcript ───────────────────────────────

type Line = { at: number; text: string; color?: string; bullet?: boolean; ja?: boolean };
export const PROMPT = 'Celesta の機能を紹介する、かっこいい動画を作って';
const TRANSCRIPT: Line[] = [
  { at: 4, text: PROMPT, ja: true },
  { at: 30, text: 'Read   skills/celesta/SKILL.md', bullet: true },
  { at: 40, text: 'Write  examples/feature-tour/film.tsx', bullet: true },
  { at: 50, text: 'Bash   node inspect.mjs film.tsx --every 15', bullet: true },
  { at: 58, text: `       composition: 1920×1080 @ 30 fps, ${DURATION} frames`, color: C.grey },
  { at: 64, text: '       OK: entry loads and every frame evaluates', color: C.sky },
  { at: 76, text: 'Bash   Celesta-export --react film.tsx', bullet: true },
  { at: 92, text: '       → feature-tour.mp4', color: C.sky },
];

export function AgentDemo() {
  const f = useCurrentFrame();
  const enter = progress(f, 2, 16, Easings.easeOutExpo);
  const x = DX;
  const y = 190;
  const lh = 50;
  const size = 19;
  const visible = TRANSCRIPT.filter((line) => f >= line.at);
  const last = visible[visible.length - 1];
  const spinner = '|/-\\'[Math.floor(f / 3) % 4];
  return (
    <Group y={30 * (1 - enter)} opacity={enter}>
      <Rect x={x} y={y} width={DW} height={680} cornerRadius={14} fill="#0B0D13" stroke={C.line} strokeWidth={1} />
      {[C.dim, C.dim, C.dim].map((color, i) => (
        <Rect key={i} x={x + 24 + i * 20} y={y + 24} width={11} height={11} cornerRadius={6} fill={color} />
      ))}
      <Label x={x + DW / 2} y={y + 30} size={15} font="mono" weight={400} color={C.grey} ax={0.5} ay={0.5}>
        agent — examples/feature-tour
      </Label>
      <Rect x={x} y={y + 58} width={DW} height={1} fill={C.line} />
      {visible.map((line, i) => {
        const ly = y + 110 + i * lh;
        const p = progress(f, line.at, 8, Easings.easeOutExpo);
        if (line.ja) {
          const typed = Math.floor(clamp((f - line.at) / 18) * line.text.length);
          return (
            <Group key={i}>
              <Rect x={x + 20} y={ly - 24} width={DW - 40} height={48} cornerRadius={8} fill={C.panel} />
              <Label x={x + 40} y={ly} size={size} font="mono" weight={700} color={C.blue} ay={0.5}>{'>'}</Label>
              <Label x={x + 70} y={ly} size={20} font="ja" weight={500} ay={0.5}>{line.text.slice(0, typed)}</Label>
            </Group>
          );
        }
        const pending = line === last && line.bullet && f < line.at + 10;
        return (
          <Group key={i} opacity={p} x={-12 * (1 - p)}>
            {line.bullet && (
              pending
                ? <Label x={x + 40} y={ly} size={size} font="mono" weight={700} color={C.sky} ay={0.5}>{spinner}</Label>
                : <Rect x={x + 40} y={ly - 5} width={10} height={10} cornerRadius={5} fill={C.blue} />
            )}
            <Label x={x + 70} y={ly} size={size} font="mono" weight={line.bullet ? 700 : 400}
              color={line.color ?? C.paper} ay={0.5}>{line.text}</Label>
          </Group>
        );
      })}
      {Math.floor(f / 8) % 2 === 0 && (
        <Rect x={x + 40} y={y + 110 + visible.length * lh - 14} width={11} height={24} fill={C.paper} />
      )}
    </Group>
  );
}
