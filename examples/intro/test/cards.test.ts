import assert from 'node:assert/strict';
import test from 'node:test';
import { CHANGES, cardsAt, cardsSource } from '../shared/cards.ts';

test('the array on screen is the array the cards draw', () => {
  for (const l of [0, 119, 120, 239]) {
    const shown = new Function(`return ${cardsSource(cardsAt(l))};`)();
    assert.deepEqual(shown, cardsAt(l));
  }
  assert.notDeepEqual(cardsAt(119), cardsAt(120));
  assert.equal(cardsAt(0).length, 12);
});

test('one card changes every two beats from local frame 120', () => {
  assert.deepEqual(CHANGES.map((c) => c.at), [120, 150, 180, 210]);
  for (const { at, index } of CHANGES) {
    const before = cardsAt(at - 1);
    const after = cardsAt(at);
    const changed = after.map((d, i) => (d.value !== before[i].value ? i : -1)).filter((i) => i >= 0);
    assert.deepEqual(changed, [index]);
  }
});
