import { Assets, Composition, Font, Group, Rect, Text, useCurrentFrame, useTextMetrics } from '@celesta.studio/react';
import { codeThemes, highlightCode } from '@celesta.studio/code';

const source = `const message: string = 'Hello, Celesta.';
console.log(message);`;

const style = { fontFamily: 'IBM Plex Mono', fontSize: 40, lineHeight: 60 };
const theme = codeThemes.dark;
// The same lines and colors Code would draw.
const lines = highlightCode(source, { language: 'ts', theme });

function LineByLine() {
  const frame = useCurrentFrame();
  // Code puts each baseline at the ascent of 'M'; so does this.
  const { ascent } = useTextMetrics('M', style);
  return (
    <Group x={64} y={64}>
      {lines.map((line, i) => {
        // Lines start 20 frames apart and type two characters a frame.
        const typed = Math.max(0, Math.floor((frame - i * 20) * 2));
        if (!line.text || typed === 0) return null;
        return (
          <Text key={i} y={i * style.lineHeight + ascent} anchorY="baseline" style={{
            ...style,
            fill: { type: 'solid', color: theme.foreground },
            colorRuns: line.runs,
            visibleCharacters: line.columns[Math.min(typed, line.columns.length - 1)],
          }}>
            {line.text}
          </Text>
        );
      })}
    </Group>
  );
}

export default function Root() {
  return (
    <Composition width={1280} height={480} fps={30} durationInFrames={90}>
      <Assets>
        <Font src="./fonts/IBMPlexMono-Regular.ttf" />
      </Assets>
      <Rect width={1280} height={480} fill="#161418" />
      <LineByLine />
    </Composition>
  );
}
