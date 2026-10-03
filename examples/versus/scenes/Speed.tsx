import { Easings, Group, Rect, spring, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { Footnote, Header, ToolTag } from '../components/Chrome';
import { Label } from '../components/Label';
import { C, ORDER, TOOL } from '../constants';
import { EXPORT, FRAMES, MACHINE, TUNED } from '../data';
import { clamp, progress } from '../helpers';

const START = 36;
// Race time runs this many times faster than the measured exports.
const SPEEDUP = 6;
const TRACK_X = 470;
const TRACK_W = 1180;

// The three exports as a race, replayed from their measured times.
export function Speed() {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const elapsed = Math.max(0, ((f - START) / fps) * SPEEDUP);
  const slowest = Math.max(...ORDER.map((id) => EXPORT[id]));
  const raceEnd = START + (slowest / SPEEDUP) * fps;
  const verdict = spring({ frame: f - raceEnd - 6, fps, config: { damping: 14, stiffness: 160 } });
  const vsRemotion = EXPORT.remotion / EXPORT.celesta;
  return (
    <>
      <Header f={f} n="05 — SPEED" title="600 frames to MP4. Go." sub="Wall-clock time from launching the CLI to the finished file. Replayed at 6× speed." />
      <Group x={1824} y={110} opacity={progress(f, 20, 14)}>
        <Label ax={1} size={20} font="mono" weight={700} color={C.grey} spacing={3}>ELAPSED</Label>
        <Label y={34} ax={1} size={64} font="mono" weight={700}>{`${Math.min(elapsed, slowest).toFixed(1)}s`}</Label>
      </Group>
      {ORDER.map((id, i) => {
        const t = EXPORT[id];
        const done = elapsed >= t;
        const p = clamp(elapsed / t);
        const frame = Math.floor(p * FRAMES);
        const y = 360 + i * 150;
        const appear = progress(f, 10 + i * 6, 18, Easings.easeOutExpo);
        const finishedAt = START + (t / SPEEDUP) * fps;
        const stamp = spring({ frame: f - finishedAt, fps, config: { damping: 12, stiffness: 200 } });
        const color = TOOL[id].color;
        return (
          <Group key={id} y={y} opacity={appear}>
            <ToolTag id={id} x={96} y={-8} size={36} />
            <Rect x={TRACK_X} y={0} width={TRACK_W} height={44} cornerRadius={22} fill="#FFFFFF0A" />
            {Array.from({ length: 11 }, (_, k) => (
              <Rect key={k} x={TRACK_X + (TRACK_W * k) / 10} y={50} width={1} height={8} fill="#FFFFFF20" />
            ))}
            <Rect x={TRACK_X} y={0} width={Math.max(44, TRACK_W * p)} height={44} cornerRadius={22} fill={{
              type: 'linear', start: { x: 0, y: 0 }, end: { x: Math.max(44, TRACK_W * p), y: 0 },
              stops: [{ offset: 0, color: `${color}30` }, { offset: 1, color }],
            }} />
            <Rect x={TRACK_X + Math.max(44, TRACK_W * p) - 22} y={22} anchorX={0.5} anchorY={0.5} width={30} height={30}
              cornerRadius={15} fill="#FFFFFF" glow={{ color, blur: 14 }} opacity={elapsed > 0 ? 1 : 0.4} />
            <Label x={TRACK_X + 4} y={62} size={16} font="mono" weight={400} color={C.grey}>
              {`${String(frame).padStart(3, '0')} / ${FRAMES} frames rendered`}
            </Label>
            <Group x={TRACK_X + TRACK_W + 40} y={-4}>
              <Label size={44} font="mono" weight={700} color={done ? color : C.soft}>
                {`${Math.min(elapsed, t).toFixed(1)}s`}
              </Label>
              <Group y={58} opacity={Math.min(1, Math.max(0, stamp))} scale={0.8 + 0.2 * stamp}>
                <Label size={16} font="mono" weight={700} color={color} spacing={2}>
                  {`DONE · ${(FRAMES / t).toFixed(1)} FPS`}
                </Label>
              </Group>
            </Group>
          </Group>
        );
      })}
      <Group x={96} y={810} opacity={Math.min(1, Math.max(0, verdict))}>
        <Group scale={0.85 + 0.15 * verdict}>
          <Label size={120} weight={700} spacing={-4} color={TOOL.celesta.color}>{`${vsRemotion.toFixed(1)}×`}</Label>
        </Group>
        <Label x={250} y={22} size={34} weight={700}>faster than Remotion</Label>
        <Label x={250} y={70} size={21} weight={400} color={C.soft}>
          {`fframes is the raw-speed winner.${TUNED ? ` Tuning Remotion (--gl=angle, 100% concurrency) gave ${TUNED.min.toFixed(0)}–${TUNED.max.toFixed(0)} s: no faster.` : ''}`}
        </Label>
      </Group>
      <Footnote opacity={progress(f, 40, 20)}>{`${MACHINE} · libx264 medium, CRF 18 · median of 3`}</Footnote>
    </>
  );
}
