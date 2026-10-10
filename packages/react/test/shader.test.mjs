// The core side of custom shaders: what the walker accepts and how it
// serializes it. `@celesta/shader`'s own API is tested in its package. Run
// after `pnpm run build`.

import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';

import { Composition, Group, Rect } from '../dist/index.js';
import { createResolver, mount } from '../dist/render.js';
import { shaderEffect, shaderSource } from '../dist/internal.js';
import { registerComponent } from '../dist/registry.js';

const h = React.createElement;
const ZERO = { value: 0, timescale: 30 };
const WGSL = 'fn effect(input: EffectInput) -> vec4f {\n  return source_at(input.position) * params.amount;\n}\n';

function frame(children) {
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
  assert.deepEqual(scene.layers[0].effects, { blur: 0, shader: { id: tint.id, params: [0.5, 1], padding: 6 } });
  assert.deepEqual(scene.layers[1].effects, { blur: 0, shader: { id: tint.id, params: [1, 0] } });
  assert.deepEqual(scene.layers[1].content.layers[0].effects.shader, { id: tint.id, params: [0, 0] });
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
  assert.deepEqual(scene.layers[0].effects.shader, { id: plain.id });
  assert.deepEqual(scene.shaders, [{ id: plain.id, wgsl: WGSL }]);
});

test('a frame without shaders has no shaders field', () => {
  const scene = frame(h(Rect, { width: 4, height: 4, blur: 2 }));
  assert.equal('shaders' in scene, false);
  assert.equal('shader' in scene.layers[0].effects, false);
});

test('the id follows the source and the parameters, and nothing else', () => {
  const id = (definition) => shaderSource(definition).id;
  const base = { wgsl: WGSL, params: [{ name: 'amount', type: 'f32' }] };
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

test('the shader prop only takes a ShaderEffect', () => {
  const plain = shaderSource({ wgsl: WGSL, params: [] });
  const lookalike = { id: plain.id };
  for (const shader of [lookalike, 'ripple', null]) {
    assert.throws(
      () => frame(h(Rect, { width: 4, height: 4, shader })),
      /shader must be a value returned by a shader from @celesta\/shader's defineShader\(\)/,
    );
  }
});

test('shaderSource and shaderEffect check their invariants', () => {
  assert.throws(() => shaderSource({ wgsl: '', params: [] }), /wgsl must be a non-empty string/);
  assert.throws(() => shaderSource({ wgsl: WGSL, params: [{ name: 'a', type: 'color' }] }), /f32, vec2, vec3, or vec4/);
  assert.throws(
    () => shaderSource({ wgsl: WGSL, params: Array.from({ length: 17 }, (_, i) => ({ name: `p${i}`, type: 'f32' })) }),
    /at most 16/,
  );
  const source = shaderSource({ wgsl: WGSL, params: [{ name: 'amount', type: 'vec3' }] });
  assert.throws(() => shaderEffect({ id: source.id }, [1, 2, 3], 0), /must come from shaderSource/);
  assert.throws(() => shaderEffect(source, [1, 2], 0), /expected 3 parameter values/);
  assert.throws(() => shaderEffect(source, [1, 2, Number.NaN], 0), /finite numbers/);
  assert.throws(() => shaderEffect(source, [1, 2, 3], -1), /padding/);
});

test('components resolved for project timelines refuse shaders', () => {
  const plain = shaderSource({ wgsl: WGSL, params: [] });
  registerComponent('ShadedForTimeline', () => h(Rect, { width: 4, height: 4, shader: shaderEffect(plain, [], 0) }));
  assert.throws(
    () => createResolver().resolve([{ component: 'ShadedForTimeline', props: {} }]),
    /ShadedForTimeline: custom shaders are not supported in components placed on project timelines/,
  );
});
