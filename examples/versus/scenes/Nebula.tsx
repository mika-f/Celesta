import { MediaFrame } from '@celesta.studio/components';
import { Easings, Group, Rect, progress, useCurrentFrame } from '@celesta.studio/react';
import { Label } from '../components/Label';
import { C, FONT, TOOL } from '../constants';

/** `color` (#RRGGBB) at `opacity`. */
const withAlpha = (color: string, opacity: number) =>
  color + Math.round(Math.min(1, Math.max(0, opacity)) * 255).toString(16).padStart(2, '0');

const SPEC: [string, string][] = [
  ['1,500', 'particles orbiting and twinkling'],
  ['24', 'rotating vector ellipses'],
  ['80', 'gradient bars, new heights every frame'],
  ['6', 'huge Gaussian blurs, screen-blended'],
  ['49', 'text layers rewritten every frame'],
];

// Where the video settles: a framed panel on the right.
const PANEL = { x: 820, y: 250, scale: 0.52 };

// The benchmark scene itself, as Celesta exported it: full screen first,
// then it moves aside for its spec sheet.
export function Nebula() {
  const f = useCurrentFrame();
  const m = progress(f, 30, 30, Easings.easeInOutCubic);
  const scale = 1 + (PANEL.scale - 1) * m;
  const x = PANEL.x * m;
  const y = PANEL.y * m;
  const list = progress(f, 44, 24, Easings.easeOutExpo);
  return (
    <>
      {/* The 1920×1080 export in a frame whose mat, corners, and caption grow in as it moves aside. */}
      <MediaFrame kind="video" src="./bench/out/celesta.mp4" sourceWidth={1920} sourceHeight={1080}
        x={x - 6 * m} y={y - 6 * m} width={1920 * scale + 12 * m} height={1080 * scale + 12 * m}
        padding={6 * m} cornerRadius={14 * m} fill={withAlpha(TOOL.celesta.color, m)}
        caption="bench/celesta/nebula.tsx · exported by Celesta" captionGap={24 - 6 * m}
        captionStyle={{ fontFamily: FONT.mono, fontSize: 16, fontWeight: 400, fill: { type: 'solid', color: withAlpha(C.grey, m) } }} />
      <Group x={96 - 40 * (1 - list)} y={110} opacity={list}>
        <Rect y={6} width={36} height={4} fill={TOOL.celesta.color} />
        <Label x={52} size={18} font="mono" weight={700} color={TOOL.celesta.color} spacing={3}>01 — THE TEST SCENE</Label>
        <Label y={40} size={96} weight={700} spacing={-3}>NEBULA</Label>
        <Label y={150} size={22} font="mono" weight={400} color={C.soft}>1920×1080 · 60 fps · 600 frames</Label>
        {SPEC.map(([n, text], i) => {
          const p = progress(f, 56 + i * 6, 20, Easings.easeOutExpo);
          return (
            <Group key={text} y={250 + i * 104} opacity={p}>
              <Group clip={{ x: -4, y: -6, width: 300, height: 84 }}>
                <Label y={80 * (1 - p)} size={64} weight={700} spacing={-2} color={TOOL.celesta.color}>{n}</Label>
              </Group>
              <Label y={70} size={20} weight={400} color={C.soft}>{text}</Label>
            </Group>
          );
        })}
      </Group>
      <Group x={PANEL.x} y={PANEL.y + 1080 * PANEL.scale + 70} opacity={progress(f, 100, 20, Easings.easeOutCubic)}>
        <Label size={30} weight={500}>~1,660 layers per frame. Same math, same fonts, three times.</Label>
      </Group>
    </>
  );
}
