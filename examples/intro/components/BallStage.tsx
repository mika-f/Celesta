import { Rect } from '@celesta/react';
import { C, W } from '../constants';
import { BALL, ballY } from '../shared/ball';

// The cold open's bouncing square on its floor, at any frame. No background,
// so Reveal and Idea can scale it into a preview.
export function BallStage({ frame }: { frame: number }) {
  return (
    <>
      <Rect x={160} y={BALL.floor} width={W - 320} height={1} fill={C.line} />
      <Rect x={(W - BALL.size) / 2} y={ballY(frame)} width={BALL.size} height={BALL.size} fill={C.accent} />
    </>
  );
}
