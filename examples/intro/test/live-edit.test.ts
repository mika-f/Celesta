import assert from 'node:assert/strict';
import test from 'node:test';
import { SAVE_FRAMES, TITLE, savedAt, typedAt } from '../shared/live-edit.ts';

test('the preview changes only on save', () => {
  assert.equal(savedAt(60).fontSize, 96); // typed 160, not saved yet
  assert.match(typedAt(60).source, /fontSize: 160/);
  assert.equal(savedAt(75).fontSize, 160);
  assert.equal(savedAt(164).fill, '#F5F5F7');
  assert.equal(savedAt(165).fill, '#3D7BFF');
  assert.deepEqual(SAVE_FRAMES, [75, 165]);
});

test('typing passes through deletion before the new value', () => {
  assert.match(typedAt(29).source, /fontSize: 96,/);
  assert.match(typedAt(36).source, /fontSize: ,/); // both digits deleted
  assert.match(typedAt(149).source, /color: '#3D7BFF' \}/);
});

test('the caret sits right after the text being edited', () => {
  const { source, caret } = typedAt(50);
  const line = source.split('\n')[caret.line - 1];
  assert.ok(line.slice(0, caret.column - 1).endsWith(typedAt(50).source.match(/fontSize: (\d*)/)![1]));
});

test('the fill line is a valid TextStyle paint, as the scene draws it', () => {
  assert.match(typedAt(0).source, /fill: \{ type: 'solid', color: '#F5F5F7' \},/);
  assert.match(typedAt(149).source, /fill: \{ type: 'solid', color: '#3D7BFF' \},/);
});

test('the title position shown is the one the preview uses', () => {
  const { x, y, anchorX } = TITLE;
  assert.ok(typedAt(0).source.includes(`<Text x={${x}} y={${y}} anchorX={${anchorX}}`));
});
