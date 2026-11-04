import { BarChart } from '@celesta.studio/components';
import { Easings, Group, Rect, progress, useCurrentFrame } from '@celesta.studio/react';
import { Footnote, Header, ToolTag } from '../components/Chrome';
import { Label } from '../components/Label';
import { C, FONT, ORDER, TOOL, type ToolId } from '../constants';
import { LOOP, LOOP_RUNS, SETUP } from '../data';

const NEEDS: Record<ToolId, string[]> = {
  remotion: ['Node.js', `npm install (${SETUP.remotion.seconds} s)`, 'Chrome Headless Shell, fetched on first render'],
  fframes: ['Rust toolchain + LLVM (libclang)', 'FFmpeg 9 shared build: FFMPEG_DIR, DLLs on PATH', 'Vulkan GPU',
    `${SETUP.fframes.install} (${SETUP.fframes.seconds} s)`],
  celesta: ['Install the app. Node.js is bundled.'],
};

// What each tool redoes after the edit.
const CHANGE: Record<ToolId, string> = {
  remotion: 're-bundle + Chromium',
  fframes: 'cargo build --release',
  celesta: 're-bundle',
};

export function Loop() {
  const f = useCurrentFrame();
  let y = 0;
  return (
    <>
      <Header n="04 — EASE" title="Write it. See it." sub="Three fresh projects on Windows 11, then the same one-line edit in each." />

      <Group x={96} y={330}>
        <Label size={18} font="mono" weight={700} color={C.grey} spacing={3} opacity={progress(f, 10, 16, Easings.easeOutCubic)}>BEFORE THE FIRST FRAME</Label>
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
        <Label size={18} font="mono" weight={700} color={C.grey} spacing={3} opacity={progress(f, 60, 16, Easings.easeOutCubic)}>
          CHANGE ONE COLOR → FRAME 300 AS PNG
        </Label>
        {/* Bars that grow in turn while their values count up; Celesta's in full color.
            The others' bars are at 75% opacity. */}
        <BarChart y={66} width={870} max={Math.max(...Object.values(LOOP)) * 1.05} decimals={1} suffix=" s"
          delay={70} stagger={10} durationInFrames={40} easing={Easings.easeOutCubic} fadeInFrames={16}
          labelWidth={240} labelGap={0} valueWidth={150} valueGap={20}
          rowHeight={64} rowGap={56} barHeight={56} cornerRadius={10} trackColor="#FFFFFF08"
          labelStyle={{ fontFamily: FONT.sans, fontSize: 28, fontWeight: 700, fill: { type: 'solid', color: C.ink } }}
          noteStyle={{ fontFamily: FONT.sans, fontSize: 18, fontWeight: 400, fill: { type: 'solid', color: C.grey } }}
          valueStyle={{ fontFamily: FONT.mono, fontSize: 38, fontWeight: 700, fill: { type: 'solid', color: C.ink } }}
          data={ORDER.map((id) => {
            const strong = id === 'celesta';
            return {
              id, label: TOOL[id].name, note: CHANGE[id], value: LOOP[id],
              color: strong ? TOOL[id].color : `${TOOL[id].color}BF`,
              textColor: strong ? TOOL[id].color : undefined,
            };
          })} />
        <Label y={460} size={22} lineHeight={34} weight={400} color={C.soft} opacity={progress(f, 150, 20, Easings.easeOutCubic)}>
          {'Rust recompiles on every change.\nIn the Celesta app, saving the file reloads the preview.'}
        </Label>
      </Group>
      <Footnote opacity={progress(f, 120, 20, Easings.easeOutCubic)}>{`measured from the CLI · ${LOOP_RUNS}`}</Footnote>
    </>
  );
}
