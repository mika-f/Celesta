import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import { defineShader } from '../dist/index.js';

const cli = fileURLToPath(new URL('../../cli/bin/celesta-react-render.js', import.meta.url));

const WGSL = 'fn effect(input: EffectInput) -> vec4f { return source_at(input.position); }';

/**
 * Renders frame 0 of a composition: `setup` runs at module level, `body` is
 * the composition's children. The frame, or `{ error }` when it failed.
 */
function render(setup, body) {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-shader-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition, Group, Rect } from '@celesta/react';\n` +
      `import { defineShader } from '@celesta/shader';\n` +
      `const wgsl = ${JSON.stringify(WGSL)};\n` +
      `${setup}\n` +
      `export default function Root() {\n` +
      `  return <Composition width={64} height={64} fps={30} durationInFrames={1}>${body}</Composition>;\n` +
      `}\n`,
  );
  try {
    const result = spawnSync(process.execPath, [cli, entry], {
      input: '{"time":{"value":0,"timescale":30}}\n',
      encoding: 'utf8',
    });
    if (result.status !== 0) {
      // Errors arrive as `{"error": …}` lines on stdout, or on stderr.
      const lines = result.stdout.split(/\r?\n/).filter(Boolean).map((line) => {
        try {
          return JSON.parse(line).error ?? line;
        } catch {
          return line;
        }
      });
      return { error: [...lines, result.stderr].join('\n') };
    }
    const [, frame] = result.stdout.split(/\r?\n/).filter(Boolean).map(JSON.parse);
    return frame;
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test('values pack in declared order, with defaults, colors, and padding', () => {
  const frame = render(
    `const grade = defineShader({ name: 'grade', wgsl, padding: 4, params: {
       amount: 'f32',
       offset: { type: 'vec2', default: [1, 2] },
       tint: 'color',
       fade: { type: 'color', default: '#00000080' },
       axis: 'vec3',
       weights: 'vec4',
     } });
     export const id = grade.id;`,
    `<Rect width={8} height={8} shader={grade({ amount: 0.5, tint: '#ff8000', axis: [1, 0, 0], weights: [0.1, 0.2, 0.3, 0.4] })} />
     <Rect width={8} height={8} shader={grade({ amount: 1, offset: [3, 4], tint: '#00ff0080', axis: [0, 1, 0], weights: [1, 1, 1, 1] }, { padding: 0 })} />`,
  );
  assert.equal(frame.error, undefined, frame.error);
  const [first, second] = frame.scene.layers.map((layer) => layer.effects.shader);
  const close = (actual, expected) => {
    assert.equal(actual.length, expected.length);
    actual.forEach((value, index) => assert.ok(Math.abs(value - expected[index]) < 1e-9, `${actual} vs ${expected}`));
  };
  close(first.params, [0.5, 1, 2, 1, 128 / 255, 0, 1, 0, 0, 0, 128 / 255, 1, 0, 0, 0.1, 0.2, 0.3, 0.4]);
  assert.equal(first.padding, 4);
  close(second.params, [1, 3, 4, 0, 1, 0, 128 / 255, 0, 0, 0, 128 / 255, 0, 1, 0, 1, 1, 1, 1]);
  assert.equal('padding' in second, false);
  assert.equal(first.id, second.id);
  assert.deepEqual(frame.scene.shaders, [{
    id: first.id,
    name: 'grade',
    wgsl: WGSL,
    params: [
      { name: 'amount', type: 'f32' },
      { name: 'offset', type: 'vec2' },
      { name: 'tint', type: 'vec4' },
      { name: 'fade', type: 'vec4' },
      { name: 'axis', type: 'vec3' },
      { name: 'weights', type: 'vec4' },
    ],
  }]);
});

test('a shader without parameters takes no arguments', () => {
  const frame = render(
    `const plain = defineShader({ wgsl });`,
    `<Group shader={plain()}><Rect width={8} height={8} /></Group>`,
  );
  assert.equal(frame.error, undefined, frame.error);
  assert.deepEqual(frame.scene.layers[0].effects.shader, { id: frame.scene.shaders[0].id });
});

test('shader.id is the id the scene carries', () => {
  const frame = render(
    `const plain = defineShader({ name: 'plain', wgsl, params: { amount: 'f32' } });`,
    `<Rect id={plain.id} width={8} height={8} shader={plain({ amount: 1 })} />`,
  );
  assert.equal(frame.error, undefined, frame.error);
  assert.equal(frame.scene.layers[0].id, frame.scene.shaders[0].id);
});

// In-process: each error would otherwise cost a CLI start, and the many
// together outlast a slow runner's test timeout.
test('definitions and values are checked with messages naming the shader', () => {
  const cases = [
    [() => defineShader({ wgsl: '' }), /shader: wgsl must be a non-empty string/],
    [() => defineShader({ name: 'x', wgsl: WGSL, padding: -1 }), /shader x: padding must be a finite number of at least 0/],
    [() => defineShader({ name: 'x', wgsl: WGSL, params: { 'bad-name': 'f32' } }), /shader x: parameter name "bad-name"/],
    [() => defineShader({ name: 'x', wgsl: WGSL, params: { celestaTime: 'f32' } }), /shader x: parameter name "celestaTime"/],
    [() => defineShader({ name: 'x', wgsl: WGSL, params: { a: 'mat4' } }), /shader x: a must have a type of f32, vec2, vec3, vec4, or color/],
    [() => defineShader({ name: 'x', wgsl: WGSL, params: { fn: 'f32' } }), /shader x: parameter name "fn" is a WGSL keyword or reserved word/],
    [() => defineShader({ name: 'x', wgsl: WGSL, params: { self: 'f32' } }), /shader x: parameter name "self" is a WGSL keyword or reserved word/],
    [() => defineShader({ name: 'x', wgsl: WGSL, params: { a: { type: 'f32', default: undefined } } }), /shader x: the default of a must be a finite number/],
    [() => defineShader({ name: 'x', wgsl: WGSL, params: { a: { type: 'color', default: 'red' } } }), /shader x: the default of a must be a #RRGGBB or #RRGGBBAA color/],
    // eslint-disable-next-line no-sparse-arrays -- the hole is the point.
    [() => defineShader({ name: 'x', wgsl: WGSL, params: { axis: { type: 'vec2', default: [1, , ] } } }), /shader x: the default of axis must be an array of 2 finite numbers/],
    [
      () => defineShader({ name: 'x', wgsl: WGSL, params: Object.fromEntries(Array.from({ length: 17 }, (_, i) => [`p${i}`, 'f32'])) }),
      /shader x: declares 17 parameters; the most is 16/,
    ],
  ];
  for (const [define, error] of cases) {
    assert.throws(define, error, String(define));
  }
  const ripple = defineShader({
    name: 'ripple',
    wgsl: WGSL,
    params: { amount: 'f32', axis: 'vec2', tint: { type: 'color', default: '#ffffff' } },
  });
  const uses = [
    [{ amount: Number.NaN, axis: [0, 0] }, /shader ripple: amount must be a finite number/],
    [{ axis: [0, 0] }, /shader ripple: amount is required/],
    [{ amount: 1, axis: [0] }, /shader ripple: axis must be an array of 2 finite numbers/],
    [{ amount: 1, axis: [0, 0], tint: '#ff00' }, /shader ripple: tint must be a #RRGGBB or #RRGGBBAA color/],
    [{ amount: 1, axis: [0, 0], speed: 1 }, /shader ripple: has no parameter "speed"/],
    // eslint-disable-next-line no-sparse-arrays -- the hole is the point.
    [{ amount: 1, axis: [0, , ] }, /shader ripple: axis must be an array of 2 finite numbers/],
  ];
  for (const [values, error] of uses) {
    assert.throws(() => ripple(values), error, JSON.stringify(values));
  }
});

test('an error in a definition reaches the CLI with its message', () => {
  const frame = render(`defineShader({ name: 'x', wgsl, padding: -1 });`, `<Rect width={8} height={8} />`);
  assert.match(frame.error ?? '', /shader x: padding must be a finite number of at least 0/);
});

test('a parameter named like an Object method takes its default when omitted', () => {
  const frame = render(
    `const odd = defineShader({ name: 'odd', wgsl, params: { toString: { type: 'f32', default: 3 } } });`,
    `<Rect width={8} height={8} shader={odd()} />`,
  );
  assert.equal(frame.error, undefined, frame.error);
  assert.deepEqual(frame.scene.layers[0].effects.shader.params, [3]);
});
