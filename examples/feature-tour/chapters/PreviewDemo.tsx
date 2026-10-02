import { Easings, Group, Rect, interpolate, useCurrentFrame } from '@celesta/react';
import { Label } from '../components/Label';
import { C, DW, DX, S } from '../constants';
import { hash, pad, progress, timecode } from '../helpers';

// ── 07 · Preview: scrub, and the frame answers ────────────────────────────

const MINI_FRAMES = 600;

export function PreviewDemo() {
  const f = useCurrentFrame();
  const enter = progress(f, 2, 16, Easings.easeOutExpo);
  const s = Math.round(interpolate(f, [0, 20, 40, 56, 76, 96, 119], [96, 150, 40, 400, 310, 520, 452], {
    easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp',
  }));
  const prev = Math.round(interpolate(f - 1, [0, 20, 40, 56, 76, 96, 119], [96, 150, 40, 400, 310, 520, 452], {
    easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp',
  }));
  const dir = s > prev ? 'FWD ▸' : s < prev ? '◂ REV' : 'HOLD';
  const muted = f >= 64;

  const wx = DX;
  const wy = 190;
  const vx = wx + 40;
  const vy = wy + 70;
  const vw = 720;
  const vh = 405;
  const tlY = vy + vh + 40;
  const tlX = vx + 90;
  const tlW = vw - 90;
  const head = tlX + (s / MINI_FRAMES) * tlW;

  // The mini scene: a pure function of its own frame number `s`.
  const orbit = Array.from({ length: 10 }, (_, i) => {
    const a = s * 0.035 + (i * Math.PI * 2) / 10;
    const r = 120 + 30 * Math.sin(s * 0.05 + i);
    return (
      <Rect key={i} x={vx + vw / 2 + Math.cos(a) * r * 1.6} y={vy + vh / 2 + Math.sin(a) * r * 0.9}
        anchorX={0.5} anchorY={0.5} width={18} height={18} cornerRadius={9} fill={i % 3 === 0 ? C.pink : C.paper} />
    );
  });

  return (
    <Group y={30 * (1 - enter)} opacity={enter}>
      <Rect x={wx} y={wy} width={DW} height={680} cornerRadius={14} fill={C.panel} stroke={C.line} strokeWidth={1} />
      <Label x={wx + 24} y={wy + 30} size={16} font="mono" weight={700} ay={0.5}>Preview</Label>
      <Label x={wx + DW - 24} y={wy + 30} size={16} font="mono" weight={400} color={C.grey} ax={1} ay={0.5}>
        {`${dir}   ${timecode(s)}`}
      </Label>
      <Rect x={vx} y={vy} width={vw} height={vh} fill={C.blue} />
      <Rect x={vx + vw / 2} y={vy + vh / 2} anchorX={0.5} anchorY={0.5} width={2} height={vh} fill="#FFFFFF22" />
      <Rect x={vx} y={vy + vh / 2} width={vw} height={2} fill="#FFFFFF22" />
      {orbit}
      <Label x={vx + vw / 2} y={vy + vh / 2} size={96} ax={0.5} ay={0.5}>{pad(s, 3)}</Label>
      <Rect x={vx} y={vy + vh - 4} width={Math.max(1, vw * (s / MINI_FRAMES))} height={4} fill={C.paper} />

      {/* Timeline: ruler, a video track, and an audio track with mute/solo. */}
      {Array.from({ length: 21 }, (_, i) => (
        <Rect key={i} x={tlX + (i / 20) * tlW} y={tlY + (i % 5 === 0 ? 0 : 6)} width={1}
          height={i % 5 === 0 ? 14 : 8} fill={C.dim} />
      ))}
      <Label x={vx} y={tlY + 44} size={15} font="mono" weight={700} ay={0.5}>V1</Label>
      <Rect x={tlX} y={tlY + 28} width={tlW} height={32} cornerRadius={4} fill={C.clip} stroke={C.line} strokeWidth={1} />
      <Rect x={tlX} y={tlY + 28} width={4} height={32} fill={C.blue} />
      <Label x={tlX + 14} y={tlY + 44} size={13} font="mono" weight={400} color={C.soft} ay={0.5}>{'<Scene />'}</Label>
      <Label x={vx} y={tlY + 90} size={15} font="mono" weight={700} ay={0.5}>A1</Label>
      <Group x={vx + 34} y={tlY + 90}>
        <Rect y={-11} width={22} height={22} cornerRadius={4} fill={muted ? C.pink : C.dim} />
        <Label x={11} y={0} size={12} font="mono" weight={700} color={muted ? C.ink : C.soft} ax={0.5} ay={0.5}>M</Label>
        <Rect x={26} y={-11} width={22} height={22} cornerRadius={4} fill={C.dim} />
        <Label x={37} y={0} size={12} font="mono" weight={700} color={C.soft} ax={0.5} ay={0.5}>S</Label>
      </Group>
      <Rect x={tlX} y={tlY + 74} width={tlW} height={32} cornerRadius={4} fill={C.clip} stroke={C.line} strokeWidth={1} />
      {Array.from({ length: Math.floor(tlW / 5) - 2 }, (_, b) => {
        const amp = Math.abs(Math.sin(b * 0.23) * Math.sin(b * 0.057 + 1)) * 0.8 + 0.12 * hash(b, 7);
        const bx = tlX + 6 + b * 5;
        return (
          <Rect key={b} x={bx} y={tlY + 90} anchorY={0.5} width={2} height={3 + 24 * amp}
            fill={muted ? C.dim : bx <= head ? C.sky : C.grey} />
        );
      })}
      <Rect x={head - 1} y={tlY - 6} width={2} height={120} fill={C.paper} />
      <Rect x={head} y={tlY - 6} anchorX={0.5} anchorY={0.5} width={12} height={12} cornerRadius={2} rotation={45} fill={C.paper} />
      <Label x={wx + 24} y={wy + 650} size={15} font="mono" weight={400} color={C.grey} ay={0.5}>
        {`frame ${pad(s, 3)} → same pixels, every time`}
      </Label>
      <Label x={wx + DW - 24} y={wy + 650} size={15} font="mono" weight={700} color={muted ? C.pink : C.grey} ax={1} ay={0.5}>
        {muted ? 'A1 muted' : 'A1 on'}
      </Label>
    </Group>
  );
}
