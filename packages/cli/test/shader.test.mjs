// `@celesta/shader` through the CLI: `.wgsl` files bundle as text, and the
// staged project types make them, and the shader API, type-check. Run after
// `pnpm run build:runtime`.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';
import { test } from 'vitest';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));
const projectTypes = fileURLToPath(new URL('../dist/project-types/tsconfig.json', import.meta.url));

const RIPPLE = `fn effect(input: EffectInput) -> vec4f {
  let wave = sin(input.position.x * 0.1 + params.time) * params.amplitude;
  return source_at(input.position + vec2f(0.0, wave));
}
`;

const ENTRY = `import { Composition, Rect, useCurrentFrame } from '@celesta/react';
import { defineShader } from '@celesta/shader';
import rippleSource from './ripple.wgsl';

const ripple = defineShader({
  name: 'ripple',
  wgsl: rippleSource,
  params: { time: 'f32', amplitude: { type: 'f32', default: 4 }, tint: 'color' },
  padding: 4,
});

export default function Root() {
  const frame = useCurrentFrame();
  return (
    <Composition width={64} height={64} fps={30} durationInFrames={1}>
      <Rect width={32} height={32} fill="#ffffff" shader={ripple({ time: frame / 30, tint: '#ff8000' })} />
    </Composition>
  );
}
`;

function project(files) {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-shader-'));
  for (const [name, text] of Object.entries(files)) {
    writeFileSync(join(dir, name), text);
  }
  return dir;
}

test('a composition imports a .wgsl file as the shader source', () => {
  const dir = project({ 'entry.tsx': ENTRY, 'ripple.wgsl': RIPPLE });
  try {
    const result = spawnSync(process.execPath, [cli, join(dir, 'entry.tsx')], {
      input: '{"time":{"value":0,"timescale":30}}\n',
      encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const [, frame] = result.stdout.split(/\r?\n/).filter(Boolean).map(JSON.parse);
    assert.equal(frame.scene.shaders.length, 1);
    assert.equal(frame.scene.shaders[0].wgsl, RIPPLE);
    assert.equal(frame.scene.layers[0].effects.shader.id, frame.scene.shaders[0].id);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('the staged project types check .wgsl imports and shader values', () => {
  const dir = project({
    'entry.tsx': `${ENTRY}
const typed = defineShader({ wgsl: rippleSource, params: { time: 'f32', axis: 'vec2' } });
typed({ time: 1, axis: [0, 1] });
// @ts-expect-error a required parameter is missing.
typed({ time: 1 });
// @ts-expect-error a vec2 takes two numbers.
typed({ time: 1, axis: [0] });
// @ts-expect-error there is no such parameter.
typed({ time: 1, axis: [0, 1], speed: 2 });
defineShader({ wgsl: rippleSource })();
defineShader({ wgsl: rippleSource, params: { tint: { type: 'color', default: '#ffffff' } } })();
// @ts-expect-error a default must match its parameter's type.
defineShader({ wgsl: rippleSource, params: { amount: { type: 'f32', default: '#ffffff' } } });
`,
    'ripple.wgsl': RIPPLE,
    'tsconfig.json': JSON.stringify({ extends: projectTypes, include: ['entry.tsx'] }),
  });
  try {
    const config = ts.getParsedCommandLineOfConfigFile(join(dir, 'tsconfig.json'), {}, {
      ...ts.sys,
      onUnRecoverableConfigFileDiagnostic: (diagnostic) => assert.fail(ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n')),
    });
    const program = ts.createProgram(config.fileNames, config.options);
    const diagnostics = ts.getPreEmitDiagnostics(program).map((diagnostic) =>
      ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n'));
    assert.deepEqual(diagnostics, []);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
