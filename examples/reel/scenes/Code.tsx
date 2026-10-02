import type { Run } from '../components/Mono';
import { Easings, Group, Rect, spring, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { Label } from '../components/Label';
import { Mono, runLength } from '../components/Mono';
import { Swap } from '../components/Swap';
import { Tag } from '../components/Tag';
import { C, H, W } from '../constants';
import { clamp, progress } from '../helpers';
import { monoAdvance } from '../metrics';

// Local frames of the scene. Keep RELOADS in make-music.py in sync
// (scene start + these frames, in seconds).
const SAVE = 90;
const EDIT_SIZE = 150;
const EDIT_TEXT = 180;

const K = {
  kw: (text: string): Run => ({ text, color: C.accent }),
  id: (text: string): Run => ({ text, color: C.paper }),
  fn: (text: string): Run => ({ text, color: C.paper, weight: 700 }),
  p: (text: string): Run => ({ text, color: C.grey }),
  num: (text: string): Run => ({ text, color: C.tint }),
  str: (text: string): Run => ({ text, color: C.soft }),
};

// The lines of title.tsx; `size` and `label` are the two values edited live.
function codeLines(size: string, label: string): Run[][] {
  const { kw, id, fn, p, num, str } = K;
  return [
    [kw('import'), p(' { '), id('Text'), p(', '), id('spring'), p(', '), id('useCurrentFrame'), p(' } '), kw('from'), p(' '), str("'@celesta/react'"), p(';')],
    [],
    [kw('export default function'), p(' '), fn('Title'), p('() {')],
    [p('  '), kw('const'), p(' '), id('frame'), p(' = '), fn('useCurrentFrame'), p('();')],
    [p('  '), kw('const'), p(' '), id('s'), p(' = '), fn('spring'), p('({ '), id('frame'), p(', '), id('fps'), p(': '), num('30'), p(' });')],
    [p('  '), kw('return'), p(' (')],
    [p('    <'), fn('Text'), p(' '), id('x'), p('={'), num('960'), p('} '), id('y'), p('={'), num('540'), p(' + '), num('80'), p(' * ('), num('1'), p(' - '), id('s'), p(')}')],
    [p('      '), id('anchorX'), p('={'), num('0.5'), p('} '), id('anchorY'), p('={'), num('0.5'), p('} '), id('opacity'), p('={'), id('s'), p('}')],
    [p('      '), id('style'), p('={{ '), id('fontSize'), p(': '), num(size), p(' }}>')],
    [p('      '), str(label)],
    [p('    </'), fn('Text'), p('>')],
    [p('  );')],
    [p('}')],
  ];
}
const SIZE_LINE = 8;
const SIZE_COL = runLength(codeLines('', '')[SIZE_LINE].slice(0, 5));
const LABEL_LINE = 9;
const LABEL_COL = 6;

export function Code() {
  const f = useCurrentFrame();
  const { fps, durationInFrames } = useVideoConfig();
  const size = f >= EDIT_SIZE ? '180' : '120';
  const label = f >= EDIT_TEXT ? 'Code is the cut.' : 'Hello, Celesta.';
  const lines = codeLines(size, label);

  const inAt = progress(f, 0, 14, Easings.easeOutExpo);
  const exit = 1 - progress(f, durationInFrames - 14, 12, Easings.easeInCubic);

  // Editor.
  const ex = 120;
  const ey = 330;
  const ew = 930;
  const cs = 21;
  const cw = cs * monoAdvance;
  const lh = 38;
  const codeX = ex + 64;
  const codeY = ey + 48 + 34;
  const typedChars = Math.max(0, (f - 8) * 4.5);
  let budget = typedChars;
  let caret: [number, number] = [0, 0];
  const rendered = lines.map((runs, li) => {
    const len = runLength(runs);
    const shown = Math.floor(clamp(budget, 0, len));
    if (budget > 0) caret = [li, shown];
    budget -= len + 1; // a newline costs one character
    return (
      <Group key={li}>
        {budget > -len - 1 && (
          <Label x={ex + 44} y={codeY + li * lh} size={16} font="mono" weight={400} color={C.dim} ax={1} ay={0.5}>
            {String(li + 1)}
          </Label>
        )}
        <Mono runs={runs} x={codeX} y={codeY + li * lh} size={cs} visible={shown} />
      </Group>
    );
  });
  // After typing, the caret jumps to each edit as it happens.
  if (f >= EDIT_SIZE - 10) caret = [SIZE_LINE, SIZE_COL + size.length];
  if (f >= EDIT_TEXT - 10) caret = [LABEL_LINE, LABEL_COL + label.length];
  const selection = (line: number, col: number, len: number, at: number) => {
    const on = f >= at - 10 && f < at + 16;
    if (!on) return null;
    const flash = f < at ? 0.35 : 0.35 * (1 - progress(f, at, 16));
    return <Rect x={codeX + col * cw - 3} y={codeY + line * lh - lh / 2 + 2} width={len * cw + 6} height={lh - 4}
      cornerRadius={4} fill={C.accent} opacity={flash} />;
  };

  // Preview: a 1920×1080 canvas drawn at k scale, playing the code above.
  const px = 1100;
  const py = 330;
  const pw = 700;
  const k = pw / W;
  const ph = H * k;
  const reloads = [SAVE, EDIT_SIZE, EDIT_TEXT];
  const lastReload = reloads.reduce((found, at) => (f >= at ? at : found), -1);
  const s = lastReload < 0 ? 0 : spring({ frame: f - lastReload, fps, config: { damping: 12, stiffness: 120 } });
  const badge = lastReload < 0 ? 0 : 1 - progress(f, lastReload + 18, 10);
  const previewFrame = lastReload < 0 ? 0 : f - lastReload;

  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Group opacity={exit}>
        <Tag x={120} y={150} n="03" name="CODE" opacity={inAt} />
        <Swap f={f} x={120} y={190} size={110} accentLast
          cues={[[0, 'Write it.'], [SAVE, 'Save it.'], [EDIT_SIZE, 'Change it.']]} />

        <Group y={30 * (1 - inAt)} opacity={inAt}>
          <Rect x={ex} y={ey} width={ew} height={590} cornerRadius={12} fill={C.panel} stroke="#FFFFFF14" strokeWidth={1} />
          <Rect x={ex} y={ey + 47} width={ew} height={1} fill="#FFFFFF14" />
          <Rect x={ex + 24} y={ey + 44} width={130} height={3} fill={C.accent} />
          <Label x={ex + 24} y={ey + 24} size={18} font="mono" weight={700} ay={0.5}>title.tsx</Label>
          <Label x={ex + ew - 24} y={ey + 24} size={16} font="mono" weight={400} color={C.grey} ax={1} ay={0.5}>
            {f >= SAVE ? 'saved' : 'modified ●'}
          </Label>
          {selection(SIZE_LINE, SIZE_COL, 3, EDIT_SIZE)}
          {selection(LABEL_LINE, LABEL_COL, label.length, EDIT_TEXT)}
          {rendered}
          {Math.floor(f / 8) % 2 === 0 && (
            <Rect x={codeX + caret[1] * cw} y={codeY + caret[0] * lh - 14} width={3} height={28} fill={C.accent} />
          )}
        </Group>

        <Group y={30 * (1 - progress(f, 4, 14, Easings.easeOutExpo))} opacity={progress(f, 4, 14)}>
          <Rect x={px} y={py} width={pw} height={ph + 48} cornerRadius={12} fill={C.panel} stroke="#FFFFFF14" strokeWidth={1} />
          <Label x={px + 24} y={py + 24} size={18} font="mono" weight={700} ay={0.5}>Preview</Label>
          <Label x={px + pw - 24} y={py + 24} size={16} font="mono" weight={400} color={C.grey} ax={1} ay={0.5}>
            {`frame ${String(previewFrame).padStart(3, '0')}`}
          </Label>
          <Rect x={px} y={py + 48} width={pw} height={ph} fill={C.paper} />
          {lastReload >= 0 && (
            <Label x={px + 960 * k} y={py + 48 + (540 + 80 * (1 - s)) * k} size={Number(size) * k}
              ax={0.5} ay={0.5} opacity={clamp(s)} color={C.ink}>{label}</Label>
          )}
          {lastReload < 0 && (
            <Label x={px + pw / 2} y={py + 48 + ph / 2} size={18} font="mono" weight={400} color={C.grey} ax={0.5} ay={0.5}>
              waiting for save…
            </Label>
          )}
          <Rect x={px} y={py + 48 + ph - 4} width={Math.max(1, pw * clamp(previewFrame / 60))} height={4} fill={C.accent} />
          <Group x={px + pw - 20} y={py + 48 + 22} opacity={badge}>
            <Rect x={-150} y={-17} width={150} height={34} cornerRadius={17} fill={C.accent} />
            <Label x={-75} y={0} size={16} font="mono" weight={700} color={C.ink} ax={0.5} ay={0.5}>↻ reloaded</Label>
          </Group>
        </Group>
        <Mono x={px} y={py + ph + 100} size={20} opacity={progress(f, SAVE, 12)}
          runs={[{ text: 'Save the file. ', color: C.paper }, { text: 'The preview reloads.', color: C.grey }]} />
      </Group>
    </>
  );
}
