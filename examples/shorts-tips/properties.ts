import { defineProjectProperties } from '@celesta/react';
import { useProjectProperty } from '@celesta/project';
import { DEMOS, type TipId } from './demos';

// The template's inputs. Each file in variants/ sets them for one Tip; the
// defaults below render the first one (variants/spring.json).
defineProjectProperties({
  tip: { type: 'select', label: 'Demo', defaultValue: 'spring', options: Object.keys(DEMOS) },
  number: { type: 'number', label: 'Tip number', defaultValue: 1, min: 1, max: 99, step: 1 },
  title: { type: 'string', label: 'Title', defaultValue: 'interpolate と spring、止まり方の違い' },
  hook: { type: 'string', label: 'Hook (first 2 s)', defaultValue: 'その動き、\n止まり方で化ける。' },
  code: {
    type: 'string',
    label: 'Code',
    defaultValue: [
      'const a = interpolate(frame,',
      '  [0, 20], [0, 1],',
      "  { extrapolateRight: 'clamp' });",
      'const b = spring({ frame, fps,',
      '  config: { damping: 8 } });',
      '',
      '<Ball x={a * 560} />',
      '<Ball x={b * 560} />',
    ].join('\n'),
  },
  // One code line (1-based) per demo stage: the stage starts when that line
  // has been typed. The demo in demos/ decides how many stages it has.
  stages: { type: 'string', label: 'Stage lines', defaultValue: '1,4,7,8' },
  closing: { type: 'string', label: 'Closing line', defaultValue: '着地は interpolate、\n余韻は spring。' },
  accent: { type: 'color', label: 'Accent', defaultValue: '#FF7A59' },
  guides: { type: 'boolean', label: 'Show safe area', defaultValue: false },
});

export function useTip() {
  return {
    tip: useProjectProperty<TipId>('tip'),
    number: useProjectProperty<number>('number'),
    title: useProjectProperty<string>('title'),
    hook: useProjectProperty<string>('hook'),
    closing: useProjectProperty<string>('closing'),
    accent: useProjectProperty<string>('accent'),
    guides: useProjectProperty<boolean>('guides'),
  };
}
