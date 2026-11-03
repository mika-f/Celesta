import { Assets, Composition, Font, Group, Rect, Text, useCurrentFrame, useTextMetrics } from '@celesta.studio/react';
import { codeThemes, highlightCode } from '@celesta.studio/code';

const source = `import { Composition, Text } from '@celesta.studio/react';

export default function Title() {
	return <Text>Hello, Celesta.</Text>;
}`;
const style = { fontFamily: 'IBM Plex Mono', fontSize: 24, lineHeight: 38 };
const theme = codeThemes.dark;
// Code's own lines and colors, drawn here as one Text per line.
const lines = highlightCode(source, { language: 'tsx', theme });
const beat = 24;

function LineByLine() {
  const frame = useCurrentFrame();
  // Code places every line's baseline at the ascent of 'M'; matching it keeps the same look.
  const { ascent } = useTextMetrics('M', style);
  return <Group x={64} y={64}>
    {lines.map((line, index) => {
      // Each line starts on its own beat and types two source code points per frame.
      const typed = Math.max(0, Math.floor((frame - 15 - index * beat) * 2));
      if (!line.text || typed === 0) return null;
      const typing = typed < line.columns.length - 1;
      return <Group key={index} y={index * style.lineHeight}>
        {typing && <Rect x={-20} width={4} height={style.lineHeight} fill="#a68bbf" />}
        <Text y={ascent} anchorY="baseline" style={{
          ...style, fill: { type: 'solid', color: theme.foreground }, colorRuns: line.runs,
          // Tabs count once in the source but expand in the text; columns converts between them.
          visibleCharacters: line.columns[Math.min(typed, line.columns.length - 1)],
        }}>{line.text}</Text>
      </Group>;
    })}
  </Group>;
}

export default function Root() {
  return <Composition width={1280} height={480} fps={30} durationInFrames={180}>
    <Assets><Font src="../../../examples/prism/assets/fonts/IBMPlexMono-Regular.ttf" /></Assets>
    <Rect width={1280} height={480} fill="#161418" />
    <LineByLine />
  </Composition>;
}
