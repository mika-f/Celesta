import { Panel, PatternBackground } from '@celesta.studio/components';
import { Center, useLayoutBounds } from '@celesta.studio/layout';
import { Composition, Easings, Font, Text, Theme, progress, useCurrentFrame } from '@celesta.studio/react';

// <Panel> is a box of a given size: a background, a border drawn inside the
// edge, rounded corners, and a shadow cast by the background. Its children
// start inside the padding, and <Center>, <Fit>, and useLayoutBounds() in it
// see the area inside the padding. `clip` cuts the content to the rounded
// box; anchorX/anchorY pick the point it scales and rotates about.

const colors = { bg: '#0E1319', surface: '#182030', line: '#2C3A50', ink: '#F4F1EA', dim: '#8A97A8', accent: '#FF4D1F' };
const shadow = { color: '#00000099', blur: 32, offsetX: 0, offsetY: 16 };

/** Body text wrapped to the width of the panel it is in. */
function Body({ y, children }: { y: number; children: string }) {
  const { width } = useLayoutBounds();
  return <Text y={y} maxWidth={width} fontFamily="sans" fontSize={20} color="dim" style={{ lineHeight: 32 }}>{children}</Text>;
}

export default function WithPanel() {
  const frame = useCurrentFrame();
  const enter = progress(frame, 0, 24, Easings.easeOutBack);
  return (
    <Composition width={960} height={540} fps={30} durationInFrames={120}>
      <Font google="Inter" weights={[400, 700]} />
      <Font google="JetBrains Mono" weights={[700]} />
      <Theme fonts={{ sans: 'Inter', mono: 'JetBrains Mono' }} colors={colors}>
        <PatternBackground background="bg" pattern="dots" spacing={24} size={2} color="ink" opacity={0.08} />

        {/* A caption card: the text starts inside the padding and wraps to it. */}
        <Panel x={48} y={48} width={420} height={260} padding={32}
          fill="surface" stroke="line" cornerRadius={20} shadow={shadow}>
          <Text fontFamily="sans" fontSize={32} fontWeight={700} color="ink">Panel</Text>
          <Body y={56}>A background, a border, padding, and rounded corners in one component, with the content laid out inside the padding.</Body>
        </Panel>

        {/* Per-edge padding: a header sits in the top padding, Center finds the middle of the rest. */}
        <Panel x={516} y={48} width={396} height={260} padding={{ top: 64, right: 24, bottom: 24, left: 24 }}
          fill="surface" stroke="line" cornerRadius={20} shadow={shadow}>
          <Text y={-40} fontFamily="mono" fontSize={14} fontWeight={700} color="dim">CONTENT AREA 348 × 172</Text>
          <Panel width={348} height={172} stroke="accent" strokeWidth={1} cornerRadius={8} />
          <Center>
            <Text anchorX={0.5} anchorY={0.5} fontFamily="sans" fontSize={72} fontWeight={700} color="ink">
              {String(Math.round(1280 * progress(frame, 10, 60, Easings.easeOutCubic)))}
            </Text>
          </Center>
        </Panel>

        {/* clip: the stripes and the sliding title stop at the rounded edge. */}
        <Panel x={48} y={348} width={420} height={144} padding={{ left: 32 }} fill="accent" cornerRadius={36} clip>
          <PatternBackground x={-32} width={420} height={144} pattern="stripes" spacing={24} size={8} color="#FFFFFF" opacity={0.15} speedX={40} />
          <Text y={36 + 120 * (1 - progress(frame, 20, 30, Easings.easeOutExpo))} fontFamily="sans" fontSize={56} fontWeight={700} color="ink">clipped</Text>
        </Panel>

        {/* Anchored at its centre, a badge scales in about that point; it nests in a panel. */}
        <Panel x={516} y={348} width={396} height={144} fill="surface" stroke="line" cornerRadius={20} shadow={shadow}>
          <Center>
            <Panel width={220} height={64} anchorX={0.5} anchorY={0.5} scale={enter} fill="ink" cornerRadius={32}>
              <Center>
                <Text anchorX={0.5} anchorY={0.5} fontFamily="mono" fontSize={22} fontWeight={700} color="bg">anchor 0.5</Text>
              </Center>
            </Panel>
          </Center>
        </Panel>
      </Theme>
    </Composition>
  );
}
