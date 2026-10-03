import { Easings, Group, Rect, useCurrentFrame } from '@celesta/react';
import { Bars } from '../components/Bars';
import { Footnote, Header, ToolTag } from '../components/Chrome';
import { Label } from '../components/Label';
import { C, ORDER, TOOL, type ToolId } from '../constants';
import { LOOP, LOOP_RUNS, SETUP } from '../data';
import { progress } from '../helpers';

const NEEDS: Record<ToolId, string[]> = {
  remotion: ['Node.js', `npm install (${SETUP.remotion.seconds} s)`, 'Chrome Headless Shell, fetched on first render'],
  fframes: ['Rust toolchain + LLVM (libclang)', 'FFmpeg 9 shared build: FFMPEG_DIR, DLLs on PATH', 'Vulkan GPU',
    `${SETUP.fframes.install} (${SETUP.fframes.seconds} s)`],
  celesta: ['Install the app. Node.js is bundled.'],
};

export function Loop() {
  const f = useCurrentFrame();
  let y = 0;
  return (
    <>
      <Header f={f} n="04 — EASE" title="Write it. See it." sub="Three fresh projects on Windows 11, then the same one-line edit in each." />

      <Group x={96} y={330}>
        <Label size={18} font="mono" weight={700} color={C.grey} spacing={3} opacity={progress(f, 10, 16)}>BEFORE THE FIRST FRAME</Label>
        {ORDER.map((id, i) => {
          const top = 50 + y;
          y += 66 + NEEDS[id].length * 38;
          const p = progress(f, 16 + i * 10, 20, Easings.easeOutExpo);
          return (
            <Group key={id} y={top + 20 * (1 - p)} opacity={p}>
              <ToolTag id={id} size={28} sub={false} />
              {NEEDS[id].map((need, j) => (
                <Group key={need} x={22} y={48 + j * 38}>
                  <Rect y={10} width={10} height={2} fill={TOOL[id].color} />
                  <Label x={24} size={21} weight={id === 'celesta' ? 700 : 400} color={id === 'celesta' ? C.ink : C.soft}>{need}</Label>
                </Group>
              ))}
            </Group>
          );
        })}
      </Group>

      <Group x={960} y={330}>
        <Label size={18} font="mono" weight={700} color={C.grey} spacing={3} opacity={progress(f, 60, 16)}>
          CHANGE ONE COLOR → FRAME 300 AS PNG
        </Label>
        <Group y={66}>
          <Bars f={f} start={70} width={870} max={Math.max(...Object.values(LOOP)) * 1.05} unit="s" labelWidth={240}
            bars={[
              { key: 'remotion', label: 'Remotion', sub: 're-bundle + Chromium', value: LOOP.remotion, color: TOOL.remotion.color },
              { key: 'fframes', label: 'fframes', sub: 'cargo build --release', value: LOOP.fframes, color: TOOL.fframes.color },
              { key: 'celesta', label: 'Celesta', sub: 're-bundle', value: LOOP.celesta, color: TOOL.celesta.color, strong: true },
            ]} />
        </Group>
        <Label y={460} size={22} lineHeight={34} weight={400} color={C.soft} opacity={progress(f, 150, 20)}>
          {'Rust recompiles on every change.\nIn the Celesta app, saving the file reloads the preview.'}
        </Label>
      </Group>
      <Footnote opacity={progress(f, 120, 20)}>{`measured from the CLI · ${LOOP_RUNS}`}</Footnote>
    </>
  );
}
