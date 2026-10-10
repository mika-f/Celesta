import { Easings, Rect, progress, useCurrentFrame } from '@celesta/react';
import { TextReveal } from '@celesta/text';
import { Copy, Tag, textStyle } from '../components/Copy';
import { Stage } from '../components/Stage';
import { BAR, BEAT, C, COL, TOP } from '../constants';

// A small line, then a heavy one entering a line per beat, then the frame
// number itself: the copy is the thing it describes.
export function Frame() {
  const f = useCurrentFrame();
  const settle = progress(f, 0, BAR, Easings.easeOutCubic);
  const counterIn = progress(f, BEAT * 4, 8, Easings.easeOutExpo);
  return (
    <Stage bg={C.paper} rotation={-2.5 * (1 - settle)} zoom={1.04 - 0.04 * settle}>
      <Copy x={COL.x} y={TOP + 40} size={60} weight={400} color={C.ink} opacity={progress(f, -3, 6)}>
        すべてのフレームは、
      </Copy>
      <TextReveal x={COL.x} y={TOP + 140} lineHeight={172} baseline={0.8} from={BEAT} stagger={BEAT}
        style={textStyle(150, 900, C.ink)}>
        {'フレーム\n番号の\n関数。'}
      </TextReveal>
      <Rect x={COL.x} y={TOP + 700} width={COL.width * progress(f, BEAT * 4, 12, Easings.easeOutExpo)} height={6} fill={C.ink} />
      <Copy x={COL.x} y={TOP + 740 + 40 * (1 - counterIn)} size={220} weight={700} font="mono" color={C.hot}
        opacity={counterIn}>
        {String(f).padStart(3, '0')}
      </Copy>
      <Tag x={COL.x + 8} y={TOP + 1010} color={C.ink} opacity={progress(f, BEAT * 5, 8)}>
        {'const frame = useCurrentFrame();'}
      </Tag>
    </Stage>
  );
}
