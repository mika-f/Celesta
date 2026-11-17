import { Group, Rect, Text } from '@celesta.studio/react';
import { C } from '../../components/theme';
import type { DemoProps } from '../program';
import { Button, PointerPath, Typed, mono, rise } from './common';

// FIND PLUGINS: the app's plugin page, drawn after the handoff of #850.
// Find plugins… opens it; the search lists the registry's plugins (the ten
// published for 0.10.0), and Add to this project writes the config and the
// lock and downloads the plugin, as `celesta plugin add` does.

const PLUGINS: readonly [string, string][] = [
  ['vrm', 'content'], ['voicevox', 'content'], ['aivis-speech', 'content'],
  ['lottie', 'content'], ['code', 'content'], ['ffmpeg-x264', 'media'],
];

const QUERY = 'vrm';
const TYPE_AT = 10;
const CLICK = 64;

export function FindDemo({ frame }: DemoProps) {
  const typed = (frame - TYPE_AT) / 3;
  const filtered = typed >= QUERY.length;
  const rows = filtered ? PLUGINS.filter(([id]) => id.includes(QUERY)) : PLUGINS;
  const adding = frame >= CLICK;
  const done = frame >= CLICK + 26;
  return (
    <Group>
      <Rect width={1000} height={563} fill={C.ink} />
      <Rect width={1000} height={48} fill={C.raised} />
      <Text x={20} y={24} anchorY={0.5} style={mono(18, C.paper, 700)}>Find plugins</Text>
      <Text x={980} y={24} anchorX={1} anchorY={0.5} style={mono(15, C.dim)}>registry.celesta.studio</Text>
      <Group x={20} y={68}>
        <Rect width={420} height={40} fill={C.panel} stroke={C.orange} />
        <Typed x={14} y={10} text={QUERY} n={typed} style={mono(18, C.paper)} caret={frame < CLICK - 20} />
        <Rect x={440} y={10} width={20} height={20} fill={C.orange} />
        <Text x={470} y={20} anchorY={0.5} style={mono(15, C.grey)}>Include pre-releases</Text>
      </Group>
      <Group x={20} y={128}>
        {rows.map(([id, kind], i) => (
          <Group key={id} y={i * 56}>
            <Rect width={420} height={52} fill={filtered && i === 0 ? C.raised : C.ink} />
            <Rect y={52} width={420} height={1} fill={C.line} />
            <Text x={14} y={14} style={mono(18, C.paper, 700)}>{id}</Text>
            <Text x={240} y={16} style={mono(15, C.dim)}>{kind}</Text>
            <Text x={406} y={16} anchorX={1} style={mono(15, C.grey)}>0.10.0</Text>
          </Group>
        ))}
      </Group>
      <Group x={470} y={128} opacity={rise(frame, TYPE_AT + 14, 8)}>
        <Text style={{ fontFamily: ['Bebas Neue'], fontSize: 64, fill: { type: 'solid', color: C.paper } }}>vrm</Text>
        <Text y={74} style={mono(16, C.grey)}>content · MIT OR Apache-2.0 · by celesta</Text>
        {[
          ['0.10.0', 'celesta >=0.10.0, <0.11.0'],
          ['builds', 'linux-x86_64 · macos-aarch64 · windows-x86_64'],
          ['here', done ? 'locked in this project: 0.10.0' : 'not locked in this project'],
        ].map(([k, v], i) => (
          <Group key={k} y={124 + i * 40}>
            <Text style={mono(16, C.dim)}>{k}</Text>
            <Text x={90} maxWidth={420} style={mono(16, i === 2 && done ? C.green : C.paper)}>{v}</Text>
          </Group>
        ))}
        <Button x={0} y={268} label={done ? 'Added' : adding ? 'Adding…' : 'Add to this project'} primary={!done} pressed={adding && !done} width={240} />
        {adding ? (
          <Group y={320}>
            <Rect width={480} height={6} fill={C.line} />
            <Rect width={480 * rise(frame, CLICK, 24)} height={6} fill={done ? C.green : C.orange} />
            <Text y={20} style={mono(15, C.grey)}>{done ? 'celesta.config.json · celesta.lock · downloaded' : 'resolving · downloading · checking hashes'}</Text>
          </Group>
        ) : null}
      </Group>
      <PointerPath f={frame} keys={[[0, 900, 500], [30, 700, 480], [CLICK - 6, 590, 412], [CLICK + 30, 600, 420], [119, 820, 520]]}
        presses={[[CLICK - 4, CLICK]]} />
    </Group>
  );
}
