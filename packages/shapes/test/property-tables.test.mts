// Property tables of the shapes: registered when the package loads.

import assert from 'node:assert/strict';
import { test } from 'vitest';

import { listPrimitiveProperties } from '@celesta/react/internal';

import '../src/index.js';

test('importing the package registers every shape', () => {
  const names = Object.keys(listPrimitiveProperties());
  for (const name of ['Ellipse', 'Circle', 'Triangle', 'Polygon', 'Star', 'Pie', 'Heart']) {
    assert.ok(names.includes(name), `${name} in ${names.join(', ')}`);
  }
});

test('shape tables name their box and keep their order', () => {
  const tables = listPrimitiveProperties();
  assert.deepEqual(tables.Circle?.size, { radius: 'radius' });
  assert.deepEqual(
    tables.Star?.groups.find((group) => group.name === 'Size')?.fields.map((row) => row.key),
    ['radius', 'points', 'innerRatio', 'cornerRadius'],
  );
});

test('props the shapes require are declared required', () => {
  const tables = listPrimitiveProperties();
  const required = (name: string) =>
    tables[name]?.groups.flatMap((group) => group.fields).filter((row) => row.field.required).map((row) => row.key);
  assert.deepEqual(required('Circle'), ['radius']);
  assert.deepEqual(required('Star'), ['radius', 'points']);
  assert.deepEqual(required('Polygon'), ['radius', 'sides']);
  assert.deepEqual(required('Pie'), ['radius', 'endAngle']);
  for (const name of ['Ellipse', 'Triangle', 'Heart']) {
    assert.deepEqual(required(name), ['width', 'height'], name);
  }
});

test('shape numbers stop where the shapes reject them', () => {
  const tables = listPrimitiveProperties();
  const field = (name: string, key: string) =>
    tables[name]?.groups.flatMap((group) => group.fields).find((row) => row.key === key)?.field;
  for (const [name, key] of [
    ['Ellipse', 'width'], ['Ellipse', 'height'], ['Circle', 'radius'], ['Triangle', 'width'],
    ['Triangle', 'cornerRadius'], ['Polygon', 'radius'], ['Star', 'radius'], ['Pie', 'radius'],
    ['Pie', 'cornerRadius'], ['Heart', 'width'],
  ] as const) {
    assert.equal((field(name, key) as { min?: number } | undefined)?.min, 0, `${name} ${key}`);
  }
  // At least three sides or points, and a whole number of them.
  assert.deepEqual(field('Polygon', 'sides'), { type: 'number', required: true, min: 3, integer: true });
  assert.deepEqual(field('Star', 'points'), { type: 'number', required: true, min: 3, integer: true });
  // A stroke must be wider than nothing.
  for (const name of ['Ellipse', 'Circle', 'Triangle', 'Polygon', 'Star', 'Pie', 'Heart']) {
    assert.deepEqual(field(name, 'strokeWidth'), { type: 'number', defaultValue: 2, min: 0.1 }, name);
  }
  // Angles take any number.
  assert.deepEqual(field('Pie', 'startAngle'), { type: 'number', defaultValue: 0 });
});

test('shape transforms scrub scale and anchors finely', () => {
  const tables = listPrimitiveProperties();
  for (const name of ['Ellipse', 'Circle', 'Triangle', 'Polygon', 'Star', 'Pie', 'Heart']) {
    const transform = tables[name]?.groups.find((group) => group.name === 'Transform')?.fields;
    const field = (key: string) => transform?.find((row) => row.key === key)?.field;
    assert.deepEqual(field('scale'), { type: 'number', defaultValue: 1, step: 0.01 }, name);
    assert.deepEqual(field('anchorX'), { type: 'number', defaultValue: 0, step: 0.01 }, name);
    assert.deepEqual(field('anchorY'), { type: 'number', defaultValue: 0, step: 0.01 }, name);
    assert.deepEqual(field('opacity'), { type: 'number', defaultValue: 1, min: 0, max: 1 }, name);
  }
});
