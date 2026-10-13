import { Group, Rect, spring, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { useTypewriter } from '@celesta/text';
import { BallStage } from '../components/BallStage';
import { CodePanel, noirTheme } from '../components/CodePanel';
import { Headline } from '../components/Headline';
import { BAR, BEAT, C, W } from '../constants';
import { BALL, BALL_SOURCE, ballY } from '../shared/ball';
import { S } from '../shared/cues';

// The preview frame the cold open shrinks into.
export const PREVIEW = { x: 980, y: 338, width: 860, scale: 860 / W } as const;

// The code that draws the ball, as shown: BALL_SOURCE is tested against
// ballY, and every number here comes from the constants the picture uses.
export function revealSource(frame: number): string {
  return `import { Rect, useCurrentFrame } from '@celesta/react';

const BALL = {
  size: ${BALL.size}, floor: ${BALL.floor},
  height: ${BALL.height}, period: ${BALL.period},
};

${BALL_SOURCE}

function Ball() {
  const frame = useCurrentFrame(); // ${frame}
  return (
    <Rect x={${(W - BALL.size) / 2}} y={ballY(frame)} fill="${C.accent}"
      width={${BALL.size}} height={${BALL.size}} />
  );
}
`;
}

// 6–10 s. The playing picture shrinks into a preview; the code that draws it
// types in beside it, its Rect line lit in step with the bounce.
export function Reveal() {
  const local = useCurrentFrame();
  const { fps } = useVideoConfig();
  const frame = S.reveal + local;
  const p = spring({ frame: local, fps, config: { stiffness: 140, damping: 20 } });
  const scale = 1 + (PREVIEW.scale - 1) * p;
  const source = revealSource(frame);
  const rectLine = source.split('\n').findIndex((line) => line.includes('<Rect')) + 1;
  const { length, done } = useTypewriter(source, { from: BEAT, framesPerChar: 0.15 });
  const lift = BALL.floor - BALL.size - ballY(frame);
  const theme = { ...noirTheme, highlightLine: `${C.accent}${Math.round(0x22 + 0x44 * (lift / BALL.height)).toString(16).padStart(2, '0')}` };
  return (
    <>
      <CodePanel x={80} y={220} width={860} height={720} source={source} fontSize={20} title="film.tsx"
        visibleCharacters={length} highlightLines={done ? [rectLine, rectLine + 1] : undefined} theme={theme}
        opacity={local >= BEAT ? 1 : 0} />
      <Group x={PREVIEW.x * p} y={PREVIEW.y * p} scale={scale}>
        <BallStage frame={frame} />
      </Group>
      <Rect x={PREVIEW.x} y={PREVIEW.y} width={PREVIEW.width} height={1080 * PREVIEW.scale}
        stroke={C.line} strokeWidth={1} opacity={p} />
      {local >= BAR && <Headline x={80} y={96} size={96}>This video is code.</Headline>}
    </>
  );
}
