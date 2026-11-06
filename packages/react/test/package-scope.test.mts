import assert from 'node:assert/strict';
import { test } from 'vitest';

import { currentPackageName } from '../src/internal.js';

test('the scope before @celesta.studio is renamed, and nothing else is', () => {
  assert.equal(currentPackageName('@celesta/react'), '@celesta.studio/react');
  assert.equal(currentPackageName('@celesta/react/jsx-runtime'), '@celesta.studio/react/jsx-runtime');
  assert.equal(currentPackageName('@celesta.studio/react'), '@celesta.studio/react');
  assert.equal(currentPackageName('@celesta-plugin/flag'), '@celesta-plugin/flag');
  assert.equal(currentPackageName('react'), 'react');
});
