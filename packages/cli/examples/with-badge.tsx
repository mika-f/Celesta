import { Badge } from '@celesta/components';
import { Composition, Font, Rect, Span, Theme, interpolate, useCurrentFrame } from '@celesta/react';

// <Badge> sizes its background to the measured text, so Japanese, Latin,
// and proportional fonts fit without guessing widths from the character
// count. The middle row changes its text each second; the bottom row wraps
// at maxWidth and turns about its centre with the background.

const colors = { paper: '#F4F1EA', ink: '#111111', accent: '#FF4D1F', navy: '#204060' };
const WORDS = ['Intro', 'ほぼ日', 'Wide WWW', ''];

function Dot({ color }: { color: string }) {
  return <Rect y={-5} width={10} height={10} cornerRadius={5} fill={color} />;
}

export default function WithBadge() {
  const frame = useCurrentFrame();
  const word = WORDS[Math.floor(frame / 30) % WORDS.length];
  return (
    <Composition width={960} height={540} fps={30} durationInFrames={120}>
      <Font google="Inter" weights={[400, 700]} />
      <Font google="Noto Sans JP" weights={[700]} />
      <Font google="JetBrains Mono" weights={[500]} />
      <Theme fonts={{ sans: ['Inter', 'Noto Sans JP'], mono: 'JetBrains Mono' }} colors={colors}>
        <Rect width={960} height={540} fill={colors.paper} />

        <Badge x={64} y={64} padding={{ horizontal: 20, vertical: 10 }} fill="navy"
          fontFamily="sans" fontSize={28} fontWeight={700}>第1章</Badge>
        <Badge x={220} y={64} padding={{ horizontal: 20, vertical: 10 }} fill="navy"
          fontFamily="sans" fontSize={28} fontWeight={700}>Chapter 1</Badge>
        <Badge x={460} y={64} padding={{ horizontal: 20, vertical: 10 }} fill="ink" cornerRadius="pill"
          fontFamily="mono" fontSize={22} fontWeight={500} color="paper"
          leading={<Dot color={colors.accent} />} leadingWidth={10} gap={12}>
          {'useTextMetrics()'}
        </Badge>

        <Badge x={64} y={200} fill="paper" stroke="ink" strokeWidth={2} cornerRadius={6}
          fontFamily="sans" fontSize={28} fontWeight={400} color="ink"
          leading={<Dot color={colors.accent} />} leadingWidth={10}>
          {word}
        </Badge>
        <Badge x={480} y={200} fill="accent" cornerRadius="pill" fontFamily="sans" fontSize={28} fontWeight={700}>
          LIVE <Span fontWeight={400}>/ 生放送</Span>
        </Badge>

        <Badge x={480} y={400} anchorX={0.5} anchorY={0.5} rotation={interpolate(frame, [0, 119], [-6, 6])}
          maxWidth={420} padding={{ horizontal: 24, vertical: 14 }} fill="ink" cornerRadius={10}
          fontFamily="sans" fontSize={24} color="paper">
          A long label wraps at maxWidth and the background grows to its lines.
        </Badge>
      </Theme>
    </Composition>
  );
}
