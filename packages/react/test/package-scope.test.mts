import assert from 'node:assert/strict';
import { test } from 'vitest';

import { currentPackageName, movedToPlugin, packageOfPlugin } from '../src/internal.js';

test('the scope before @celesta.studio is renamed, and nothing else is', () => {
  assert.equal(currentPackageName('@celesta/react'), '@celesta.studio/react');
  assert.equal(currentPackageName('@celesta/react/jsx-runtime'), '@celesta.studio/react/jsx-runtime');
  assert.equal(currentPackageName('@celesta.studio/react'), '@celesta.studio/react');
  assert.equal(currentPackageName('@celesta-plugin/flag'), '@celesta-plugin/flag');
  assert.equal(currentPackageName('react'), 'react');
});

test('the packages that became plugins name their plugins, and nothing else does', () => {
  assert.equal(movedToPlugin('@celesta.studio/aivis-speech'), 'aivis-speech');
  assert.equal(movedToPlugin('@celesta.studio/code'), 'code');
  assert.equal(movedToPlugin('@celesta.studio/lottie'), 'lottie');
  assert.equal(movedToPlugin('@celesta.studio/voicevox'), 'voicevox');
  for (const other of ['@celesta.studio/react', '@celesta.studio/character', '@celesta/code', '@celesta-plugin/code', 'toString']) {
    assert.equal(movedToPlugin(other), undefined, other);
  }
});

test('the first-party plugin imports name the packages they were made from', () => {
  assert.equal(packageOfPlugin('@celesta-plugin/code'), '@celesta.studio/code');
  assert.equal(packageOfPlugin('@celesta-plugin/voicevox'), '@celesta.studio/voicevox');
  for (const other of ['@celesta-plugin/vrm', '@celesta-plugin/flag', '@celesta.studio/code', 'code']) {
    assert.equal(packageOfPlugin(other), undefined, other);
  }
});
