import { Easings, Group, Rect, Text, interpolate } from '@celesta.studio/react';
import { defineShader } from '@celesta.studio/shader';
import type { DemoProps } from '../../components/ChapterShell';
import { STAGE, enter } from '../../components/ChapterShell';
import { Delta } from '../../components/Delta';
import { C, FONT, solid } from '../../components/theme';
import { BAR, BEAT } from '../timeline';

// One lens, three ways of reading past the layer's edge. It shrinks the
// picture and bends it, so its reads land outside the box: source_at()
// reads transparent there, source_at_clamped() holds the edge pixel, and
// source_at_mirrored() folds the picture back in.
export const lens = defineShader({
  name: 'lens',
  params: { zoom: 'f32', bend: 'f32', time: 'f32', mode: 'f32' },
  wgsl: /* wgsl */ `
fn read(p: vec2f) -> vec4f {
    if (params.mode < 0.5) { return source_at(p); }
    if (params.mode < 1.5) { return source_at_clamped(p); }
    return source_at_mirrored(p);
}

fn effect(input: EffectInput) -> vec4f {
    let center = mix(celesta.content.xy, celesta.content.zw, vec2f(0.5));
    let offset = input.position - center;
    let wave = vec2f(sin(offset.y / 38.0 + params.time * 3.0), cos(offset.x / 46.0 + params.time * 2.0)) * params.bend;
    return read(center + offset * params.zoom + wave);
}`,
});

const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;
const PANEL = { width: 260, height: 330, gap: 40 };
const MODES = ['source_at', 'source_at_clamped', 'source_at_mirrored'] as const;

function Picture() {
  return (
    <Group>
      <Rect width={PANEL.width} height={PANEL.height} fill={C.paper} />
      {Array.from({ length: 6 }, (_, i) => (
        <Rect key={i} x={i * 52} width={26} height={PANEL.height} fill={i % 2 ? C.blue : C.orange} />
      ))}
      <Rect y={PANEL.height - 70} width={PANEL.width} height={70} fill={C.ink} />
      <Delta x={PANEL.width / 2} y={PANEL.height / 2 - 20} size={120} fill={C.ink} />
    </Group>
  );
}

export function EdgesDemo({ frame }: DemoProps) {
  // Zooms out on the first two bars, bends on the last two.
  const zoom = interpolate(frame, [0, BAR, 2 * BAR], [1, 1.9, 1.5], { easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  const bend = interpolate(frame, [2 * BAR, 3 * BAR], [0, 40], { easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  return (
    <Group>
      {MODES.map((mode, i) => {
        const p = enter(frame, i * 4, 14);
        return (
          <Group key={mode} x={i * (PANEL.width + PANEL.gap)} y={30} opacity={p}>
            <Rect x={-3} y={-3} width={PANEL.width + 6} height={PANEL.height + 6} fill={C.line} />
            {/* Behind the picture: what a transparent read shows. */}
            <Rect width={PANEL.width} height={PANEL.height} fill={C.ink} />
            <Group shader={lens({ zoom, bend, time: frame / 30, mode: i })}>
              <Picture />
            </Group>
            <Text x={PANEL.width / 2} y={PANEL.height + 34} anchorX={0.5} anchorY={0.5} style={mono(18, i === 2 ? C.orange : C.paper, 700)}>{`${mode}()`}</Text>
            <Text x={PANEL.width / 2} y={PANEL.height + 62} anchorX={0.5} anchorY={0.5} style={mono(16, C.grey)}>
              {['transparent past the box', 'holds the edge pixel', 'folds the picture back'][i]}
            </Text>
          </Group>
        );
      })}
      <Group y={500} opacity={enter(frame, 2 * BEAT)}>
        <Rect width={STAGE.width} height={200} cornerRadius={10} fill={C.raised} />
        {[
          ['fn effect(input: EffectInput) -> vec4f {', C.paper],
          ['    let p = center + offset * zoom + wave;', C.paper],
          ['    return source_at_mirrored(p);', C.orange],
          ['}', C.paper],
        ].map(([line, color], i) => (
          <Text key={i} x={24} y={36 + i * 40} anchorY={0.5} style={mono(22, color)}>{line}</Text>
        ))}
        <Text x={STAGE.width - 24} y={36} anchorX={1} anchorY={0.5} style={mono(18, C.grey)}>{`zoom ${zoom.toFixed(2)}  bend ${bend.toFixed(0)}`}</Text>
      </Group>
    </Group>
  );
}
