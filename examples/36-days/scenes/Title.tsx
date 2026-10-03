import { Rect, TextReveal, progress, useCurrentFrame } from '@celesta/react';
import { DotGrid } from '../components/DotGrid';
import { Exit } from '../components/Exit';
import { Label, textStyle } from '../components/Label';
import { BEAT, C, H, W } from '../constants';
import { DAYS } from '../data';

export function Title() {
  const f = useCurrentFrame();
  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <DotGrid />
      <Exit>
        <Label x={160} y={250} size={26} font="mono" weight={700} color={C.mint} ay={0.5}
          opacity={progress(f, BEAT, 10)}>{`${DAYS[0].date}  →  ${DAYS[DAYS.length - 1].date}`}</Label>
        <TextReveal x={160} y={300} lineHeight={200} baseline={0.84} from={2} stagger={6}
          style={textStyle('display', 210)}>
          {'36 DAYS\nOF CELESTA'}
        </TextReveal>
        <Label x={160} y={800} size={34} font="ja" weight={700} ay={0.5} opacity={progress(f, BEAT * 2, 12)}>
          Git の履歴で振り返る、Celesta の 36 日間。
        </Label>
        <Label x={W - 160} y={800} size={20} font="mono" color={C.grey} ax={1} ay={0.5}
          opacity={progress(f, BEAT * 3, 12)}>A DATA FILM, MADE WITH CELESTA</Label>
      </Exit>
    </>
  );
}
