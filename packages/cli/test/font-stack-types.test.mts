// The staged project types accept font stacks wherever a family goes,
// including read-only lists declared `as const`. Run after
// `pnpm run build:runtime`: the test checks the build's dist/project-types.

import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';
import { test } from 'vitest';

const projectTypes = fileURLToPath(new URL('../dist/project-types/tsconfig.json', import.meta.url));

const ENTRY = `import { Composition, Span, Text, useTextMetrics } from '@celesta.studio/react';
import type { FontFamily, TextStyle } from '@celesta.studio/react';
import { Code } from '@celesta.studio/code';
import { TextBox } from '@celesta.studio/text';

const SANS = ['Inter', 'Noto Sans JP'] as const;
const MONO: FontFamily = ['JetBrains Mono', 'Noto Sans JP'];
const style: TextStyle = { fontFamily: SANS, fontSize: 48 };
// @ts-expect-error a family is a name or a list of names.
const wrong: TextStyle = { fontFamily: [1, 2] };

export default function Root() {
  const metrics = useTextMetrics('和欧', { fontFamily: SANS });
  return (
    <Composition width={64} height={64} fps={30} durationInFrames={1}>
      <Text style={style}>Celesta <Span style={{ fontFamily: SANS, fontWeight: 700 }}>で</Span></Text>
      <Text style={{ fontFamily: 'Inter' }}>{metrics.width}</Text>
      <TextBox width={60} height={20} minFontSize={8} maxFontSize={24} style={{ fontFamily: SANS }}>和欧混植</TextBox>
      <Code style={{ fontFamily: MONO }}>{'// 和文'}</Code>
    </Composition>
  );
}
`;

test('the staged project types take font stacks, read-only ones included', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-font-stack-'));
  writeFileSync(join(dir, 'entry.tsx'), ENTRY);
  writeFileSync(join(dir, 'tsconfig.json'), JSON.stringify({ extends: projectTypes, include: ['entry.tsx'] }));
  try {
    const config = ts.getParsedCommandLineOfConfigFile(join(dir, 'tsconfig.json'), {}, {
      ...ts.sys,
      onUnRecoverableConfigFileDiagnostic: (diagnostic) => assert.fail(ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n')),
    });
    assert.ok(config);
    const program = ts.createProgram(config.fileNames, config.options);
    const diagnostics = ts.getPreEmitDiagnostics(program).map((diagnostic) =>
      ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n'));
    assert.deepEqual(diagnostics, []);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
