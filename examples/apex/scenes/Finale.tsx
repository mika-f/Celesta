import { Group, Rect, TextReveal, progress, useBeat, useCurrentFrame } from '@celesta/react';
import { Label } from '../components/Label';
import { Ring } from '../components/Ring';
import { ACID, BONE, CYAN, GREY, H, INK, MAG, W } from '../constants';

export function Finale() {
  const f = useCurrentFrame();
  const { pulse } = useBeat({ bpm: 120 });
  const out = progress(f, 150, 30);
  return <>
    <Rect width={W} height={H} fill={INK} />
    <Group opacity={1 - out}>
      {[0, 1, 2, 3, 4, 5].map((i) => {
        const age = (((f - i * 15) % 90) + 90) % 90 / 90;
        return <Ring key={i} r={120 + age * 900} color={[ACID, CYAN, MAG][i % 3]} width={3}
          opacity={(1 - age) * 0.7 * progress(f, i * 6, 10)} />;
      })}
      <Group x={W / 2} y={H / 2} scale={1 + pulse * 0.03}>
        <TextReveal x={-440} y={-312} lineHeight={560} baseline={0.82} stagger={0} durationInFrames={26}
          style={{ fontFamily: 'Bebas Neue', fontSize: 600, fill: { type: 'solid', color: BONE } }}>
          {'APEX'}
        </TextReveal>
      </Group>
      <Label x={W / 2} y={H - 170} size={30} mono color={BONE} anchorX={0.5} letterSpacing={10}
        opacity={progress(f, 40, 20)}>{'MADE WITH CELESTA'}</Label>
      <Label x={W / 2} y={H - 120} size={22} mono color={GREY} anchorX={0.5} opacity={progress(f, 56, 20)}>
        {'REACT · RUST · FFMPEG — EVERY FRAME RENDERED FROM CODE'}
      </Label>
    </Group>
  </>;
}
