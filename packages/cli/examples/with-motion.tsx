import { Motion } from '@celesta.studio/components';
import { Composition, Font, Rect, Sequence, Text, Theme } from '@celesta.studio/react';

// <Motion> eases an element in at the start of its window, holds it exactly
// as written, and eases it out at the end. The title uses the enclosing
// sequence's window; the subtitle and the badge open their own with `from`
// and `durationInFrames`, like <Sequence>. The last card's window is shorter
// than its entrance and exit together, so both shrink to meet on one frame.

const colors = { paper: '#F4F1EA', ink: '#111111', accent: '#FF4D1F' };

export default function WithMotion() {
  return (
    <Composition width={960} height={540} fps={30} durationInFrames={120}>
      <Font google="Inter" weights={[400, 700]} />
      <Theme fonts={{ display: 'Inter' }} colors={colors}>
        <Rect width={960} height={540} fill={colors.ink} />
        <Sequence durationInFrames={96}>
          <Motion enter="fade-up" exit={{ type: 'fade-down', durationInFrames: 10 }}>
            <Text x={480} y={230} anchorX={0.5} anchorY={0.5} fontFamily="display" fontSize={72} fontWeight={700} color="paper">
              Enter, hold, exit
            </Text>
          </Motion>
          <Motion from={12} durationInFrames={72} enter={{ type: 'fade-left', distance: 32 }} exit="fade">
            <Text x={480} y={310} anchorX={0.5} anchorY={0.5} fontFamily="display" fontSize={28} color="paper" opacity={0.7}>
              One wrapper on the sequence's clock
            </Text>
          </Motion>
          {/* Placed with x/y, so `scale` grows about the badge's centre. */}
          <Motion x={480} y={400} from={24} durationInFrames={60}
            enter={{ type: 'scale', scaleFrom: 0.6 }} exit={{ type: 'scale', durationInFrames: 8 }}>
            <Rect x={-150} y={-24} width={300} height={48} cornerRadius={24} fill={colors.accent} />
            <Text anchorX={0.5} anchorY={0.5} fontFamily="display" fontSize={20} fontWeight={700} color="paper">
              @celesta.studio/components
            </Text>
          </Motion>
        </Sequence>
        <Motion from={100} durationInFrames={12} enter={{ type: 'fade-up', durationInFrames: 12 }} exit={{ type: 'fade-up', durationInFrames: 12 }}>
          <Text x={480} y={270} anchorX={0.5} anchorY={0.5} fontFamily="display" fontSize={40} fontWeight={700} color="accent">
            12 frames, still lands
          </Text>
        </Motion>
      </Theme>
    </Composition>
  );
}
