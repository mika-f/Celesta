import type { ReactNode } from 'react';
import { Easings, Group, Rect, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { Label } from '../components/Label';
import { Swap } from '../components/Swap';
import { Tag } from '../components/Tag';
import { BAR, C, H, W } from '../constants';
import { clamp, hash, progress, timecode } from '../helpers';

type Clip = { at: number; len: number; label: string };
const TRACKS: { id: string; name: string; tag: string; clips: Clip[]; wave?: boolean }[] = [
  {
    id: 'V1', name: 'VIDEO', tag: C.paper, clips: [
      { at: 0, len: 560, label: 'opening.mp4' },
      { at: 580, len: 760, label: 'gameplay_04.mp4' },
      { at: 1360, len: 900, label: 'boss_fight.mp4' },
    ]
  },
  {
    id: 'A1', name: 'VOICE', tag: C.grey, wave: true, clips: [
      { at: 90, len: 420, label: 'akane_001.wav' },
      { at: 640, len: 380, label: 'akane_002.wav' },
      { at: 1120, len: 460, label: 'yukari_003.wav' },
      { at: 1700, len: 520, label: 'akari_004.wav' },
    ]
  },
  {
    id: 'T1', name: 'TITLES', tag: C.accent, clips: [
      { at: 40, len: 380, label: '<Title />' },
      { at: 860, len: 420, label: '<LowerThird />' },
      { at: 1480, len: 480, label: '<Subtitle />' },
    ]
  },
  {
    id: 'FX', name: 'EFFECT', tag: C.dim, clips: [
      { at: 300, len: 520, label: 'fade' },
      { at: 1240, len: 300, label: 'slide' },
      { at: 1900, len: 560, label: 'scale' },
    ]
  },
];

export function Timeline() {
  const f = useCurrentFrame();
  const { durationInFrames } = useVideoConfig();
  const left = 400;
  const top = 560;
  const row = 92;
  const scroll = 200 + f * 6;
  const playhead = 1180;
  const exit = 1 - progress(f, durationInFrames - 16, 12, Easings.easeInCubic);

  const tracks = TRACKS.map((track, ti) => {
    const y = top + ti * row;
    const clips = track.clips.map((clip, ci) => {
      const x = left + clip.at - scroll;
      if (x > W || x + clip.len < left - 40) return null;
      const grow = progress(f, 4 + ti * 3 + ci * 2, 14, Easings.easeOutExpo);
      const width = Math.max(1, clip.len * grow);
      const live = playhead >= x && playhead <= x + width;
      const bars: ReactNode[] = [];
      if (track.wave) {
        for (let b = 0; b * 6 < width - 20; b++) {
          const bx = x + 14 + b * 6;
          if (bx < left - 10 || bx > W) continue;
          const amp = Math.abs(Math.sin(b * 0.37 + ci) * Math.sin(b * 0.071 + ci * 2)) * 0.8 + 0.1 * hash(b, ci);
          bars.push(<Rect key={b} x={bx} y={y + 52} anchorY={0.5} width={3} height={4 + 30 * amp}
            fill={live ? C.accent : C.grey} opacity={0.8} />);
        }
      }
      return (
        <Group key={ci} opacity={clamp(grow * 3)}>
          <Rect x={x} y={y} width={width} height={row - 16} cornerRadius={6}
            fill={live ? '#26262C' : C.clip} stroke={live ? C.accent : '#FFFFFF1A'} strokeWidth={live ? 2 : 1} />
          <Rect x={x} y={y} width={6} height={row - 16} fill={track.tag} />
          {bars}
          <Label x={x + 20} y={y + (track.wave ? 20 : 38)} size={19} font="mono" weight={400}
            color={live ? C.paper : C.grey} ay={0.5} opacity={grow}>{clip.label}</Label>
        </Group>
      );
    });
    return <Group key={track.id}>{clips}</Group>;
  });

  // Ruler ticks every 30 px of timeline, labeled every second (150 px).
  const ticks: ReactNode[] = [];
  const first = Math.floor(scroll / 30);
  for (let k = first; k * 30 - scroll < W - left; k++) {
    const x = left + k * 30 - scroll;
    if (x < left) continue;
    const major = k % 5 === 0;
    ticks.push(<Rect key={`t${k}`} x={x} y={major ? 508 : 520} width={1} height={major ? 24 : 12} fill={C.dim} />);
    if (major) {
      ticks.push(<Label key={`l${k}`} x={x + 6} y={500} size={16} font="mono" weight={400} color={C.grey} ay={0.5}>
        {timecode(k * 6).slice(3)}</Label>);
    }
  }

  const headUp = progress(f, 0, 14, Easings.easeOutExpo);

  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Group opacity={exit}>
        <Tag x={120} y={150} n="04" name="TIMELINE" opacity={headUp} />
        <Swap f={f} x={120} y={190} size={176} accentLast cues={[[0, 'Layer it.'], [BAR, 'Sequence it.']]} />
        {/* Clips scroll under the label column, so keep them inside the panel. */}
        <Group clip={{ x: left - 10, y: 480, width: W - left + 10, height: H - 480 }}>
          {ticks}
          {tracks}
        </Group>
        {TRACKS.map((track, ti) => (
          <Group key={track.id} x={120} y={top + ti * row + 38} opacity={progress(f, ti * 3, 12)}>
            <Rect y={-8} width={16} height={16} fill={track.tag} />
            <Label x={34} y={0} size={20} font="mono" weight={700} color={C.paper} ay={0.5}>{track.id}</Label>
            <Label x={90} y={0} size={20} font="mono" weight={400} color={C.grey} ay={0.5}>{track.name}</Label>
          </Group>
        ))}
        <Rect x={playhead - 1} y={488} width={3} height={row * TRACKS.length + 60} fill={C.accent} />
        <Rect x={playhead} y={470} anchorX={0.5} width={176} height={36} cornerRadius={4} fill={C.accent} />
        <Label x={playhead} y={488} size={18} font="mono" weight={700} color={C.ink} ax={0.5} ay={0.5}>
          {timecode(Math.round((scroll + playhead - left) / 5))}
        </Label>
      </Group>
    </>
  );
}
