import { Group, Rect, cueAt, frameToTimecode, useBeat, useCurrentFrame } from '@celesta/react';
import { Label } from './Label';
import { BPM, C, FPS, H, W } from '../constants';
import { DAYS } from '../data';
import { SCENES, SCENE_CUES, TIMING } from '../timeline';

export function Hud() {
  const f = useCurrentFrame();
  const { beatInBar, pulse } = useBeat({ bpm: BPM });
  const scene = cueAt(SCENE_CUES, f)?.cue;
  // Only over the scenes with a light frame around them.
  if (!scene || scene.index === 0 || scene.index === SCENES.length - 1) return null;
  const m = 56;
  return (
    <>
      <Group blendMode="difference" opacity={0.8}>
        <Label x={m} y={m} size={18} font="mono" weight={700} ay={0.5}>CELESTA / 36 DAYS</Label>
        <Label x={W - m} y={m} size={18} font="mono" ax={1} ay={0.5}>{frameToTimecode(f, FPS)}</Label>
        <Label x={m} y={H - m} size={18} font="mono" ay={0.5}>{`0${scene.index + 1}  ${scene.name}`}</Label>
        <Label x={W - m - 120} y={H - m} size={18} font="mono" ax={1} ay={0.5}>{`${BPM} BPM`}</Label>
        {[0, 1, 2, 3].map((b) => (
          <Rect key={b} x={W - m - 96 + b * 26} y={H - m - 8} width={16} height={16}
            fill={C.paper} opacity={b === beatInBar ? 0.4 + 0.6 * pulse : 0.2} />
        ))}
      </Group>
      <Rect x={m} y={H - 24} width={(W - m * 2) * (f / (TIMING.durationInFrames - 1))} height={2} fill={C.mint} opacity={0.8} />
    </>
  );
}
