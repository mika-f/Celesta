import * as React from 'react';
import {
  Assets, Character, CharacterView, Composition, Dialogue, Font, Rect, Sequence, Span, Text,
  TextReveal, useTypewriter,
} from '@celesta/react';
import type { AssetReference, CharacterViewReference, TextStyle } from '@celesta/react';

// Rich text: one Text, partly emphasized, still laid out as one paragraph.
// Four sections of two seconds each: an explanatory paragraph, an emphasized
// subtitle, a headline reveal, and typing.

const W = 1920, H = 1080;
const INK = '#f4f1ea', ACCENT = '#28a34a', HIGHLIGHT = '#ffd447', BG = '#16181f';
const body: TextStyle = { fontSize: 56, lineHeight: 84, fill: { type: 'solid', color: INK } };
const brand = { fontFamily: 'Bebas Neue', fill: ACCENT };

const komugi = React.createRef<AssetReference>();
const komugiView = React.createRef<CharacterViewReference>();

const explanation = <>
  Celesta は <Span style={brand}>REACT</Span> で映像を組み立てます。{'\n'}
  字幕の一部だけを<Span style={{ fontWeight: 700, fill: ACCENT }}>太く、色を変えて</Span>も、折り返しと基準線は一つの段落のまま保たれます。
</>;

function Typing() {
  const content = <>速い、<Span style={{ fontWeight: 700, fill: ACCENT }}>カンタン</Span>、頼もしい。</>;
  // `length` reveals the text without changing its layout.
  const { length } = useTypewriter(content, { from: 0, framesPerChar: 3 });
  return (
    <Text x={W / 2} y={H / 2} anchorX={0.5} anchorY={0.5}
      style={{ ...body, fontSize: 96, visibleCharacters: length }}>
      {content}
    </Text>
  );
}

export default function RichText() {
  return (
    <Composition width={W} height={H} fps={30} durationInFrames={240} lang="ja-JP">
      <Assets>
        <Font src="../afterimage/assets/fonts/BebasNeue-Regular.ttf" />
        <Character ref={komugi} name="komugi"
          portrait={{
            defaultExpression: 'normal',
            expressions: {
              normal: '../assets/dialogue-demo/portraits/komugi-normal.png',
              smile: '../assets/dialogue-demo/portraits/komugi-smile.png',
            },
          }}
          subtitle={{
            x: W / 2, y: 960, anchorX: 0.5, anchorY: 0.5, maxWidth: 1600,
            style: {
              fontSize: 56, align: 'center', fill: { type: 'solid', color: '#ffffff' },
              stroke: { paint: { type: 'solid', color: '#3a2d52' }, width: 6 },
            },
          }} />
      </Assets>
      <Rect width={W} height={H} fill={BG} />
      <Sequence durationInFrames={60}>
        <Text x={160} y={240} maxWidth={1600} style={{ ...body, lineBreak: 'phrase' }}>{explanation}</Text>
      </Sequence>
      <Sequence from={60} durationInFrames={60}>
        <CharacterView ref={komugiView} character={komugi} x={1300} y={120} />
        <Dialogue character={komugiView} expression="smile">
          この機能、<Span style={{ fill: HIGHLIGHT, fontWeight: 700 }}>ぜひ一度</Span>試してみてね！
        </Dialogue>
      </Sequence>
      <Sequence from={120} durationInFrames={60}>
        <TextReveal x={160} y={300} lineHeight={160} stagger={6}
          style={{ fontSize: 140, fill: { type: 'solid', color: INK } }}>
          {'ONE '}<Span style={brand}>PARAGRAPH</Span>{'\nMANY '}
          <Span style={{ fontWeight: 700, fill: HIGHLIGHT }}>STYLES</Span>
        </TextReveal>
      </Sequence>
      <Sequence from={180} durationInFrames={60}>
        <Typing />
      </Sequence>
    </Composition>
  );
}
