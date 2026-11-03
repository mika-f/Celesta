// The core side of custom shaders: what the walker accepts and how it
// serializes it. `@celesta.studio/shader`'s own API is tested in its package.

import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';
import type { ReactNode } from 'react';

import { Composition, Group, Rect } from '../src/index.js';
import { createResolver, mount } from '../src/render.js';
import { shaderEffect, shaderSource } from '../src/internal.js';
import { ShaderSourceHandle } from '../src/shader.js';
import { registerComponent } from '../src/registry.js';
import type { ShaderEffect, Time } from '../src/index.js';

import { childrenOf } from './helpers.mjs';

const h = React.createElement;
const ZERO: Time = { value: 0, timescale: 30 };
const WGSL = 'fn effect(input: EffectInput) -> vec4f {\n  return source_at(input.position) * params.amount;\n}\n';

function frame(children: ReactNode) {
  const Root = () => h(Composition, { width: 16, height: 16, fps: 30, durationInFrames: 1 }, children);
  return mount(Root).renderAt(ZERO, null).scene;
}

test('a layer serializes its shader and the scene lists each source once', () => {
  const tint = shaderSource({ name: 'tint', wgsl: WGSL, params: [{ name: 'amount', type: 'vec2' }] });
  const scene = frame([
    h(Rect, { key: 'a', id: 'a', width: 4, height: 4, fill: '#ffffff', shader: shaderEffect(tint, [0.5, 1], 6) }),
    h(Group, { key: 'b', id: 'b', shader: shaderEffect(tint, [1, 0], 0) },
      h(Rect, { width: 4, height: 4, fill: '#ffffff', shader: shaderEffect(tint, [0, 0], 0) })),
  ]);
  assert.deepEqual(scene.layers[0].effects, { blur: 0, filters: [{ type: 'shader', id: tint.id, params: [0.5, 1], padding: 6 }] });
  assert.deepEqual(scene.layers[1].effects, { blur: 0, filters: [{ type: 'shader', id: tint.id, params: [1, 0] }] });
  assert.deepEqual(childrenOf(scene.layers[1])[0].effects?.filters, [{ type: 'shader', id: tint.id, params: [0, 0] }]);
  assert.deepEqual(scene.shaders, [{
    id: tint.id,
    name: 'tint',
    wgsl: WGSL,
    params: [{ name: 'amount', type: 'vec2' }],
  }]);
});

test('a shader without parameters or padding serializes only its id', () => {
  const plain = shaderSource({ wgsl: WGSL, params: [] });
  const scene = frame(h(Rect, { width: 4, height: 4, shader: shaderEffect(plain, [], 0) }));
  assert.deepEqual(scene.layers[0].effects?.filters, [{ type: 'shader', id: plain.id }]);
  assert.deepEqual(scene.shaders, [{ id: plain.id, wgsl: WGSL }]);
});

test('a frame without shaders has no shaders field', () => {
  const scene = frame(h(Rect, { width: 4, height: 4, blur: 2 }));
  assert.equal('shaders' in scene, false);
  assert.ok(scene.layers[0].effects);
  assert.equal('filters' in scene.layers[0].effects, false);
});

test('the id follows the source and the parameters, and nothing else', () => {
  const id = (definition: Parameters<typeof shaderSource>[0]) => shaderSource(definition).id;
  const base: Parameters<typeof shaderSource>[0] = { wgsl: WGSL, params: [{ name: 'amount', type: 'f32' }] };
  assert.match(id(base), /^[0-9a-f]{16}$/);
  assert.equal(id(base), id({ ...base }));
  assert.equal(id(base), id({ ...base, name: 'renamed' }));
  assert.notEqual(id(base), id({ ...base, wgsl: `${WGSL} ` }));
  assert.notEqual(id(base), id({ ...base, params: [{ name: 'other', type: 'f32' }] }));
  assert.notEqual(id(base), id({ ...base, params: [{ name: 'amount', type: 'vec2' }] }));
  // Pinned, so ids stay stable across runs and releases.
  assert.equal(id({ wgsl: 'x', params: [] }), shaderSource({ wgsl: 'x', params: [] }).id);
  assert.equal(id({ wgsl: 'x', params: [] }), '1b0a62b2133ed451');
});

test('an array of shaders serializes them in order, listing each source once', () => {
  const tint = shaderSource({ name: 'tint', wgsl: WGSL, params: [{ name: 'amount', type: 'f32' }] });
  const plain = shaderSource({ wgsl: WGSL, params: [] });
  const scene = frame(h(Rect, {
    width: 4,
    height: 4,
    shader: [shaderEffect(plain, [], 0), shaderEffect(tint, [0.5], 4), shaderEffect(plain, [], 2)],
  }));
  assert.deepEqual(scene.layers[0].effects?.filters, [
    { type: 'shader', id: plain.id },
    { type: 'shader', id: tint.id, params: [0.5], padding: 4 },
    { type: 'shader', id: plain.id, padding: 2 },
  ]);
  assert.deepEqual(scene.shaders?.map((source) => source.id), [plain.id, tint.id]);
});

test('an empty array of shaders adds no effect', () => {
  const scene = frame(h(Rect, { width: 4, height: 4, shader: [] }));
  assert.equal('effects' in scene.layers[0], false);
  assert.equal('shaders' in scene, false);
});

test('the shader prop only takes a ShaderEffect or an array of them', () => {
  const plain = shaderSource({ wgsl: WGSL, params: [] });
  const lookalike = { id: plain.id };
  const effect = shaderEffect(plain, [], 0);
  // Each is invalid on purpose, so they are cast.
  // eslint-disable-next-line no-sparse-arrays -- the hole is the point.
  const shaders = [lookalike, 'ripple', null, [effect, lookalike], [effect, null], [, effect]] as unknown as ShaderEffect[];
  for (const shader of shaders) {
    assert.throws(
      () => frame(h(Rect, { width: 4, height: 4, shader })),
      /shader must be a value returned by @celesta\.studio\/shader \(defineShader\(\) or a built-in filter such as brightness\(\)\), or an array of them/,
    );
  }
});

test('shaderSource and shaderEffect check their invariants', () => {
  assert.throws(() => shaderSource({ wgsl: '', params: [] }), /wgsl must be a non-empty string/);
  // Invalid on purpose.
  assert.throws(() => shaderSource({ wgsl: WGSL, params: [{ name: 'a', type: 'color' as 'f32' }] }), /f32, vec2, vec3, or vec4/);
  assert.throws(
    () => shaderSource({ wgsl: WGSL, params: Array.from({ length: 17 }, (_, i) => ({ name: `p${i}`, type: 'f32' })) }),
    /at most 16/,
  );
  const source = shaderSource({ wgsl: WGSL, params: [{ name: 'amount', type: 'vec3' }] });
  // Invalid on purpose: a lookalike, not a ShaderSourceHandle.
  assert.throws(() => shaderEffect({ id: source.id } as ShaderSourceHandle, [1, 2, 3], 0), /must come from shaderSource/);
  assert.throws(() => shaderEffect(source, [1, 2], 0), /expected 3 parameter values/);
  assert.throws(() => shaderEffect(source, [1, 2, Number.NaN], 0), /finite numbers/);
  // eslint-disable-next-line no-sparse-arrays -- the hole is the point.
  assert.throws(() => shaderEffect(source, [1, , 3] as number[], 0), /finite numbers/);
  assert.throws(() => shaderEffect(source, [1, 2, 3], -1), /padding/);
});

test('component resolutions list the shaders their layers use, each once', () => {
  const tint = shaderSource({ name: 'tint', wgsl: WGSL, params: [{ name: 'amount', type: 'f32' }] });
  const plain = shaderSource({ wgsl: WGSL, params: [] });
  registerComponent<{ amount: number }>('TintedForTimeline', ({ amount }) =>
    h(Rect, { width: 4, height: 4, shader: shaderEffect(tint, [amount], 0) }));
  registerComponent('PlainForTimeline', () =>
    h(Group, { shader: shaderEffect(plain, [], 0) }, h(Rect, { width: 4, height: 4, shader: shaderEffect(tint, [1], 0) })));
  const { components, shaders } = createResolver().resolve([
    { component: 'TintedForTimeline', props: { amount: 0.5 } },
    { component: 'NotRegisteredForTimeline', props: {} },
    { component: 'PlainForTimeline', props: {} },
    { component: 'TintedForTimeline', props: { amount: 1 } },
  ]);
  assert.equal(components[1], null);
  assert.deepEqual(components[0]?.[0].effects?.filters, [{ type: 'shader', id: tint.id, params: [0.5] }]);
  assert.deepEqual(components[2]?.[0].effects?.filters, [{ type: 'shader', id: plain.id }]);
  assert.deepEqual(components[3]?.[0].effects?.filters, [{ type: 'shader', id: tint.id, params: [1] }]);
  assert.deepEqual(shaders, [
    { id: tint.id, name: 'tint', wgsl: WGSL, params: [{ name: 'amount', type: 'f32' }] },
    { id: plain.id, wgsl: WGSL },
  ]);
});

test('component resolutions without shaders have no shaders field', () => {
  registerComponent('UnshadedForTimeline', () => h(Rect, { width: 4, height: 4 }));
  const resolutions = createResolver().resolve([{ component: 'UnshadedForTimeline', props: {} }]);
  assert.equal(resolutions.components.length, 1);
  assert.equal('shaders' in resolutions, false);
});

test('two different shaders with the same id in one frame are refused', () => {
  // Real ids are hashes; colliding ones are built directly.
  const first = new ShaderSourceHandle({ id: 'collide', wgsl: WGSL }, 0);
  const second = new ShaderSourceHandle({ id: 'collide', wgsl: `${WGSL}\n` }, 0);
  const same = new ShaderSourceHandle({ id: 'collide', wgsl: WGSL }, 0);
  const rect = (key: string, source: ShaderSourceHandle) => h(Rect, { key, width: 4, height: 4, shader: shaderEffect(source, [], 0) });
  assert.throws(() => frame([rect('a', first), rect('b', second)]), /two different shaders share the id collide/);
  // Separately defined but identical shaders are one source.
  assert.equal(frame([rect('a', first), rect('b', same)]).shaders?.length, 1);
});
