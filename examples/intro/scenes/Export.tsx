import { Easings, Rect, interpolate, useCurrentFrame } from '@celesta/react';
import { useCodePoint } from '@celesta/code';
import { useTypewriter } from '@celesta/text';
import { Check } from '../components/Check';
import { CodePanel, codeStyle } from '../components/CodePanel';
import { Headline } from '../components/Headline';
import { BEAT, C, DURATION } from '../constants';
import { BALL, ballY } from '../shared/ball';
import { AGENT_LOG, DONE_AT, EXPORT_COMMAND, TYPED_AT, framesDone } from '../shared/export';

const clamp = { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' } as const;
const FONT_SIZE = 28;
const AGENT_AT = DONE_AT + BEAT; // the copy changes and the mosaic gives way to the agent log
const COLUMNS = 30;
const TILES = 300; // one tile per 6 frames of this video
const MOSAIC = { x: 363, y: 470, tile: 34, step: 40 } as const;

// Positioned in code coordinates at a source position, like a caret.
function CheckAt({ source, line, column, size, dx = 0 }: { source: string; line: number; column: number; size: number; dx?: number }) {
  const point = useCodePoint(source, { line, column }, codeStyle(FONT_SIZE));
  return <Check x={point.x + dx} y={point.y + (point.lineHeight - size) / 2} size={size} />;
}

// 44–52 s. One command renders every frame of this video; then an agent's
// log shows the same work done from the Celesta skill.
export function Export() {
  const local = useCurrentFrame();
  const { text } = useTypewriter(EXPORT_COMMAND, { framesPerChar: (TYPED_AT - 4) / EXPORT_COMMAND.length });
  const done = framesDone(local);
  const status = local < TYPED_AT ? '' : done < DURATION
    ? `rendering ${String(done).padStart(4, ' ')} / ${DURATION}`
    : `wrote intro.mp4 · ${DURATION} frames`;
  const terminal = status ? `$ ${text}\n${status}` : `$ ${text}`;
  const mosaic = interpolate(local, [AGENT_AT, AGENT_AT + 10], [1, 0], clamp);
  const agent = interpolate(local, [AGENT_AT + 4, AGENT_AT + 14], [0, 1], { ...clamp, easing: Easings.easeOutCubic });
  const lines = AGENT_LOG.filter((_, i) => local >= AGENT_AT + BEAT * (i + 1));
  const log = lines.map((line) => `   ${line}`).join('\n');
  return (
    <>
      <Headline x={160} y={96} size={80}>{local >= AGENT_AT ? 'Your agent can write it too.' : 'One command to MP4.'}</Headline>
      <CodePanel x={160} y={220} width={1600} height={210} source={terminal} language="text" fontSize={FONT_SIZE} title="terminal">
        {done === DURATION && <CheckAt source={terminal} line={2} column={status.length + 1} size={28} dx={20} />}
      </CodePanel>

      {mosaic > 0 && Array.from({ length: TILES }, (_, i) => {
        const lit = done >= (i + 1) * (DURATION / TILES);
        const high = BALL.floor - BALL.size - ballY(i * (DURATION / TILES)) > BALL.height / 2;
        return (
          <Rect key={i} x={MOSAIC.x + (i % COLUMNS) * MOSAIC.step} y={MOSAIC.y + Math.floor(i / COLUMNS) * MOSAIC.step}
            width={MOSAIC.tile} height={MOSAIC.tile} opacity={mosaic}
            fill={lit ? (high ? C.accent : C.muted) : C.panel} />
        );
      })}

      {agent > 0 && (
        <CodePanel x={160} y={470} width={1600} height={300} source={log || ' '} language="text" fontSize={FONT_SIZE}
          title="agent" opacity={agent}>
          {lines.map((_, i) => <CheckAt key={i} source={log} line={i + 1} column={1} size={24} />)}
        </CodePanel>
      )}
    </>
  );
}
