import { Group, Rect, Text } from '@celesta.studio/react';
import { highlightCode } from '@celesta-plugin/code';
import { codeTheme } from '../../components/codeTheme';
import { C } from '../../components/theme';
import type { DemoProps } from '../program';
import { Check, mono, rise } from './common';

// LOCK: this project's own files. examples/delta declares the registry
// plugins its films import (code, lottie) in celesta.config.json, and
// celesta.lock pins each version and its hashes; Celesta-export runs the
// locked versions. Copied from the two files: change them together.

const CONFIG = `{ "plugins": { "dependencies": {
    "code": "0.10.0-alpha.3",
    "lottie": "0.10.0-alpha.3" } } }`;

const LOCK = `"code": {
  "celesta": ">=0.10.0-alpha.4, <0.11.0",
  "registry": "https://registry.celesta.studio",
  "targets": { "any": {
    "sha256": "sha256:7196aebb71133d4b…",
    "treeHash": "sha256:ee3e8448ba5cd393…" } },
  "version": "0.10.0-alpha.4"
},`;

const CODE = { fontFamily: ['JetBrains Mono', 'Noto Sans JP'], fontSize: 18, lineHeight: 30 } as const;

function Listing({ x, y, width, file, code, lit, f }: { x: number; y: number; width: number; file: string; code: string; lit: number; f: number }) {
  const lines = highlightCode(code, { language: 'json', theme: codeTheme });
  return (
    <Group x={x} y={y}>
      <Rect width={width} height={44 + lines.length * CODE.lineHeight + 12} fill={C.ink} stroke={C.line} />
      <Text x={16} y={22} anchorY={0.5} style={mono(16, C.grey)}>{file}</Text>
      {lines.map((line, i) => (
        <Group key={i} y={50 + i * CODE.lineHeight} opacity={rise(f, lit + i * 2, 6)}>
          <Text x={16} style={{ ...CODE, fill: { type: 'solid', color: codeTheme.foreground }, colorRuns: line.runs }}>{line.text || ' '}</Text>
        </Group>
      ))}
    </Group>
  );
}

export function LockDemo({ frame }: DemoProps) {
  return (
    <Group>
      <Listing x={24} y={24} width={952} file="celesta.config.json" code={CONFIG} lit={2} f={frame} />
      <Listing x={24} y={176} width={600} file="celesta.lock" code={LOCK} lit={14} f={frame} />
      <Group x={650} y={176} opacity={rise(frame, 52, 10)}>
        <Text style={mono(16, C.dim)}>Celesta-export reads the lock</Text>
        {[['code', '0.10.0-alpha.4'], ['lottie', '0.10.0-alpha.4']].map(([id, version], i) => (
          <Group key={id} y={44 + i * 64}>
            <Rect width={326} height={52} fill="#3DDC8414" stroke={C.green} />
            <Text x={16} y={26} anchorY={0.5} style={mono(19, C.paper, 700)}>{id}</Text>
            <Text x={120} y={26} anchorY={0.5} style={mono(16, C.grey)}>{version}</Text>
            <Check x={300} y={26} p={rise(frame, 60 + i * 8, 8)} />
          </Group>
        ))}
        <Text y={200} maxWidth={320} style={mono(15, C.grey)}>targets: linux-x86_64,</Text>
        <Text y={224} maxWidth={320} style={mono(15, C.grey)}>macos-aarch64, windows-x86_64</Text>
        <Text y={270} maxWidth={320} style={mono(15, C.dim)}>committed, never edited by hand</Text>
      </Group>
    </Group>
  );
}
