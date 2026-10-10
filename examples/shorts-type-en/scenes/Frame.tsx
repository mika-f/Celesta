import { Easings, Rect, progress, useCurrentFrame } from '@celesta/react';
import { Copy, Tag } from '../components/Copy';
import { FitStack, stackHeight } from '../components/FitStack';
import { Stage } from '../components/Stage';
import { BAR, BEAT, C, COL, TOP, tint } from '../constants';
import { frameLines } from '../measure';

const Y = TOP + 130;

// A small line, then a fitted stack entering a line per beat, then the frame
// number itself: the copy is the thing it describes.
export function Frame() {
  const f = useCurrentFrame();
  const settle = progress(f, 0, BAR, Easings.easeOutCubic);
  const counterIn = progress(f, BEAT * 4, 8, Easings.easeOutExpo);
  const rule = Y + stackHeight(frameLines) + 50;
  return (
    <Stage bg={C.paper} tone={tint(C.ink, 0.14)} fg={C.ink} accent={C.hot}
      tapes={['Every frame is a function of its number.', 'useCurrentFrame()']}
      rotation={-2.5 * (1 - settle)} zoom={1.04 - 0.04 * settle}>
      <Copy x={COL.x} y={TOP + 40} size={60} color={C.ink} opacity={progress(f, -3, 6)}>
        Every frame is
      </Copy>
      <FitStack lines={frameLines} x={COL.x} y={Y} from={BEAT} stagger={BEAT} color={C.ink} />
      <Rect x={COL.x} y={rule} width={COL.width * progress(f, BEAT * 4, 12, Easings.easeOutExpo)} height={6} fill={C.ink} />
      <Copy x={COL.x} y={rule + 40 + 40 * (1 - counterIn)} size={220} weight={700} font="mono" color={C.hot}
        opacity={counterIn}>
        {String(f).padStart(3, '0')}
      </Copy>
      <Tag x={COL.x + 8} y={rule + 310} color={C.ink} opacity={progress(f, BEAT * 5, 8)}>
        {'const frame = useCurrentFrame();'}
      </Tag>
    </Stage>
  );
}
