import * as React from 'react';
import { Assets, Composition, Font, Rect, Sequence, Span, Text } from '@celesta.studio/react';
import { Code } from '@celesta.studio/code';
import { useTypewriter } from '@celesta.studio/text';
import type { TextStyle } from '@celesta.studio/react';

// Font stacks (和欧混植): `fontFamily` lists families, and each character is
// drawn with the first one that has it. Latin comes from Inter or JetBrains
// Mono, Japanese from Noto Sans JP. Three sections of two seconds each: a
// title and a paragraph, the same line without and with a stack, and code
// with Japanese comments.

const W = 1920, H = 1080;
const INK = '#f4f1ea', MUTED = '#8f8b99', ACCENT = '#ffd447', BG = '#16181f';
const SANS = ['Inter', 'Noto Sans JP'];
const MONO = ['JetBrains Mono', 'Noto Sans JP'];
const ink = (color: string) => ({ type: 'solid', color }) as const;

function Title() {
  return <>
    <Text x={160} y={240} anchorY="baseline"
      style={{ fontFamily: SANS, fontSize: 96, fontWeight: 700, fill: ink(INK) }}>
      Celesta 0.5.0 で字幕の組版を改善
    </Text>
    <Text x={160} y={360} maxWidth={1500}
      style={{ fontFamily: SANS, fontSize: 52, lineHeight: 84, fontWeight: 500, lineBreak: 'phrase', fill: ink(INK) }}>
      {'英数字は Inter、かなと漢字は Noto Sans JP で描きます。一つの段落のまま折り返し、行間は '}
      <Span style={{ fill: ACCENT, fontWeight: 700 }}>Inter の基準線</Span>
      {'にそろいます。'}
    </Text>
  </>;
}

function Comparison() {
  const label: TextStyle = { fontFamily: 'JetBrains Mono', fontSize: 32, fill: ink(MUTED) };
  const line: TextStyle = { fontSize: 88, fontWeight: 700, fill: ink(INK) };
  return <>
    <Text x={160} y={220} anchorY="baseline" style={label}>{"fontFamily: 'Inter'"}</Text>
    <Text x={160} y={340} anchorY="baseline" style={{ ...line, fontFamily: 'Inter' }}>React で映像を書く</Text>
    <Text x={160} y={560} anchorY="baseline" style={label}>{"fontFamily: ['Inter', 'Noto Sans JP']"}</Text>
    <Text x={160} y={680} anchorY="baseline" style={{ ...line, fontFamily: SANS }}>React で映像を書く</Text>
    <Text x={160} y={860} maxWidth={1600}
      style={{ fontFamily: SANS, fontSize: 40, lineHeight: 60, lineBreak: 'phrase', fill: ink(MUTED) }}>
      上は Inter にない和文が OS の書体で描かれます。読み込んだ Noto Sans JP は、並びに書いたときだけ使われます。
    </Text>
  </>;
}

const source = `// 動画をコードで書ける
const title = "和欧混植";
export default function Film() {
  return <Text>{title}</Text>; // 一行で書ける
}`;

function Typing() {
  const { length } = useTypewriter(source, { from: 0, framesPerChar: 0.5 });
  return (
    <Code x={160} y={240} language="tsx" highlightLines={[1, 2]} visibleCharacters={length}
      style={{ fontFamily: MONO, fontSize: 48, lineHeight: 80 }}>
      {source}
    </Code>
  );
}

export default function FontStack() {
  return (
    <Composition width={W} height={H} fps={30} durationInFrames={180} lang="ja-JP">
      <Assets>
        <Font src="https://fonts.googleapis.com/css2?family=Inter:wght@500;700" />
        <Font src="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;700" />
        <Font src="https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@500;700" />
      </Assets>
      <Rect width={W} height={H} fill={BG} />
      <Sequence durationInFrames={60}>
        <Title />
      </Sequence>
      <Sequence from={60} durationInFrames={60}>
        <Comparison />
      </Sequence>
      <Sequence from={120} durationInFrames={60}>
        <Typing />
      </Sequence>
    </Composition>
  );
}
