import { Group, Rect, Text } from '@celesta.studio/react';
import { C } from '../../components/theme';
import type { DemoProps } from '../program';
import { Check, mono, rise } from './common';

// REGISTRY: how Celesta trusts registry.celesta.studio, a step a beat. The
// root key built into Celesta certifies the online key config.json lists;
// that key signs every file; the index gives each build's archive hash and
// the unpacked tree's hash, checked before the plugin goes into the store.
// The key, key name, and hashes are the real ones (this project's lock).

const STEPS: readonly [string, string, string][] = [
  ['root key', 'built into Celesta', 'eqOBn7d6SvlrSXx+f3oWpJPhxxAn209/9F8QqhIBSs0='],
  ['config.json', 'certifies online key 2026-11', 'expires 2027-11-15T00:00:00Z'],
  ['index/code.json', 'signed by 2026-11 · .sig', 'code 0.10.0-alpha.4 · targets: any'],
  ['archive', 'sha256', '7196aebb71133d4b4574861e0dd57204f8ace15510e2…'],
  ['tree', 'treeHash', 'ee3e8448ba5cd393d3e182aeac2a20ff2bf3a196f2fa…'],
];

export function RegistryDemo({ frame }: DemoProps) {
  return (
    <Group x={34} y={30}>
      <Text style={mono(18, C.dim)}>https://registry.celesta.studio</Text>
      {STEPS.map(([name, how, value], i) => {
        const at = 8 + i * 15;
        const p = rise(frame, at, 8);
        return (
          <Group key={name} y={44 + i * 96} opacity={0.25 + 0.75 * p}>
            {i > 0 ? <Rect x={22} y={-36} width={3} height={36} fill={p > 0.5 ? C.green : C.line} /> : null}
            <Rect width={932} height={70} fill={i === 0 ? '#3DDC841A' : C.ink} stroke={p > 0.5 ? C.green : C.line} strokeWidth={i === 0 ? 2 : 1} />
            <Text x={56} y={14} style={mono(20, C.paper, 700)}>{name}</Text>
            <Text x={56} y={42} style={mono(15, C.grey)}>{how}</Text>
            <Text x={900} y={35} anchorX={1} anchorY={0.5} maxWidth={560} style={mono(16, i === 0 ? C.green : C.paper)}>{value}</Text>
            <Check x={24} y={35} p={rise(frame, at + 4, 8)} />
          </Group>
        );
      })}
    </Group>
  );
}
