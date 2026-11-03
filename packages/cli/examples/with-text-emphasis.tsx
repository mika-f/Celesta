import { Composition, Font, Group, Rect, Text, Theme, useCurrentFrame, useTheme } from '@celesta.studio/react';
import type { TextStyle } from '@celesta.studio/react';
import { emphasisContent, graphemes, resolveEmphasis, resolveTimedRanges, timedRangeAt } from '@celesta.studio/text';
import type { EmphasisStyle, TextEmphasis } from '@celesta.studio/text';

// Emphasis by grapheme cluster, and phrases on half-open frame ranges: the
// shared shapes <CaptionTrack> and dialogue subtitles build on. Each phrase
// is shown on frames [from, from + durationInFrames); 60–75 is a gap with
// nothing up. Ranges count clusters, so the family emoji, the flag, the
// accented "é", and 𠮷 are one each.

const W = 1280, H = 720, FPS = 30;
const INK = '#f4f1ea', MUTED = '#8b8f9c';

const styles: Record<string, EmphasisStyle> = {
  danger: { color: '#ff6060', fontWeight: 700 },
  key: { color: 'accent', fontWeight: 700 },
};

const phrases: { from: number; durationInFrames: number; text: string; emphasis: TextEmphasis[] }[] = [
  { from: 0, durationInFrames: 30, text: 'HPが1です。回復しましょう。', emphasis: [{ start: 3, end: 4, style: 'danger' }] },
  { from: 30, durationInFrames: 30, text: '家族👨‍👩‍👧で🇯🇵へ、café 𠮷野家', emphasis: [
    { start: 0, end: 3, style: 'key' },
    { start: 7, end: 11, style: { fontWeight: 700 } },
    { start: 12, end: 13, style: 'danger' },
  ] },
  { from: 75, durationInFrames: 45, text: 'Highlight only the words that matter.', emphasis: [
    { start: 19, end: 24, style: 'key' },
    { start: 30, end: 36, style: 'danger' },
  ] },
];

const ranges = resolveTimedRanges(phrases, { what: 'with-text-emphasis', label: (index) => `phrase ${index}` });

const textStyle: TextStyle = {
  fontFamily: ['Inter', 'Noto Sans JP'], fontSize: 56, lineHeight: 80, align: 'center',
  fill: { type: 'solid', color: INK }, lineBreak: 'phrase',
};
const small: TextStyle = { fontFamily: ['JetBrains Mono', 'Noto Sans JP'], fontSize: 22, lineHeight: 34, fill: { type: 'solid', color: MUTED } };

function Phrase() {
  const frame = useCurrentFrame();
  const theme = useTheme();
  const range = timedRangeAt(ranges, frame);
  if (!range) {
    return <Text x={W / 2} y={300} anchorX={0.5} style={{ ...small, fontSize: 28 }}>(gap: frames 60–75 show nothing)</Text>;
  }
  const phrase = phrases[range.index];
  const resolved = resolveEmphasis(phrase.text, phrase.emphasis, { styles, theme, what: `phrase ${range.index}` });
  const clusters = graphemes(phrase.text);
  return <Group>
    <Text x={W / 2} y={300} anchorX={0.5} maxWidth={1100} style={textStyle}>
      {emphasisContent(phrase.text, resolved)}
    </Text>
    <Text x={90} y={470} style={small}>
      {[
        `frames [${range.from}, ${range.end})  local frame ${frame - range.from}`,
        `${clusters.length} grapheme clusters, ${Array.from(phrase.text).length} code points`,
        ...resolved.map((entry) => `[${entry.start}, ${entry.end}) "${entry.text}" ${entry.name ?? 'inline'} ${JSON.stringify(entry.style)}`),
      ].join('\n')}
    </Text>
  </Group>;
}

export default function Root() {
  return <Composition width={W} height={H} fps={FPS} durationInFrames={120} lang="ja-JP">
    <Font src="https://fonts.googleapis.com/css2?family=Inter:wght@400;700" />
    <Font src="https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@400;700" />
    <Font src="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400" />
    <Theme colors={{ accent: '#28a34a' }}>
      <Rect width={W} height={H} fill="#16181f" />
      <Phrase />
    </Theme>
  </Composition>;
}
