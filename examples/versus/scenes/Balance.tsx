import { Easings, Group, Rect, spring, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { Header } from '../components/Chrome';
import { Label } from '../components/Label';
import { C, ORDER, TOOL } from '../constants';
import { EXPORT, LOOP, fmt } from '../data';
import { progress } from '../helpers';

const X0 = 260;
const Y0 = 930;
const PW = 1060;
const PH = 560;

const VERDICT = [
  ['remotion', 'Writes like React. Renders like a browser.'],
  ['fframes', 'Fastest render. Rust, and a rebuild per edit.'],
  ['celesta', 'Writes like React. Renders on the GPU.'],
] as const;

// Edit loop (x) against export time (y): lower left is better on both.
export function Balance() {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const maxX = Math.ceil(Math.max(...Object.values(LOOP)) / 2) * 2 + 2;
  const maxY = Math.ceil(Math.max(EXPORT.remotion, EXPORT.fframes, EXPORT.celesta) / 10) * 10 + 10;
  const px = (s: number) => X0 + (s / maxX) * PW;
  const py = (s: number) => Y0 - (s / maxY) * PH;
  const axes = progress(f, 6, 24, Easings.easeOutExpo);
  const zone = progress(f, 90, 30);
  return (
    <>
      <Header f={f} n="06 — BALANCE" title="Easy and fast." sub="Lower left wins: quicker to check an edit, quicker to export." />
      <Rect x={X0} y={Y0 - PH * 0.5} width={PW * 0.34} height={PH * 0.5} cornerRadius={20}
        fill={`${TOOL.celesta.color}1C`} stroke={`${TOOL.celesta.color}55`} strokeWidth={1} opacity={zone} />
      <Label x={X0 + 24} y={Y0 - 46} size={18} font="mono" weight={700} color={TOOL.celesta.color} spacing={3} opacity={zone}>SWEET SPOT</Label>
      <Rect x={X0} y={Y0 - PH * axes} width={2} height={PH * axes} fill={C.grey} />
      <Rect x={X0} y={Y0} width={PW * axes} height={2} fill={C.grey} />
      <Label x={X0 + PW} y={Y0 + 60} ax={1} size={16} font="mono" weight={700} color={C.soft} spacing={2} opacity={axes}>EDIT → PREVIEW (s) →</Label>
      <Label x={X0 - 40} y={Y0 - PH - 60} size={16} font="mono" weight={700} color={C.soft} spacing={2} opacity={axes}>↑ EXPORT (s)</Label>
      {[0, maxY / 2, maxY].map((v) => (
        <Label key={v} x={X0 - 20} y={py(v)} ax={1} ay={0.5} size={16} font="mono" weight={400} color={C.grey} opacity={axes}>{`${v}`}</Label>
      ))}
      {[0, maxX / 2, maxX].map((v) => (
        <Label key={v} x={px(v)} y={Y0 + 22} ax={0.5} size={16} font="mono" weight={400} color={C.grey} opacity={axes}>{`${v}`}</Label>
      ))}
      {ORDER.map((id, i) => {
        const s = spring({ frame: f - 30 - i * 12, fps, config: { damping: 12, stiffness: 160 } });
        const size = id === 'celesta' ? 40 : 28;
        return (
          <Group key={id} x={px(LOOP[id])} y={py(EXPORT[id])}>
            <Rect anchorX={0.5} anchorY={0.5} width={size} height={size} cornerRadius={size / 2} scale={Math.max(0, s)}
              fill={TOOL[id].color} glow={id === 'celesta' ? { color: TOOL.celesta.color, blur: 18 } : undefined} />
            <Group x={34} y={-30} opacity={Math.min(1, Math.max(0, s))}>
              <Label size={34} weight={700} color={TOOL[id].color}>{TOOL[id].name}</Label>
              <Label y={44} size={16} font="mono" weight={400} color={C.soft}>{`${fmt(LOOP[id])}s edit · ${fmt(EXPORT[id])}s export`}</Label>
            </Group>
          </Group>
        );
      })}
      <Group x={1420} y={400}>
        {VERDICT.map(([id, text], i) => (
          <Group key={id} y={i * 130} opacity={progress(f, 110 + i * 10, 16)}>
            <Rect width={4} height={92} fill={TOOL[id].color} />
            <Label x={24} size={32} weight={700} color={TOOL[id].color}>{TOOL[id].name}</Label>
            <Label x={24} y={48} size={20} weight={400} color={C.soft} >{text}</Label>
          </Group>
        ))}
      </Group>
    </>
  );
}
