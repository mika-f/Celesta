import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import type { Layer, LayerFilter, Scene, ShaderSource } from '@celesta/react';
import { defineShader } from '../src/index.js';
import type { ShaderParamSpec } from '../src/index.js';

// The tests that render are integration tests of the build: they go through the
// CLI, which loads @celesta/shader from its dist/, so `pnpm run build:runtime`
// (repository root) must run first. The in-process test below imports the
// source.
const cli = fileURLToPath(new URL('../../cli/bin/celesta-react-render.js', import.meta.url));

const WGSL = 'fn effect(input: EffectInput) -> vec4f { return source_at(input.position); }';

/** A frame the CLI rendered, or the error it failed with. */
type RenderResult = { scene: Scene; error?: undefined } | { scene?: undefined; error: string };

type ShaderFilter = Extract<LayerFilter, { type: 'shader' }>;

/** The scene of a frame that rendered. */
function sceneOf(frame: RenderResult): Scene {
  assert.equal(frame.error, undefined, 'a frame without an error');
  assert.ok(frame.scene);
  return frame.scene;
}

/** `layer`'s filters, checked to be there. */
function filtersOf(layer: Layer | undefined): LayerFilter[] {
  assert.ok(layer?.effects?.filters, 'a layer with filters');
  return layer.effects.filters;
}

/** `filter`, checked to be a custom shader. */
function shaderOf(filter: LayerFilter | undefined): ShaderFilter {
  assert.equal(filter?.type, 'shader');
  return filter as ShaderFilter;
}

/** The custom shaders `scene` lists, checked to be there. */
function shadersOf(scene: Scene): ShaderSource[] {
  assert.ok(scene.shaders, 'a scene listing shaders');
  return scene.shaders;
}

/**
 * Renders frame 0 of a composition: `setup` runs at module level, `body` is
 * the composition's children. The frame, or `{ error }` when it failed.
 */
function render(setup: string, body: string): RenderResult {
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
      const lines = result.stdout.split(/\r?\n/).filter(Boolean).map((line): string => {
        try {
          return (JSON.parse(line) as { error?: string }).error ?? line;
        } catch {
          return line;
        }
      });
      return { error: [...lines, result.stderr].join('\n') };
    }
    const [, frame] = result.stdout.split(/\r?\n/).filter(Boolean).map((line): RenderResult => JSON.parse(line));
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
  const scene = sceneOf(frame);
  const [first, second] = scene.layers.map((layer) => shaderOf(filtersOf(layer)[0]));
  const close = (actual: readonly number[] | undefined, expected: readonly number[]) => {
    assert.ok(actual);
    assert.equal(actual.length, expected.length);
    actual.forEach((value, index) => assert.ok(Math.abs(value - expected[index]) < 1e-9, `${actual} vs ${expected}`));
  };
  close(first.params, [0.5, 1, 2, 1, 128 / 255, 0, 1, 0, 0, 0, 128 / 255, 1, 0, 0, 0.1, 0.2, 0.3, 0.4]);
  assert.equal(first.padding, 4);
  close(second.params, [1, 3, 4, 0, 1, 0, 128 / 255, 0, 0, 0, 128 / 255, 0, 1, 0, 1, 1, 1, 1]);
  assert.equal('padding' in second, false);
  assert.equal(first.id, second.id);
  assert.deepEqual(scene.shaders, [{
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
  const scene = sceneOf(frame);
  assert.deepEqual(filtersOf(scene.layers[0]), [{ type: 'shader', id: shadersOf(scene)[0].id }]);
});

test('a layer takes several shaders, run in the order given', () => {
  const frame = render(
    `const plain = defineShader({ name: 'plain', wgsl });
     const ripple = defineShader({ name: 'ripple', wgsl: wgsl + ' ', params: { time: 'f32' }, padding: 6 });`,
    `<Rect width={8} height={8} shader={[ripple({ time: 1 }), plain(), ripple({ time: 2 }, { padding: 0 })]} />`,
  );
  const scene = sceneOf(frame);
  const [ripple, plain] = shadersOf(scene);
  assert.deepEqual(shadersOf(scene).map((source) => source.name), ['ripple', 'plain']);
  assert.deepEqual(filtersOf(scene.layers[0]), [
    { type: 'shader', id: ripple.id, params: [1], padding: 6 },
    { type: 'shader', id: plain.id },
    { type: 'shader', id: ripple.id, params: [2] },
  ]);
});

test('the built-in color filters run where the array lists them', () => {
  const frame = render(
    `import { brightness, contrast, grayscale, hue, invert, saturation, tint, vignette } from '@celesta/shader';
     const plain = defineShader({ name: 'plain', wgsl });`,
    `<Rect width={8} height={8} shader={[
       brightness(1.2), plain(), contrast(0.5), saturation(0), hue(-30), grayscale(), invert(0.5),
       tint({ color: '#ff8000' }), vignette(0.4), vignette({ amount: 1, radius: 0.2, softness: 0 }),
     ]} />`,
  );
  const scene = sceneOf(frame);
  // Only custom shaders are listed in the scene.
  assert.deepEqual(shadersOf(scene).map((source) => source.name), ['plain']);
  assert.deepEqual(filtersOf(scene.layers[0]), [
    { type: 'brightness', amount: 1.2 },
    { type: 'shader', id: shadersOf(scene)[0].id },
    { type: 'contrast', amount: 0.5 },
    { type: 'saturation', amount: 0 },
    { type: 'hue', degrees: -30 },
    { type: 'grayscale', amount: 1 },
    { type: 'invert', amount: 0.5 },
    { type: 'tint', color: '#ff8000', amount: 1 },
    { type: 'vignette', amount: 0.4, radius: 0.5, softness: 0.5 },
    { type: 'vignette', amount: 1, radius: 0.2, softness: 0 },
  ]);
});

test('the stylize effects and the chroma key run where the array lists them', () => {
  const frame = render(
    `import { chromaKey, chromaticAberration, halftone, noise, pixelate, scanlines, wave, zoomBlur } from '@celesta/shader';`,
    `<Rect width={8} height={8} shader={[
       scanlines(0.4), pixelate(6), halftone({ size: 8, angle: 15 }), chromaticAberration(2),
       wave({ amplitude: 3, wavelength: 40, phase: 90 }), noise(0.1), noise({ amount: 0.1, seed: 5, monochrome: false }),
       zoomBlur({ amount: 0.2, centerY: 0 }), chromaKey({ color: '#0000ff', spill: 0 }),
     ]} />`,
  );
  const scene = sceneOf(frame);
  assert.equal('shaders' in scene, false);
  assert.deepEqual(filtersOf(scene.layers[0]), [
    { type: 'scanlines', amount: 0.4, spacing: 4, thickness: 0.5, offset: 0 },
    { type: 'pixelate', size: 6 },
    { type: 'halftone', size: 8, angle: 15 },
    { type: 'chromaticAberration', offset: 2, angle: 0 },
    { type: 'wave', amplitude: 3, wavelength: 40, phase: 90, angle: 0 },
    // Frame 0's seed.
    { type: 'noise', amount: 0.1, seed: 0, monochrome: true },
    { type: 'noise', amount: 0.1, seed: 5, monochrome: false },
    { type: 'zoomBlur', amount: 0.2, centerX: 0.5, centerY: 0 },
    { type: 'chromaKey', color: '#0000ff', tolerance: 0.2, softness: 0.1, spill: 0 },
  ]);
});

test('color filters without custom shaders list no shaders in the scene', () => {
  const frame = render(
    `import { invert } from '@celesta/shader';`,
    `<Rect width={8} height={8} shader={invert()} />`,
  );
  const scene = sceneOf(frame);
  assert.equal('shaders' in scene, false);
  assert.deepEqual(filtersOf(scene.layers[0]), [{ type: 'invert', amount: 1 }]);
});

test('shader.id is the id the scene carries', () => {
  const frame = render(
    `const plain = defineShader({ name: 'plain', wgsl, params: { amount: 'f32' } });`,
    `<Rect id={plain.id} width={8} height={8} shader={plain({ amount: 1 })} />`,
  );
  const scene = sceneOf(frame);
  assert.equal(scene.layers[0].id, shadersOf(scene)[0].id);
});

// In-process: each error would otherwise cost a CLI start, and the many
// together outlast a slow runner's test timeout.
test('definitions and values are checked with messages naming the shader', () => {
  // Deliberately invalid parameters, typed as valid ones so they reach the checks.
  const invalid = (params: Record<string, unknown>) => params as Record<string, ShaderParamSpec>;
  const cases: [() => unknown, RegExp][] = [
    [() => defineShader({ wgsl: '' }), /shader: wgsl must be a non-empty string/],
    [() => defineShader({ name: 'x', wgsl: WGSL, padding: -1 }), /shader x: padding must be a finite number of at least 0/],
    [() => defineShader({ name: 'x', wgsl: WGSL, params: { 'bad-name': 'f32' } }), /shader x: parameter name "bad-name"/],
    [() => defineShader({ name: 'x', wgsl: WGSL, params: { celestaTime: 'f32' } }), /shader x: parameter name "celestaTime"/],
    [() => defineShader({ name: 'x', wgsl: WGSL, params: invalid({ a: 'mat4' }) }), /shader x: a must have a type of f32, vec2, vec3, vec4, or color/],
    [() => defineShader({ name: 'x', wgsl: WGSL, params: { fn: 'f32' } }), /shader x: parameter name "fn" is a WGSL keyword or reserved word/],
    [() => defineShader({ name: 'x', wgsl: WGSL, params: { self: 'f32' } }), /shader x: parameter name "self" is a WGSL keyword or reserved word/],
    [() => defineShader({ name: 'x', wgsl: WGSL, params: { a: { type: 'f32', default: undefined } } }), /shader x: the default of a must be a finite number/],
    [() => defineShader({ name: 'x', wgsl: WGSL, params: { a: { type: 'color', default: 'red' } } }), /shader x: the default of a must be a #RRGGBB or #RRGGBBAA color/],
    // eslint-disable-next-line no-sparse-arrays -- the hole is the point.
    [() => defineShader({ name: 'x', wgsl: WGSL, params: invalid({ axis: { type: 'vec2', default: [1, , ] } }) }), /shader x: the default of axis must be an array of 2 finite numbers/],
    [
      () => defineShader({ name: 'x', wgsl: WGSL, params: Object.fromEntries(Array.from({ length: 17 }, (_, i): [string, ShaderParamSpec] => [`p${i}`, 'f32'])) }),
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
  // Deliberately invalid values, so they are not typed as ripple's; cast where
  // they are passed.
  const uses: [Record<string, unknown>, RegExp][] = [
    [{ amount: Number.NaN, axis: [0, 0] }, /shader ripple: amount must be a finite number/],
    [{ axis: [0, 0] }, /shader ripple: amount is required/],
    [{ amount: 1, axis: [0] }, /shader ripple: axis must be an array of 2 finite numbers/],
    [{ amount: 1, axis: [0, 0], tint: '#ff00' }, /shader ripple: tint must be a #RRGGBB or #RRGGBBAA color/],
    [{ amount: 1, axis: [0, 0], speed: 1 }, /shader ripple: has no parameter "speed"/],
    // eslint-disable-next-line no-sparse-arrays -- the hole is the point.
    [{ amount: 1, axis: [0, , ] }, /shader ripple: axis must be an array of 2 finite numbers/],
  ];
  for (const [values, error] of uses) {
    assert.throws(() => ripple(values as Parameters<typeof ripple>[0]), error, JSON.stringify(values));
  }
});

test('errors in a definition and in a use reach the CLI with their messages', () => {
  // A definition fails as the entry loads; a use fails while a frame renders.
  const definition = render(`defineShader({ name: 'x', wgsl, padding: -1 });`, `<Rect width={8} height={8} />`);
  assert.match(definition.error ?? '', /shader x: padding must be a finite number of at least 0/);
  const use = render(
    `const ripple = defineShader({ name: 'ripple', wgsl, params: { amount: 'f32' } });`,
    `<Rect width={8} height={8} shader={ripple({ amount: Number.NaN })} />`,
  );
  assert.match(use.error ?? '', /shader ripple: amount must be a finite number/);
});

test('a parameter named like an Object method takes its default when omitted', () => {
  const frame = render(
    `const odd = defineShader({ name: 'odd', wgsl, params: { toString: { type: 'f32', default: 3 } } });`,
    `<Rect width={8} height={8} shader={odd()} />`,
  );
  const scene = sceneOf(frame);
  assert.deepEqual(shaderOf(filtersOf(scene.layers[0])[0]).params, [3]);
});
