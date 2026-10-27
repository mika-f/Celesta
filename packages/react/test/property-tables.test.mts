// Property tables: the props the editor's Properties tab edits, per primitive.

import assert from 'node:assert/strict';
import { test } from 'vitest';

import type { RectProps } from '../src/components.js';
import { listPrimitiveProperties, num, propertyTable, required } from '../src/internal.js';

// The primitives register their tables beside their definitions, as an entry
// loads them.
import '../src/index.js';

const keysOf = (table: string, group: string) =>
  listPrimitiveProperties()[table]?.groups.find((entry) => entry.name === group)?.fields.map((row) => row.key);

test('every Celesta primitive registers a table', () => {
  const names = Object.keys(listPrimitiveProperties());
  for (const name of ['Group', 'Image', 'Rect', 'Text', 'Video']) {
    assert.ok(names.includes(name), `${name} in ${names.join(', ')}`);
  }
});

test('tables keep their declared order', () => {
  const tables = listPrimitiveProperties();
  assert.deepEqual(tables.Rect?.groups.map((group) => group.name), ['Transform', 'Size', 'Appearance']);
  assert.deepEqual(keysOf('Rect', 'Size'), ['width', 'height', 'cornerRadius']);
  assert.deepEqual(keysOf('Text', 'Text'), ['children', 'fontFamily', 'fontSize', 'fontWeight', 'color']);
});

test('size names the props that hold the box', () => {
  const tables = listPrimitiveProperties();
  assert.deepEqual(tables.Rect?.size, { width: 'width', height: 'height' });
  assert.deepEqual(tables.Text?.size, { width: 'maxWidth' });
  assert.equal(tables.Group?.size, undefined);
});

test('the Video table sizes by width and height', () => {
  const tables = listPrimitiveProperties();
  assert.deepEqual(tables.Video?.size, { width: 'width', height: 'height' });
  assert.deepEqual(tables.Video?.groups.map((group) => group.name), ['Transform', 'Source', 'Size', 'Audio']);
  const field = (key: string) => tables.Video?.groups.find((entry) => entry.name === 'Size')?.fields
    .find((row) => row.key === key)?.field;
  // A size must be positive (`Video width must be finite and positive`); no `fit`.
  assert.deepEqual(keysOf('Video', 'Size'), ['width', 'height']);
  assert.deepEqual(field('width'), { type: 'number', min: 1 });
  assert.deepEqual(field('height'), { type: 'number', min: 1 });
});

test('fields carry their defaults and bounds', () => {
  const field = (table: string, group: string, key: string) =>
    listPrimitiveProperties()[table]?.groups.find((entry) => entry.name === group)?.fields
      .find((row) => row.key === key)?.field;
  assert.deepEqual(field('Rect', 'Transform', 'opacity'), { type: 'number', defaultValue: 1, min: 0, max: 1 });
  assert.deepEqual(field('Rect', 'Transform', 'x'), { type: 'number', defaultValue: 0 });
  assert.deepEqual(field('Image', 'Size', 'fit'), { type: 'select', options: ['contain', 'cover'] });
});

test('ratios and fractions declare a fine scrub step', () => {
  const field = (table: string, group: string, key: string) =>
    listPrimitiveProperties()[table]?.groups.find((entry) => entry.name === group)?.fields
      .find((row) => row.key === key)?.field;
  for (const table of ['Group', 'Image', 'Text', 'Rect', 'Video']) {
    assert.deepEqual(field(table, 'Transform', 'scale'), { type: 'number', defaultValue: 1, step: 0.01 }, table);
    assert.deepEqual(field(table, 'Transform', 'anchorX'), { type: 'number', defaultValue: 0, step: 0.01 }, table);
    assert.deepEqual(field(table, 'Transform', 'anchorY'), { type: 'number', defaultValue: 0, step: 0.01 }, table);
  }
  assert.deepEqual(field('Video', 'Source', 'playbackRate'), { type: 'number', defaultValue: 1, min: 0, step: 0.01 });
  assert.deepEqual(field('Video', 'Audio', 'volume'), { type: 'number', defaultValue: 1, min: 0, step: 0.01 });
  assert.deepEqual(num(0, { min: 0, max: 1, step: 0.05 }), { type: 'number', defaultValue: 0, min: 0, max: 1, step: 0.05 });
  // Whole numbers are declared, never implied by a step of 1.
  assert.deepEqual(num(undefined, { min: 3, integer: true }), { type: 'number', min: 3, integer: true });
  assert.deepEqual(num(undefined, { step: 1 }), { type: 'number', step: 1 });
  assert.deepEqual(num(undefined, { integer: false }), { type: 'number' });
});

test('props the props type requires are declared required', () => {
  const field = (table: string, group: string, key: string) =>
    listPrimitiveProperties()[table]?.groups.find((entry) => entry.name === group)?.fields
      .find((row) => row.key === key)?.field;
  assert.deepEqual(field('Rect', 'Size', 'width'), { type: 'number', required: true, min: 0 });
  assert.deepEqual(field('Rect', 'Size', 'height'), { type: 'number', required: true, min: 0 });
  assert.deepEqual(field('Image', 'Source', 'src'), { type: 'path', required: true });
  assert.deepEqual(field('Video', 'Source', 'src'), { type: 'path', required: true });
  assert.deepEqual(field('Text', 'Text', 'children'), { type: 'text', required: true });
  // An optional prop says nothing.
  assert.deepEqual(field('Rect', 'Size', 'cornerRadius'), { type: 'number', defaultValue: 0, min: 0 });
  assert.deepEqual(field('Text', 'Layout', 'maxWidth'), { type: 'number', min: 0 });
  assert.deepEqual(required(num()), { type: 'number', required: true });
});

test('numbers stop where the components stop drawing or accept them', () => {
  const field = (table: string, group: string, key: string) =>
    listPrimitiveProperties()[table]?.groups.find((entry) => entry.name === group)?.fields
      .find((row) => row.key === key)?.field;
  // Font weights select static faces by weight class, 100 apart.
  assert.deepEqual(field('Text', 'Text', 'fontWeight'), { type: 'number', min: 1, max: 1000, step: 100 });
  assert.deepEqual(field('Text', 'Text', 'fontSize'), { type: 'number', min: 0 });
  assert.deepEqual(field('Rect', 'Appearance', 'strokeWidth'), { type: 'number', defaultValue: 0, min: 0 });
  // An image's size must be positive.
  assert.deepEqual(field('Image', 'Size', 'width'), { type: 'number', min: 1 });
  assert.deepEqual(field('Image', 'Size', 'height'), { type: 'number', min: 1 });
  assert.deepEqual(field('Video', 'Source', 'startFrom'), { type: 'number', defaultValue: 0, min: 0 });
  // Positions, angles, and scales take any number.
  assert.deepEqual(field('Rect', 'Transform', 'x'), { type: 'number', defaultValue: 0 });
});

// Type test: compiled by `tsc -b test`, never run. A key or size prop the
// props type lacks does not compile, nor does a required prop not declared
// `required`, or an optional one declared so.
export function misspelledKeysDoNotCompile() {
  // @ts-expect-error `widht` is not a prop of `<Rect>`.
  propertyTable<RectProps>({ groups: { Size: { widht: num() } } });
  // @ts-expect-error `<Rect>` has no `radius`.
  propertyTable<RectProps>({ size: { radius: 'radius' }, groups: {} });
}

export function requiredMatchesThePropsType() {
  // @ts-expect-error `<Rect>` requires `width`.
  propertyTable<RectProps>({ groups: { Size: { width: num() } } });
  // @ts-expect-error `cornerRadius` is optional.
  propertyTable<RectProps>({ groups: { Size: { cornerRadius: required(num(0)) } } });
  propertyTable<RectProps>({ groups: { Size: { width: required(num()), cornerRadius: num(0) } } });
}
