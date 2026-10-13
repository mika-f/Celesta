import assert from 'node:assert/strict';
import test from 'node:test';
import { BALL, BALL_SOURCE, ballY } from '../shared/ball.ts';

test('BALL_SOURCE computes exactly what ballY draws', () => {
  const shown = new Function('BALL', `${BALL_SOURCE}\nreturn ballY;`)(BALL);
  for (let f = 0; f < 1800; f += 1) assert.equal(shown(f), ballY(f));
});

test('the ball lands on the floor every period', () => {
  for (const f of [0, 30, 60, 900]) assert.equal(ballY(f), BALL.floor - BALL.size);
  assert.equal(ballY(15), BALL.floor - BALL.size - BALL.height);
});
