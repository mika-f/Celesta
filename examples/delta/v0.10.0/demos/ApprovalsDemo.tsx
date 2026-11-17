import { Group, Rect, Text } from '@celesta.studio/react';
import { Flag } from '@celesta-plugin/flag';
import { C } from '../../components/theme';
import type { DemoProps } from '../program';
import { Button, Panel, PointerPath, Switch, mono, rise } from './common';
import { PORTRAIT } from './FlagDemo';

// APPROVALS: the Plugins tab, drawn after the user guide. A plugin from an
// added folder waits for approval; until the user allows it, the entry still
// opens and its layer draws a stand-in. Allow is pressed on frame 54, and
// the real <Flag> is drawn from then on.

const ALLOW = 54;

const ROWS: readonly { id: string; source: string; state: string }[] = [
  { id: 'flag', source: 'added folder', state: 'waiting' },
  { id: 'turntable', source: 'added folder', state: 'on' },
  { id: 'code', source: 'celesta.lock 0.10.0', state: 'on' },
  { id: 'ffmpeg', source: 'bundled', state: 'on' },
  { id: 'avfoundation', source: 'bundled', state: 'on' },
];

export function ApprovalsDemo({ frame }: DemoProps) {
  const allowed = frame >= ALLOW;
  return (
    <Group>
      <Panel x={20} y={20} width={500} height={523} title="Plugins" right="5 plugins">
        {ROWS.map((row, i) => {
          const waiting = row.state === 'waiting' && !allowed;
          return (
            <Group key={row.id} y={i * 74}>
              <Rect y={73} width={500} height={1} fill={C.line} />
              <Text x={16} y={16} style={mono(19, C.paper, 700)}>{row.id}</Text>
              <Text x={16} y={44} style={mono(15, waiting ? C.amber : C.dim)}>{waiting ? 'waits for your approval' : row.source}</Text>
              {waiting ? (
                <Group>
                  <Switch x={330} y={24} on={0} />
                  <Button x={390} y={20} label="Allow" primary pressed={frame >= ALLOW - 4} width={90} />
                </Group>
              ) : <Switch x={430} y={24} on={row.state === 'waiting' ? rise(frame, ALLOW, 6) : 1} />}
            </Group>
          );
        })}
        <Text x={16} y={400} maxWidth={470} style={mono(15, C.dim)}>Celesta-export and MCP follow these choices.</Text>
        <Text x={16} y={428} maxWidth={470} style={mono(15, C.dim)}>A plugin turned off gives way to the next with its id.</Text>
      </Panel>
      <Group x={560} y={20}>
        <Rect width={420} height={523} fill={C.ink} stroke={C.line} />
        <Text x={14} y={14} style={mono(15, C.dim)}>Preview</Text>
        {allowed ? (
          <Group opacity={rise(frame, ALLOW, 6)}>
            <Rect x={92} y={70} width={8} height={400} fill="#8A6A4A" />
            <Flag x={100} y={56} scale={0.8} src={PORTRAIT} amplitude={0.1} />
          </Group>
        ) : (
          <Group x={100} y={80}>
            <Rect width={288} height={400} stroke={C.amber} strokeWidth={2} dash={[10, 8]} fill="#FFB5470F" />
            <Text x={144} y={180} anchorX={0.5} style={mono(18, C.amber, 700)}>flag</Text>
            <Text x={144} y={214} anchorX={0.5} style={mono(15, C.grey)}>waits for your approval</Text>
          </Group>
        )}
      </Group>
      <PointerPath f={frame} keys={[[0, 700, 520], [30, 700, 480], [ALLOW - 8, 450, 60], [ALLOW + 20, 450, 64], [110, 760, 300]]}
        presses={[[ALLOW - 4, ALLOW]]} />
    </Group>
  );
}
