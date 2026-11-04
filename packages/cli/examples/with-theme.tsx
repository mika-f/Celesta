import { Composition, Font, Rect, Span, Text, Theme, useTextMetrics } from '@celesta.studio/react';
import { TextBox } from '@celesta.studio/text';

// <Theme> names fonts and colors once; <Text>, <Span>, <Rect>, and <Path>
// refer to them by name, and the shorthand props stand in for the usual
// `style` entries.
// <TextBox> and the other @celesta.studio/text components take the same names, and
// fit the text with the fonts they stand for.

const fonts = { display: 'Inter', mono: 'JetBrains Mono' };
const colors = { paper: '#F4F1EA', ink: '#111111', accent: '#FF4D1F' };

function Badge({ x, y, children }: { x: number; y: number; children: string }) {
  const m = useTextMetrics(children, { fontFamily: 'mono', fontSize: 18, fontWeight: 700 });
  return <>
    <Rect x={x} y={y} width={m.width + 24} height={36} fill="accent" />
    <Text x={x + 12} y={y + 18} anchorY={0.5} fontFamily="mono" fontSize={18} fontWeight={700} color="ink">
      {children}
    </Text>
  </>;
}

export default function WithTheme() {
  return (
    <Composition width={960} height={540} fps={30} durationInFrames={30}>
      <Font google="Inter" weights={[400, 700]} />
      <Font google="JetBrains Mono" weights={[700]} />
      <Theme fonts={fonts} colors={colors}>
        <Rect width={960} height={540} fill="ink" />
        <Badge x={56} y={56}>CELESTA</Badge>
        <Text x={56} y={140} fontFamily="display" fontSize={72} fontWeight={700} color="paper">
          Code is the <Span color="accent">cut</Span>.
        </Text>
        {/* An inner theme adds to the outer one, its own names winning. */}
        <Theme colors={{ paper: '#8B8B90' }}>
          <Text x={56} y={250} fontFamily="display" fontSize={28} color="paper">
            Same names, a quieter paper.
          </Text>
        </Theme>
        <Rect x={56} y={330} width={848} height={150} fill="#1D1D20" />
        <TextBox x={56} y={330} width={848} height={150} minFontSize={16} maxFontSize={120} maxLines={1}
          verticalAlign="middle" style={{ fontFamily: 'mono', fontWeight: 700, align: 'center' }}>
          <Span color="accent">fit</Span>Text(<Span color="paper">'mono'</Span>)
        </TextBox>
      </Theme>
    </Composition>
  );
}
