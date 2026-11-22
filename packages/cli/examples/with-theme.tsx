import {
  Composition, Font, Rect, Span, Text, Theme, defineProjectProperties, definePalettes, getProjectProperty, mix,
  useTextMetrics,
} from '@celesta.studio/react';
import { TextBox } from '@celesta.studio/text';

// <Theme> names fonts and colors once; <Text>, <Span>, <Rect>, and <Path>
// refer to them by name, and the shorthand props stand in for the usual
// `style` entries.
// The colors come from a palette: four roles (ground, main, second for
// text, accent) and blends made from them. The Properties panel picks the
// palette, as does `--props '{"palette":"paper"}'` on export, and the whole
// film follows.
// <TextBox> and the other @celesta.studio/text components take the same names, and
// fit the text with the fonts they stand for.

const fonts = { display: 'Inter', mono: 'JetBrains Mono' };

const palettes = definePalettes({
  ink: { ground: '#111111', main: '#F4F1EA', second: '#F4F1EA', accent: '#FF4D1F' },
  paper: { ground: '#F4F1EA', main: '#1F4E79', second: '#111111', accent: '#D93A12' },
}, {
  derived: {
    panel: mix('ground', 'second', 0.06),
    quiet: mix('second', 'ground', 0.45),
  },
});

defineProjectProperties({ palette: palettes.property({ label: 'Palette' }) });

function Badge({ x, y, children }: { x: number; y: number; children: string }) {
  const m = useTextMetrics(children, { fontFamily: 'mono', fontSize: 18, fontWeight: 700 });
  return <>
    <Rect x={x} y={y} width={m.width + 24} height={36} fill="accent" />
    <Text x={x + 12} y={y + 18} anchorY={0.5} fontFamily="mono" fontSize={18} fontWeight={700} color="ground">
      {children}
    </Text>
  </>;
}

export default function WithTheme() {
  const palette = palettes.get(getProjectProperty('palette'));
  return (
    <Composition width={960} height={540} fps={30} durationInFrames={30}>
      <Font google="Inter" weights={[400, 700]} />
      <Font google="JetBrains Mono" weights={[700]} />
      <Theme fonts={fonts} palette={palette}>
        <Rect width={960} height={540} fill="ground" />
        <Badge x={56} y={56}>CELESTA</Badge>
        <Text x={56} y={140} fontFamily="display" fontSize={72} fontWeight={700} color="second">
          Code is the <Span color="accent">cut</Span>.
        </Text>
        {/* An inner theme adds to the outer one, its own names winning. */}
        <Theme colors={{ second: palette.colors.quiet! }}>
          <Text x={56} y={250} fontFamily="display" fontSize={28} color="second">
            Same names, a quieter second.
          </Text>
        </Theme>
        <Rect x={56} y={330} width={848} height={150} fill="panel" />
        <TextBox x={56} y={330} width={848} height={150} minFontSize={16} maxFontSize={120} maxLines={1}
          verticalAlign="middle" style={{ fontFamily: 'mono', fontWeight: 700, align: 'center' }}>
          <Span color="accent">fit</Span><Span color="main">Text(</Span><Span color="second">'mono'</Span><Span color="main">)</Span>
        </TextBox>
      </Theme>
    </Composition>
  );
}
