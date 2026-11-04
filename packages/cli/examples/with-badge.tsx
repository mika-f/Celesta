import { Badge, PatternBackground } from '@celesta.studio/components';
import { Center } from '@celesta.studio/layout';
import { Composition, Easings, Font, Rect, Span, Text, Theme, progress, useCurrentFrame } from '@celesta.studio/react';

// <Badge> is a label on a <Panel> sized from its measured text: the same
// text and style go to useTextMetrics() and <Text>, so the background fits
// Japanese, Latin, and proportional fonts without guessing widths.
// cornerRadius="pill" gives round ends; `leading` draws a decoration before
// the text; `maxWidth` wraps long text and the badge grows taller.

const colors = { bg: '#0E1319', surface: '#182030', line: '#2C3A50', ink: '#F4F1EA', dim: '#8A97A8', accent: '#FF4D1F', sky: '#5BC0FF' };
const words = ['Badge', 'Badge grows', 'Badge grows with its text'];

/** A colored dot centered in the leading box. */
const dot = (fill: string) => <Center><Rect anchorX={0.5} anchorY={0.5} width={10} height={10} cornerRadius={5} fill={fill} /></Center>;

export default function WithBadge() {
  const frame = useCurrentFrame();
  const pop = progress(frame, 0, 20, Easings.easeOutBack);
  const word = words[Math.min(words.length - 1, Math.floor(frame / 30))];
  return (
    <Composition width={960} height={540} fps={30} durationInFrames={120}>
      <Font google="Inter" weights={[400, 700]} />
      <Font google="JetBrains Mono" weights={[500, 700]} />
      <Font google="Noto Sans JP" weights={[700]} />
      <Theme fonts={{ sans: 'Inter', mono: 'JetBrains Mono', ja: ['Inter', 'Noto Sans JP'] }} colors={colors}>
        <PatternBackground background="bg" pattern="dots" spacing={24} size={2} color="ink" opacity={0.08} />
        <Text x={48} y={40} fontFamily="mono" fontSize={14} fontWeight={700} color="dim">MEASURED, NOT ESTIMATED</Text>

        {/* Japanese, Latin, and a pill with a dot: each background fits its text. */}
        <Badge x={48} y={80} fill="accent" fontFamily="ja" fontSize={28} fontWeight={700} color="ink">第1章</Badge>
        <Badge x={176} y={80} fill="surface" stroke="line" fontFamily="sans" fontSize={28} fontWeight={700} color="ink">Chapter 1</Badge>
        <Badge x={372} y={80} fill="surface" cornerRadius="pill" padding={{ top: 10, right: 24, bottom: 10, left: 20 }}
          fontFamily="mono" fontSize={22} fontWeight={500} color="sky" leading={dot('#5BC0FF')} leadingWidth={10} gap={12}>
          {'<Badge cornerRadius="pill">'}
        </Badge>

        {/* The text changes every second; the background follows on the same frame. */}
        <Text x={48} y={180} fontFamily="mono" fontSize={14} fontWeight={700} color="dim">THE TEXT CHANGES</Text>
        <Badge x={48} y={212} fill="ink" cornerRadius="pill" fontFamily="sans" fontSize={32} fontWeight={700} color="bg">
          {word}
        </Badge>

        {/* Spans are measured with their own weight and color. */}
        <Badge x={516} y={212} fill="surface" stroke="accent" strokeWidth={2} fontFamily="sans" fontSize={32} color="ink">
          New in <Span fontWeight={700} color="accent">0.8</Span>
        </Badge>

        {/* maxWidth: long text wraps inside the padding and the badge grows taller. */}
        <Text x={48} y={312} fontFamily="mono" fontSize={14} fontWeight={700} color="dim">MAXWIDTH 400</Text>
        <Badge x={48} y={344} maxWidth={400} fill="surface" stroke="line" padding={20} cornerRadius={16}
          fontFamily="ja" fontSize={22} color="ink" style={{ lineHeight: 34 }} leading={dot('#FF4D1F')} leadingWidth={10} gap={14}>
          長いラベルは maxWidth で折り返し、背景は行数に合わせて高くなります。
        </Badge>

        {/* Anchored at its centre, a badge scales in about that point, text and all. */}
        <Badge x={720} y={420} anchorX={0.5} anchorY={0.5} scale={pop} rotation={-6 * (1 - pop)}
          fill="accent" cornerRadius="pill" padding={{ top: 14, right: 32, bottom: 14, left: 32 }}
          fontFamily="sans" fontSize={36} fontWeight={700} color="ink">
          anchor 0.5
        </Badge>
      </Theme>
    </Composition>
  );
}
