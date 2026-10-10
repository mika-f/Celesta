import { Easings, Group, Rect, Text, progress, useCue, useCurrentFrame } from '@celesta/react';
import { TextReveal } from '@celesta/text';
import { Copy, Tag, textStyle } from '../components/Copy';
import { Stage } from '../components/Stage';
import { BEAT, C, COL, TOP, tint } from '../constants';
import { FIT_BOXES, FIT_COPY, FIT_PAD, fits } from '../measure';

// The box snaps to a new shape every other beat and the copy refills it at
// the size fitText() chose for that shape in prepare().
const STOPS = FIT_BOXES.map((box, i) => ({ at: i * BEAT * 2, ...box, fit: i }));
const BOX_Y = TOP + 330;

export function Fit() {
  const f = useCurrentFrame();
  const stop = useCue(STOPS);
  const box = stop?.cue ?? STOPS[0];
  const fit = fits[box.fit];
  const pop = 1 + 0.04 * (1 - progress(stop?.frame ?? 0, 0, 8, Easings.easeOutCubic));
  return (
    <Stage bg={C.sun} tone={tint(C.ink, 0.18)} fg={C.ink} accent={C.hot}
      tapes={['Fit the box.', 'fitText()']}>
      <TextReveal x={COL.x} y={TOP} lineHeight={116} baseline={0.86} from={-6} stagger={4}
        style={{ ...textStyle(116, 'display', C.ink), letterSpacing: -0.02 * 116 }}>
        {'FIT THE\nBOX.'}
      </TextReveal>
      <Group x={COL.x} y={BOX_Y} scale={pop} opacity={progress(f, -2, 6)}>
        <Rect width={box.width} height={box.height} stroke={C.ink} strokeWidth={5} />
        <Text x={FIT_PAD} y={(box.height - fit.height) / 2} maxWidth={box.width - FIT_PAD * 2} style={fit.style}>
          {FIT_COPY}
        </Text>
        <Tag y={box.height + 24} color={C.ink}>{`fitText() → ${fit.fontSize}px`}</Tag>
      </Group>
      <Copy x={COL.x + COL.width} y={BOX_Y - 40} ax={1} size={26} weight={500} font="mono" color={C.ink}
        opacity={progress(f, -2, 6)}>
        {`${box.width} × ${box.height}`}
      </Copy>
    </Stage>
  );
}
