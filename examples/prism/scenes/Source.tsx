import { Rect, Text, useCurrentFrame } from '@celesta/react';
import { Label } from '../components/Label';
import { Prism } from '../components/Prism';
import { Rails } from '../components/Rails';
import { GREY, PAPER, RED } from '../constants';
import { ease, lerp } from '../math';

export function Source() {
  const f = useCurrentFrame();
  const changed = f >= 62;
  const scale = lerp(0.65, 1, ease((f - 62) / 30));
  const lines = [
    { indent: 0, text: 'function Idea() {' },
    { indent: 1, text: 'const f = useCurrentFrame();' },
    { indent: 1, text: 'return (' },
    { indent: 2, text: '<Group rotation={f * 0.4}>' },
    { indent: 3, text: `<Prism color="${changed ? RED : PAPER}" />` },
    { indent: 2, text: '</Group>' },
    { indent: 1, text: ');' },
    { indent: 0, text: '}' },
  ];
  const codeX = 110, codeY = 455, lineHeight = 39, indentWidth = 28;
  return <>
    <Rails chapter="01 / WRITE & PREVIEW" />
    <Label y={161} size={155}>YOUR CODE. ALIVE.</Label>
    <Rect x={80} y={377} width={875} height={489} fill="#1D201E" />
    <Rect x={975} y={377} width={865} height={489} fill="#252824" />
    <Label x={108} y={402} size={18} mono color={GREY}>idea.tsx</Label>
    <Label x={1003} y={402} size={18} mono color={GREY}>LIVE PREVIEW</Label>
    {/* Align the highlight to the rendered glyphs (10 px above and below). */}
    <Rect x={codeX - 10} y={codeY + 4 * lineHeight + 1} width={827} height={lineHeight + 1}
      fill={changed ? '#4A2C23' : '#30352F'} />
    {/* Explicit indentation keeps matching delimiters on the same pixel column.
        Multiline layout preserves glyph bearings and the common baseline. */}
    {lines.map((line, i) => <Text key={i} x={codeX + line.indent * indentWidth}
      y={codeY + i * lineHeight} maxWidth={817 - line.indent * indentWidth}
      style={{ fontFamily: 'IBM Plex Mono', fontSize: 23, lineHeight,
        fill: { type: 'solid', color: i === 4 ? RED : i === 1 ? '#B5C59D' : PAPER },
      }}>{`${line.text}\n`}</Text>)}
    <Prism frame={f + 180} x={1410} y={642} size={165 * scale} color={changed ? RED : PAPER} count={20} />
    <Rect x={1679} y={408} width={9} height={9} cornerRadius={4.5} fill={RED} />
    <Label x={1704} y={402} size={16} mono>{changed ? 'UPDATED' : 'READY'}</Label>
    <Label y={912} size={23} mono color={GREY}>REACT COMPONENTS. SAVE THE SOURCE. SEE THE CHANGE.</Label>
  </>;
}
