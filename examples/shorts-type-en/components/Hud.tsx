import { Group, Rect, cueAt, useBeat, useCurrentFrame } from '@celesta/react';
import { Copy } from './Copy';
import { BPM, C, COL, SAFE } from '../constants';
import { SCENE_CUES, SCENES } from '../timeline';

// Scene number and a four-beat metronome along the top of the safe area,
// above the copy (scenes start their copy at TOP). Drawn with
// `difference` so it reads on the dark, light and coloured scenes alike.
export function Hud() {
  const f = useCurrentFrame();
  const { beatInBar, pulse } = useBeat({ bpm: BPM });
  const scene = cueAt(SCENE_CUES, f)?.cue;
  if (!scene) return null;
  const x = SAFE.left + COL.x;
  const y = SAFE.top + 12;
  return (
    <Group blendMode="difference" opacity={0.85}>
      <Copy x={x} y={y} size={22} weight={700} font="mono" ay={0.5}>
        {`0${scene.index + 1}/0${SCENES.length}  ${scene.name}`}
      </Copy>
      {[0, 1, 2, 3].map((b) => (
        <Rect key={b} x={x + 330 + b * 24} y={y - 7} width={14} height={14} cornerRadius={7}
          fill={C.paper} opacity={b === beatInBar ? 0.4 + 0.6 * pulse : 0.2} />
      ))}
    </Group>
  );
}
