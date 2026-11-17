import { Group, Rect, Text } from '@celesta.studio/react';
import { Circle } from '@celesta.studio/shapes';
import { C } from '../../components/theme';
import type { DemoProps } from '../program';
import { mono } from './common';

// PIPELINE: an export's frames, one at a time against pipelined. Before,
// each frame was rendered, copied to the plugin, and encoded before the next
// began. Now the GPU renders into a ring of slots in memory shared with the
// plugin's process, up to three ahead, while the plugin encodes, so the two
// lanes overlap. The ring turns with the frames.

const RENDER = 10;
const COPY = 4;
const ENCODE = 9;
const SCALE = 3.6;

function Lane({ y, label, color }: { y: number; label: string; color: string }) {
  return (
    <Group y={y}>
      <Text x={0} y={10} style={mono(15, C.grey)}>{label}</Text>
      <Rect x={90} y={30} width={600} height={1} fill={C.line} />
      <Rect x={90} y={0} width={4} height={30} fill={color} />
    </Group>
  );
}

export function PipelineDemo({ frame }: DemoProps) {
  const now = frame * 1.2;
  const serial = Array.from({ length: 10 }, (_, i) => i * (RENDER + COPY + ENCODE));
  const piped = Array.from({ length: 20 }, (_, i) => i * RENDER);
  const head = 90 + Math.min(600, now * SCALE);
  const block = (x: number, w: number, y: number, color: string, key: string) => {
    const shown = Math.min(w, Math.max(0, now - x)) * SCALE;
    return shown > 0 ? <Rect key={key} x={90 + x * SCALE} y={y} width={Math.min(shown, 600 - x * SCALE)} height={22} fill={color} /> : null;
  };
  const done = (starts: number[], offset: number) => starts.filter((s) => s + offset <= now).length;
  return (
    <Group x={28}>
      <Text y={22} style={mono(18, C.paper, 700)}>one frame at a time</Text>
      <Text x={690} y={22} anchorX={1} style={mono(16, C.grey)}>{`${done(serial, RENDER + COPY + ENCODE)} frames`}</Text>
      <Lane y={56} label="render" color={C.orange} />
      <Lane y={96} label="encode" color={C.blue} />
      {serial.map((s) => block(s, RENDER, 60, C.orange, `r${s}`))}
      {serial.map((s) => block(s + RENDER, COPY, 60, C.dim, `c${s}`))}
      {serial.map((s) => block(s + RENDER + COPY, ENCODE, 100, C.blue, `e${s}`))}
      <Text y={182} style={mono(18, C.paper, 700)}>pipelined, in shared memory</Text>
      <Text x={690} y={182} anchorX={1} style={mono(16, C.blue, 700)}>{`${done(piped, RENDER + ENCODE)} frames`}</Text>
      <Lane y={216} label="render" color={C.orange} />
      <Lane y={256} label="encode" color={C.blue} />
      {piped.map((s) => block(s, RENDER, 220, C.orange, `pr${s}`))}
      {piped.map((s) => block(s + RENDER, ENCODE, 260, C.blue, `pe${s}`))}
      <Rect x={head} y={40} width={2} height={260} fill={C.paper} />
      <Group x={810} y={170}>
        {[0, 1, 2].map((slot) => {
          const a = ((slot / 3) * 360 + now * 12) * (Math.PI / 180);
          const filled = (Math.floor(now / RENDER) + slot) % 3 !== 0;
          return (
            <Group key={slot} x={Math.cos(a) * 80} y={Math.sin(a) * 80}>
              <Circle x={-34} y={-34} radius={34} fill={filled ? '#F26A2E33' : C.ink} stroke={filled ? C.orange : C.line} strokeWidth={2} />
              <Text anchorX={0.5} anchorY={0.5} style={mono(14, filled ? C.paper : C.dim, 700)}>{`slot ${slot}`}</Text>
            </Group>
          );
        })}
        <Text y={140} anchorX={0.5} style={mono(15, C.grey)}>a ring of frame slots</Text>
      </Group>
      <Text y={360} maxWidth={930} style={mono(17, C.grey)}>
        Frames cross memory shared with the plugin's process; an export keeps up to three in flight, within 256 MiB.
      </Text>
      <Text y={430} style={mono(16, C.dim)}>{'ProviderUse::Export { ring }  ·  PIPE_WINDOW'}</Text>
    </Group>
  );
}
