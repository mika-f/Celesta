import { Composition, Font, Group, Rect, TextBox, registerComponent } from '@celesta/react';
import type { ComponentPropertySchema } from '@celesta/react';

// A lower-third caption: change only `text`, and the font size follows from
// the box. Short text uses 64px; longer text shrinks to fit two lines, down
// to 28px; past that, the box clips it.
type CaptionProps = { text: string };

export function Caption({ text }: CaptionProps) {
  return <Group>
    <Rect id="caption-background" width={1120} height={200} cornerRadius={24} fill="#101820E6" />
    <TextBox id="caption-text" x={48} y={32} width={1024} height={136}
      minFontSize={28} maxFontSize={64} maxLines={2} lineHeight={1.3}
      verticalAlign="middle"
      style={{ fontFamily: 'IBM Plex Mono', align: 'center', lineBreak: 'phrase', fill: { type: 'solid', color: '#ffffff' } }}>
      {text}
    </TextBox>
  </Group>;
}

const captionSchema: ComponentPropertySchema<CaptionProps> = {
  text: { type: 'string', label: 'Text', defaultValue: '字幕' },
};

registerComponent('Caption', Caption, captionSchema);

const captions = [
  '字幕',
  'Celesta fits any caption into the same box.',
  '日本語の長い字幕でも、指定した枠と二行に収まるまで文字を小さくします。',
  '一行目\n二行目',
  '下限の文字サイズでも収まらないほど長い文章は、枠の端と二行目の後ろで切り取られます。'.repeat(3),
];

export default function Root() {
  return <Composition width={1280} height={1200} fps={30} durationInFrames={1}>
    <Font src="../../../examples/prism/assets/fonts/IBMPlexMono-Regular.ttf" />
    {captions.map((text, index) => <Group key={index} x={80} y={24 + index * 232}>
      <Caption text={text} />
    </Group>)}
  </Composition>;
}
