import { Group, Rect, frameToTimecode, useCurrentFrame, useScene } from '@celesta.studio/react';
import { CHAPTERS } from '../chapters';
import { Label } from './Label';
import { C, FPS, H, W } from '../constants';
import { SCENES } from '../timeline';

// From the index to the outro: corner marks, the timecode, and one segment
// per chapter with the current one's name.
export function Hud() {
  const f = useCurrentFrame();
  const scene = useScene(SCENES);
  if (!scene || scene.id === 'open' || scene.id === 'outro') {
    return null;
  }
  const chapter = scene.scene.chapter ?? -1;
  const m = 56;
  const arm = 22;
  const corner = (x: number, y: number, sx: number, sy: number) => (
    <Group key={`${x}-${y}`} x={x} y={y}>
      <Rect x={sx < 0 ? -arm : 0} width={arm} height={2} fill={C.paper} />
      <Rect y={sy < 0 ? -arm : 0} width={2} height={arm} fill={C.paper} />
    </Group>
  );
  return (
    <Group opacity={0.7}>
      {corner(m, m, 1, 1)}
      {corner(W - m, m, -1, 1)}
      {corner(m, H - m, 1, -1)}
      {corner(W - m, H - m, -1, -1)}
      <Label x={m + 36} y={m + 10} size={16} font="mono" weight={700} ay={0.5}>CELESTA</Label>
      <Label x={m + 118} y={m + 10} size={16} font="mono" weight={400} ay={0.5} opacity={0.6}>/ FEATURE TOUR</Label>
      <Label x={W - m - 36} y={m + 10} size={16} font="mono" weight={400} ax={1} ay={0.5}>{frameToTimecode(f, FPS).slice(3)}</Label>
      {CHAPTERS.map((_, i) => (
        <Rect key={i} x={m + 36 + i * 30} y={H - m - 12} width={24} height={4}
          fill={i === chapter ? (CHAPTERS[i].bg === C.blue ? C.paper : C.blue) : i < chapter ? C.soft : C.dim} />
      ))}
      <Label x={m + 36 + CHAPTERS.length * 30 + 12} y={H - m - 10} size={16} font="mono" weight={400} ay={0.5}>
        {scene.name}
      </Label>
      <Label x={W - m - 36} y={H - m - 10} size={16} font="mono" weight={400} ax={1} ay={0.5}>
        {`${W}×${H} · ${FPS} FPS · 120 BPM`}
      </Label>
    </Group>
  );
}
