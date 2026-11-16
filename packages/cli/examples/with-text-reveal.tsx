import { Composition, Font, Rect, Span, Text, Theme } from '@celesta.studio/react';
import { TextReveal } from '@celesta.studio/text';

// <TextReveal> slides text up from behind a mask. `unit` chooses what
// arrives on its own: each line (the default), each word, each Japanese
// phrase, or each character. `stagger` counts units in reading order, so a
// second line carries on where the first stopped.
//
//   celesta-exporter --react with-text-reveal.tsx --frames 12,24,90 frame.png

const fonts = { display: ['Inter', 'Noto Sans JP'], mono: 'JetBrains Mono' };
const colors = { bg: '#101820', ink: '#F4F1EA', dim: '#8A94A0', accent: '#FF4D1F' };

function Label({ x, y, children }: { x: number; y: number; children: string }) {
  return <Text x={x} y={y} fontFamily="mono" fontSize={16} fontWeight={700} color="dim">{children}</Text>;
}

export default function WithTextReveal() {
  const style = { fontFamily: 'display', fontWeight: 700, fontSize: 64, fill: { type: 'solid' as const, color: 'ink' } };
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={120} lang="ja-JP">
      <Font google="JetBrains Mono" weights={[700]} />
      <Font google="Inter" weights={[700]} />
      <Font google="Noto Sans JP" weights={[700]} />
      <Theme fonts={fonts} colors={colors}>
        <Rect width={1920} height={1080} fill="bg" />

        <Label x={120} y={90}>unit="line" (DEFAULT)</Label>
        <TextReveal x={120} y={130} lineHeight={84} from={6} stagger={6} style={style}>
          {'Each line arrives\non its own.'}
        </TextReveal>

        <Label x={1020} y={90}>unit="word"</Label>
        <TextReveal x={1020} y={130} lineHeight={84} from={6} stagger={3} unit="word" style={style}>
          Each <Span color="accent">word</Span> arrives{'\n'}on its own.
        </TextReveal>

        <Label x={120} y={420}>unit="grapheme"</Label>
        <TextReveal x={120} y={460} lineHeight={84} from={6} stagger={1} durationInFrames={14} unit="grapheme"
          style={style}>
          {'一文字ずつ、\n順番に出します。'}
        </TextReveal>

        <Label x={1020} y={420}>unit="phrase", CENTERED, maxWidth</Label>
        <TextReveal x={1380} y={460} align={0.5} maxWidth={720} lineHeight={84} from={6} stagger={4} unit="phrase"
          style={{ ...style, lineBreak: 'phrase' }}>
          文節ごとに、幅に合わせて折り返しながら出します。
        </TextReveal>

        <Label x={120} y={780}>unit="word", direction="out"</Label>
        <TextReveal x={120} y={820} lineHeight={84} from={60} stagger={3} unit="word" direction="out" style={style}>
          And leaves the same way.
        </TextReveal>
      </Theme>
    </Composition>
  );
}
