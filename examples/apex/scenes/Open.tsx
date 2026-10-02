import { Easings, Rect, progress, useCountUp, useCurrentFrame, useTypewriter } from '@celesta/react';
import { Label } from '../components/Label';
import { ACID, BONE, GREY, H, INK, W } from '../constants';
import { mix } from '../math';

export function Open() {
  const f = useCurrentFrame();
  const tw = useTypewriter('SIGNAL ACQUIRED', { from: 20, framesPerChar: 2 });
  const n = useCountUp(4096, { delay: 8, durationInFrames: 90 });
  const w = 1700 * progress(f, 6, 40, Easings.easeOutExpo);
  const flood = progress(f, 96, 24, Easings.easeInExpo);
  return <>
    <Rect width={W} height={H} fill={INK} />
    <Rect x={W / 2} y={H / 2} anchorX={0.5} anchorY={0.5} width={w}
      height={mix(2, H, flood)} fill={flood > 0 ? BONE : ACID} />
    <Label x={160} y={470} size={34} mono color={BONE} anchorY={1} opacity={1 - flood}>
      {tw.text + (tw.caretVisible ? '_' : ' ')}
    </Label>
    <Label x={160} y={620} size={26} mono color={GREY} opacity={progress(f, 14, 12) * (1 - flood)}>
      {`FRAME ${String(f).padStart(4, '0')} / ${String(n).padStart(4, '0')} NODES`}
    </Label>
    <Label x={W - 160} y={620} size={26} mono color={GREY} anchorX={1} opacity={progress(f, 30, 12) * (1 - flood)}>
      {'48 kHz · 1920×1080 · 30 FPS'}
    </Label>
  </>;
}
