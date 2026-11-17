import { Group, Rect, Text } from '@celesta.studio/react';
import { C } from '../../components/theme';
import type { DemoProps } from '../program';
import { Typed, mono } from './common';

// CELESTA PLUGIN: a terminal. The commands and their output are the
// celesta command's own (search printed by the registry on 2026-11-17;
// add's two lines are the text crates/cli/src/commands/project.rs prints).

interface Line { at: number; text: string; command?: boolean; color?: string }

const LINES: readonly Line[] = [
  { at: 2, text: 'celesta plugin search vrm', command: true },
  { at: 24, text: 'vrm  0.10.0' },
  { at: 34, text: 'celesta plugin add vrm', command: true },
  { at: 54, text: 'added vrm 0.10.0, locked 0.10.0', color: C.green },
  { at: 58, text: 'installed vrm 0.10.0', color: C.green },
  { at: 68, text: 'celesta plugin install --locked', command: true },
  { at: 92, text: 'celesta.lock is satisfied', color: C.green },
];

const PROMPT = 'examples/delta %';

export function CommandDemo({ frame }: DemoProps) {
  const style = mono(22, C.paper);
  return (
    <Group>
      <Rect width={1000} height={44} fill={C.raised} />
      <Text x={20} y={22} anchorY={0.5} style={mono(16, C.dim)}>zsh · Celesta-cli</Text>
      <Group x={28} y={70}>
        {LINES.filter((line) => frame >= line.at).map((line, i) => (
          <Group key={i} y={i * 56}>
            {line.command ? (
              <>
                <Text style={mono(22, C.grey)}>{PROMPT}</Text>
                <Typed x={17 * 0.6 * 22 + 4} y={0} text={line.text} n={(frame - line.at) * 1.6} style={mono(22, C.paper, 700)}
                  caret={LINES.findIndex((l) => l.at > line.at && frame >= l.at) < 0} />
              </>
            ) : <Text style={{ ...style, fill: { type: 'solid', color: line.color ?? C.paper } }}>{line.text}</Text>}
          </Group>
        ))}
      </Group>
    </Group>
  );
}
