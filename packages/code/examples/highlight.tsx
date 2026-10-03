import { Assets, Composition, Font, Group, Rect, useTypewriter } from '@celesta/react';
import { Code, useCodePoint } from '@celesta/code';

const source = `import { Composition, Text } from '@celesta/react';

export default function Title() {
  return <Text>Hello, Celesta.</Text>;
}`;
const style = { fontFamily: 'IBM Plex Mono', fontSize: 24, lineHeight: 38 };

function CodeDemo() {
  const { length, caretVisible } = useTypewriter(source, { from: 15, framesPerChar: 0.5 });
  const typedLines = Array.from(source).slice(0, length).join('').split(/\r\n|\r|\n/);
  const caret = useCodePoint(source, {
    line: typedLines.length, column: Array.from(typedLines[typedLines.length - 1]).length + 1,
  }, style);
  return <Group x={64} y={64}>
    <Code language="tsx" style={style} visibleCharacters={length} highlightLines={[4]} highlightWidth={1152}>{source}</Code>
    {caretVisible && <Rect x={caret.x} y={caret.y} width={2} height={caret.lineHeight} fill="#a68bbf" />}
  </Group>;
}

export default function Root() {
  return <Composition width={1280} height={480} fps={30} durationInFrames={180}>
    <Assets><Font src="../../../examples/prism/assets/fonts/IBMPlexMono-Regular.ttf" /></Assets>
    <Rect width={1280} height={480} fill="#161418" />
    <CodeDemo />
  </Composition>;
}
