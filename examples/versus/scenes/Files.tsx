import { Easings, Group, Rect, useCurrentFrame } from '@celesta/react';
import { Header, ToolTag } from '../components/Chrome';
import { Label } from '../components/Label';
import { C, ORDER, TOOL } from '../constants';
import { LINES } from '../data';
import { progress } from '../helpers';
import { FILES } from '../snippets';

const COL = 560;

// What one video needs on disk, and the command that renders it.
export function Files() {
  const f = useCurrentFrame();
  return (
    <>
      <Header f={f} n="03 — SYNTAX" title="What one video takes." sub="Every file written to get NEBULA to an MP4." />
      {ORDER.map((id, i) => {
        const p = progress(f, 14 + i * 8, 22, Easings.easeOutExpo);
        const x = 96 + i * (COL + 24);
        const { files, command } = FILES[id];
        const lines = LINES[id];
        const celesta = id === 'celesta';
        const count = Math.round(files.length * progress(f, 40 + i * 8, 30, Easings.easeOutCubic));
        return (
          <Group key={id} x={x} y={330 + 30 * (1 - p)} opacity={p}>
            <Rect width={COL} height={600} cornerRadius={20} fill={C.panel}
              stroke={celesta ? TOOL.celesta.color : C.line} strokeWidth={celesta ? 2 : 1} />
            <Rect x={0} y={0} width={COL} height={6} cornerRadius={3} fill={TOOL[id].color} />
            <ToolTag id={id} x={36} y={36} size={32} />
            <Group x={36} y={150}>
              <Label size={120} weight={700} spacing={-4} color={TOOL[id].color}>{String(count)}</Label>
              <Label x={files.length >= 10 ? 150 : 86} y={70} size={30} weight={500}>{files.length === 1 ? 'file' : 'files'}</Label>
              <Label x={COL - 72} y={70} ax={1} size={22} font="mono" weight={400} color={C.soft}>{`${lines.scene + lines.other} lines`}</Label>
            </Group>
            {files.map(([name, what], j) => {
              const q = progress(f, 40 + i * 8 + j * 5, 16);
              return (
                <Group key={name} x={36} y={310 + j * 40} opacity={q}>
                  <Label size={20} font="mono" weight={700}>{name}</Label>
                  <Label x={COL - 72} y={2} ax={1} size={17} weight={400} color={C.grey}>{what}</Label>
                </Group>
              );
            })}
            <Group y={530} opacity={progress(f, 80 + i * 6, 18)}>
              <Rect x={20} width={COL - 40} height={48} cornerRadius={10} fill="#00000066" />
              <Label x={40} y={14} size={17} font="mono" weight={400} color={C.soft}>{`$ ${command}`}</Label>
            </Group>
          </Group>
        );
      })}
    </>
  );
}
