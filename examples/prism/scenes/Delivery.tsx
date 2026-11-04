import { Easings, Rect, progress, useCurrentFrame } from '@celesta.studio/react';
import { Label } from '../components/Label';
import { Prism } from '../components/Prism';
import { Rails } from '../components/Rails';
import { FPS, GREY, H, PAPER, RED } from '../constants';

export function Delivery() {
  const f = useCurrentFrame();
  const sent = progress(f, 0, 105, Easings.easeOutQuart);
  return <>
    <Rails chapter="07 / READY TO SHARE" />
    <Label y={178} size={223}>MAKE IT</Label>
    <Label y={415} size={223} color={RED}>A MOVIE.</Label>
    <Label y={738} size={25} mono>H.264 + AAC / MP4</Label>
    <Label y={788} size={21} mono color={GREY}>1920 × 1080 / 30 FPS / STEREO</Label>
    <Rect x={1050} y={233} width={790} height={563} stroke="#43483F" strokeWidth={2} />
    <Prism frame={f + 720} x={1445} y={510} size={230} color={PAPER} />
    <Rect x={1050} y={833} width={790} height={5} fill="#30352F" />
    <Rect x={1050} y={833} width={Math.max(0.1, 790 * sent)} height={5} fill={RED} />
    <Label x={1050} y={873} size={21} mono color={GREY}>{f >= 105 ? 'YOUR NEXT FILM STARTS HERE.' : 'RENDERING THE POSSIBILITIES'}</Label>
    <Label x={1715} y={735} size={22} mono>{`${String(Math.floor(sent * 100)).padStart(3, '0')}%`}</Label>
  </>;
}
