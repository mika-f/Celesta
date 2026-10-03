import { Camera, Easings, Group, Rect, interpolate, progress, useCurrentFrame, useTypewriter } from '@celesta/react';
import { Caret } from '../components/Caret';
import { Label } from '../components/Label';
import { BEAT, C, H, MONO_ADVANCE, W } from '../constants';
import { DAYS } from '../data';

// Keep in sync with the key clicks in make-score.py.
const PROMPT = 'git log --reverse --date=short --format=%ad';
const LOG = DAYS.flatMap((d) => Array.from({ length: d.commits }, () => d.date));

export function Open() {
  const f = useCurrentFrame();
  const typed = useTypewriter(PROMPT, { from: 6, blinkFrames: BEAT });
  const size = 40;
  const cell = size * MONO_ADVANCE;
  const x0 = 160;
  const y0 = 300;
  const firstY = y0 + 90;

  const shown = f < 60 ? 0 : Math.min(LOG.length, Math.floor((f - 60) * 3) + 1);
  const rows = 8;
  // The first line stays put; the rest of the log scrolls up beneath it.
  const scroll = Math.max(1, shown - rows);
  // From frame 96 the camera flies to the first line and dives into it.
  const aim = progress(f, 96, 18, Easings.easeInOutCubic);
  const dive = progress(f, 96, 24, Easings.easeInExpo);

  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Camera x={interpolate(aim, [0, 1], [W / 2, x0 + 5 * 28 * MONO_ADVANCE])}
        y={interpolate(aim, [0, 1], [H / 2, firstY])} zoom={1 + 7 * dive}>
        <Group x={x0} y={y0}>
          <Label size={size} font="mono" weight={700} color={C.mint} ay={0.5}>$</Label>
          <Label x={cell * 2} size={size} font="mono" ay={0.5}>{typed.text}</Label>
          <Caret x={cell * (2 + typed.length)} size={size} visible={f < 60 && typed.caretVisible} />
        </Group>
        {shown > 0 && <Label x={x0} y={firstY} size={28} font="mono" ay={0.5}>{LOG[0]}</Label>}
        <Group y={firstY + 24} clip={{ x: -W, width: W * 3, height: rows * 44 }}>
          {LOG.slice(scroll, shown).map((date, i) => (
            <Label key={scroll + i} x={x0} y={20 + i * 44} size={28} font="mono" color={C.grey} ay={0.5}
              opacity={1 - 0.8 * progress(f, 84, 10)}>{date}</Label>
          ))}
        </Group>
        <Label x={x0 + 260} y={firstY} size={22} font="mono" color={C.mint} ay={0.5}
          opacity={progress(f, 84, 10)}>{'← day 1'}</Label>
        <Label x={x0} y={firstY + rows * 44 + 70} size={22} font="mono" color={C.grey} ay={0.5}
          opacity={progress(f, 72, 10)}>{`${shown} commits`}</Label>
      </Camera>
    </>
  );
}
