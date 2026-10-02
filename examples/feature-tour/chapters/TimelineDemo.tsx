import type { ReactNode } from 'react';
import { Easings, Group, Rect, spring, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { Label } from '../components/Label';
import { BEAT, C, DW, DX } from '../constants';
import { clamp, hash, progress } from '../helpers';

// ── 05 · Timeline: a JSON project, and components on it ───────────────────

type TClip = { at: number; len: number; label: string; component?: boolean };
const T_TRACKS: { id: string; kind: string; tag: string; wave?: boolean; clips: TClip[] }[] = [
  { id: 'footage', kind: 'video', tag: C.paper, clips: [
    { at: 0, len: 300, label: 'gameplay.mp4' }, { at: 310, len: 370, label: 'boss_fight.mp4' }] },
  { id: 'voice', kind: 'audio', tag: C.grey, wave: true, clips: [
    { at: 40, len: 200, label: '001.wav' }, { at: 330, len: 250, label: '002.wav' }] },
  { id: 'titles', kind: 'overlay', tag: C.pink, clips: [
    { at: 20, len: 230, label: '<LowerThird />', component: true }, { at: 420, len: 220, label: 'Stage 1' }] },
  { id: 'lines', kind: 'dialogue', tag: C.blue, clips: [
    { at: 60, len: 250, label: 'akane · line 01' }, { at: 360, len: 290, label: 'yukari · line 02' }] },
];
const trackDrop = (i: number) => 10 + i * BEAT;

export function TimelineDemo() {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const enter = progress(f, 2, 16, Easings.easeOutExpo);
  // The JSON.
  const jx = DX;
  const jy = 200;
  const fs = 17;
  const lh = 27;
  const json = [
    '{',
    '  "version": 0,',
    '  "tracks": [',
    ...T_TRACKS.map((t, i) => {
      const id = `"${t.id}",`.padEnd(11);
      const kind = `"${t.kind}",`.padEnd(12);
      return `    { "id": ${id} "kind": ${kind} "items": [ … ] }${i < T_TRACKS.length - 1 ? ',' : ''}`;
    }),
    '  ]',
    '}',
  ];
  const active = T_TRACKS.reduce((found, _, i) => (f >= trackDrop(i) ? i : found), -1);

  // The timeline.
  const top = 570;
  const row = 76;
  const left = DX + 120;
  const span = DX + DW - left;
  const k = span / 680;
  const head = left + span * progress(f, 30, 86, Easings.linear);

  return (
    <Group y={30 * (1 - enter)} opacity={enter}>
      <Rect x={jx} y={jy} width={DW} height={json.length * lh + 36} cornerRadius={14} fill={C.panel} stroke={C.line} strokeWidth={1} />
      {active >= 0 && (
        <Rect x={jx + 12} y={jy + 18 + (3 + active) * lh} width={DW - 24} height={lh} cornerRadius={4}
          fill={C.blue} opacity={0.35 * (1 - progress(f, trackDrop(active) + 4, 12))} />
      )}
      {json.map((line, i) => (
        <Label key={i} x={jx + 24} y={jy + 18 + i * lh + lh / 2 + fs * 0.34} size={fs} font="mono" weight={400}
          ay="baseline" color={i >= 3 && i < 3 + T_TRACKS.length && i - 3 <= active ? C.paper : C.grey}>{line}</Label>
      ))}
      {T_TRACKS.map((track, ti) => {
        const y = top + ti * row;
        return (
          <Group key={track.id}>
            <Rect x={DX} y={y + 22} width={12} height={12} fill={track.tag} />
            <Label x={DX + 24} y={y + 28} size={16} font="mono" weight={700} ay={0.5}>{track.id}</Label>
            <Label x={DX + 24} y={y + 50} size={13} font="mono" weight={400} color={C.grey} ay={0.5}>{track.kind}</Label>
            {track.clips.map((clip, ci) => {
              const s = spring({ frame: f - trackDrop(ti) - ci * 5, fps, config: { damping: 13, stiffness: 160 } });
              const x = left + clip.at * k;
              const w = clip.len * k;
              const live = head >= x && head <= x + w;
              const bars: ReactNode[] = [];
              if (track.wave) {
                for (let b = 0; b * 5 < w - 24; b++) {
                  const amp = Math.abs(Math.sin(b * 0.41 + ci) * Math.sin(b * 0.09 + ci * 2)) * 0.85 + 0.1 * hash(b, ci);
                  bars.push(<Rect key={b} x={x + 14 + b * 5} y={y + 44} anchorY={0.5} width={2} height={3 + 22 * amp}
                    fill={live ? C.sky : C.grey} opacity={0.7} />);
                }
              }
              return (
                <Group key={ci} y={-50 * (1 - clamp(s))} opacity={clamp(s * 2)}>
                  <Rect x={x} y={y} width={w} height={row - 14} cornerRadius={6}
                    fill={live ? '#1F2436' : C.clip} stroke={live ? C.blue : clip.component ? C.pink : C.line}
                    strokeWidth={live || clip.component ? 2 : 1} />
                  <Rect x={x} y={y} width={5} height={row - 14} fill={track.tag} />
                  {bars}
                  <Label x={x + 16} y={y + (track.wave ? 18 : 31)} size={15} font="mono" weight={clip.component ? 700 : 400}
                    color={clip.component ? C.pink : live ? C.paper : C.soft} ay={0.5}>{clip.label}</Label>
                </Group>
              );
            })}
          </Group>
        );
      })}
      <Rect x={head - 1} y={top - 16} width={2} height={row * T_TRACKS.length + 10} fill={C.blue} opacity={progress(f, 26, 6)} />
      <Rect x={head} y={top - 16} anchorX={0.5} anchorY={0.5} width={14} height={14} cornerRadius={7}
        fill={C.blue} opacity={progress(f, 26, 6)} />
    </Group>
  );
}
