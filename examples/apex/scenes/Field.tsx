import { Grid, Group, Rect, progress, useBeat, useCurrentFrame } from '@celesta/react';
import { Label } from '../components/Label';
import { ACID, BONE, CYAN, H, INK, MAG, W } from '../constants';

export function Field() {
  const f = useCurrentFrame();
  const { pulse } = useBeat({ bpm: 120, decay: 4 });
  const cols = 24, rows = 14, cw = W / cols, ch = H / rows;
  const out = progress(f, 270, 30);
  return <>
    <Rect width={W} height={H} fill={INK} />
    <Grid columns={cols} columnWidth={cw} rowHeight={ch}>
      {Array.from({ length: cols * rows }, (_, i) => {
        const cx = (i % cols + 0.5) * cw - W / 2, cy = (Math.floor(i / cols) + 0.5) * ch - H / 2;
        const d = Math.hypot(cx, cy);
        const wave = 0.5 + 0.5 * Math.sin(d * 0.011 - f * 0.12 + pulse * 1.5);
        const on = progress(f, d * 0.04, 14);
        const s = (0.12 + 0.86 * wave) * on * (1 - out);
        return <Rect key={i} x={cw / 2} y={ch / 2} anchorX={0.5} anchorY={0.5} width={cw * 0.86 * s}
          height={ch * 0.86 * s} rotation={(1 - wave) * 90 * on} cornerRadius={wave > 0.8 ? ch * 0.43 * s : 0}
          fill={wave < 0.4 ? CYAN : wave < 0.72 ? MAG : ACID} />;
      })}
    </Grid>
    <Group opacity={progress(f, 30, 12) * (1 - out)}>
      <Rect x={W / 2} y={H / 2 + 25} anchorX={0.5} anchorY={0.5} width={1560} height={640} fill={INK} opacity={0.9} />
      <Rect x={W / 2 - 780} y={H / 2 - 295} width={1560} height={4} fill={ACID} />
      <Label x={W / 2} y={H / 2 - 100} size={300} color={BONE} anchorX={0.5} anchorY={0.5}>{'EVERY FRAME'}</Label>
      <Label x={W / 2} y={H / 2 + 150} size={300} color={BONE} anchorX={0.5} anchorY={0.5}>{'A FUNCTION'}</Label>
    </Group>
    <Label x={72} y={64} size={28} mono color={BONE} blendMode="normal">{'06 — FIELD · 336 CELLS'}</Label>
  </>;
}
