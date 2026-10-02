import { Easings, Group, Rect, interpolate, useCurrentFrame } from '@celesta/react';
import { CHAPTERS } from '../chapters';
import { Label } from '../components/Label';
import { BAR, BEAT, C, H, W } from '../constants';
import { clamp, pad, progress } from '../helpers';

// ── 00 · Cold open: a ball bounces through one frame, then fills the screen ──

export function Open() {
  const f = useCurrentFrame();
  const fw = 960;
  const fh = 540;
  const fx = (W - fw) / 2;
  const fy = (H - fh) / 2 - 20;
  const floor = fy + fh - 70;
  const frameIn = progress(f, 0, 12, Easings.easeOutExpo);
  const frameOut = 1 - progress(f, 46, 6);

  // The ball lands on every beat: 0, 15, 30, then rests at the center on 45.
  const beat = Math.floor(f / BEAT);
  const u = (f % BEAT) / BEAT;
  const hop = beat < 3 ? 4 * u * (1 - u) * [230, 180, 130][beat] : 0;
  const x = interpolate(f, [0, 45], [fx + 150, W / 2], {
    easing: Easings.easeOutSine, extrapolateLeft: 'clamp', extrapolateRight: 'clamp',
  });
  const land = beat < 3 ? Math.min(u, 1 - u) : 1; // 0 at the moment of contact
  const squash = beat < 4 ? 1 - 0.28 * clamp(1 - land / 0.12) : 1;
  const d = 110;
  const ballIn = progress(f, 2, 10, Easings.easeOutBack);
  // From frame 46 the ball opens like an iris until it covers the screen.
  const iris = progress(f, 46, 14, Easings.easeInCubic);
  const size = d + (2300 - d) * iris;
  const cy = floor - d / 2 - hop + (H / 2 - (floor - d / 2)) * iris;

  const ball = (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Group opacity={frameIn * frameOut} y={20 * (1 - frameIn)}>
        <Rect x={fx} y={fy} width={fw} height={fh} cornerRadius={18} fill={C.panel} stroke="#FFFFFF33" strokeWidth={2} />
        <Rect x={fx + 60} y={floor} width={fw - 120} height={2} fill="#FFFFFF22" />
        <Label x={fx + 28} y={fy + 34} size={18} font="mono" weight={700} color={C.soft} ay={0.5}>
          {`frame ${pad(f, 3)}`}
        </Label>
        {/* The ball's shadow shrinks as it rises. */}
        <Rect x={x} y={floor + 1} anchorX={0.5} anchorY={0.5} width={d * (1 - hop / 400)} height={10}
          cornerRadius={5} fill="#000000" opacity={0.35 * (1 - hop / 300) * ballIn} />
        <Label x={fx} y={fy + fh + 50} size={20} font="mono" weight={700} ay={0.5}>{'const y = bounce(frame);'}</Label>
        <Label x={fx + fw} y={fy + fh + 50} size={20} font="mono" weight={400} color={C.soft} ax={1} ay={0.5}>
          {'f(frame) → pixels'}
        </Label>
      </Group>
      <Rect x={x} y={cy} anchorX={0.5} anchorY={0.5}
        width={size * (iris > 0 ? 1 : (2 - squash))} height={size * (iris > 0 ? 1 : squash)}
        cornerRadius={size / 2} fill={C.blue} scale={iris > 0 ? 1 : ballIn} />
    </>
  );

  // Bar 1: the title.
  const t = f - BAR;
  const hit = progress(t, 0, 16, Easings.easeOutExpo);
  const label = progress(t, BEAT, 12, Easings.easeOutExpo);
  const ja = progress(t, BEAT * 2, 12, Easings.easeOutExpo);

  return (
    <>
      {t < 0 && ball}
      {t >= 0 && (
        <>
          <Rect width={W} height={H} fill={C.blue} />
          <Label x={W / 2} y={H / 2 - 10 + 30 * (1 - hit)} size={250} ax={0.5} ay={0.5} scale={1.08 - 0.08 * hit}
            opacity={clamp(hit * 1.5)}>Celesta</Label>
          <Label x={W / 2} y={H / 2 - 210 + 16 * (1 - label)} size={22} font="mono" weight={700} ax={0.5} ay={0.5}
            opacity={label}>{`FEATURE TOUR  ·  ${pad(CHAPTERS.length)} CHAPTERS`}</Label>
          <Label x={W / 2} y={H / 2 + 190 + 16 * (1 - ja)} size={40} font="ja" weight={700} ax={0.5} ay={0.5}
            opacity={ja}>{TAGLINE}</Label>
        </>
      )}
    </>
  );
}

export const TAGLINE = '動画を、コードで書く。';
