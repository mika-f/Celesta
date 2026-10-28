import assert from 'node:assert/strict';
import { test } from 'vitest';

import { validators } from '../src/validate.js';

const { fail, finite, nonNegative, positive, insets } = validators('Box');

test('fail prefixes the message with the component tag', () => {
  assert.throws(() => fail('something is off'), { message: '<Box> something is off' });
});

test('finite passes finite numbers through and names the value it rejects', () => {
  assert.equal(finite('x', -3.5), -3.5);
  assert.throws(() => finite('x', Number.NaN), { message: '<Box> x must be a finite number, not NaN' });
  assert.throws(() => finite('x', Infinity), { message: '<Box> x must be a finite number, not Infinity' });
});

test('nonNegative accepts 0 and rejects negatives and non-finite values', () => {
  assert.equal(nonNegative('gap', 0), 0);
  assert.equal(nonNegative('gap', 12), 12);
  assert.throws(() => nonNegative('gap', -1), { message: '<Box> gap must be a finite number of 0 or more, not -1' });
  assert.throws(() => nonNegative('gap', Number.NaN), { message: '<Box> gap must be a finite number of 0 or more, not NaN' });
});

test('positive rejects 0, negatives, and non-finite values', () => {
  assert.equal(positive('size', 0.5), 0.5);
  assert.throws(() => positive('size', 0), { message: '<Box> size must be a finite number greater than 0, not 0' });
  assert.throws(() => positive('size', -Infinity), { message: '<Box> size must be a finite number greater than 0, not -Infinity' });
});

test('insets spreads a number to every edge and fills missing edges with 0', () => {
  assert.deepEqual(insets(8), { top: 8, right: 8, bottom: 8, left: 8 });
  assert.deepEqual(insets({ top: 4, left: 2 }), { top: 4, right: 0, bottom: 0, left: 2 });
  assert.throws(() => insets({ right: -2 }), { message: '<Box> padding right must be a finite number of 0 or more, not -2' });
});
