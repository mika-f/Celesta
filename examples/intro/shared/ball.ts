// The blue square of the cold open. Reveal and Idea show BALL_SOURCE as the
// code that moves it; test/ball.test.ts runs that text against ballY.

export const BALL = { size: 120, floor: 760, height: 420, period: 30 } as const;

// Top edge of the square: one parabola per period, landing every two beats.
export function ballY(frame: number): number {
  const t = (frame % BALL.period) / BALL.period;
  return BALL.floor - BALL.size - BALL.height * 4 * t * (1 - t);
}

export const BALL_SOURCE = `function ballY(frame) {
  const t = (frame % BALL.period) / BALL.period;
  return BALL.floor - BALL.size - BALL.height * 4 * t * (1 - t);
}`;
