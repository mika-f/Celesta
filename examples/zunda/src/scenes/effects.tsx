// 2 · エフェクト
// タイトルが跳ねて出て、glow で光り、shadow で影が付き、背景の写真がぼける
// （ぼかしは backdrop.tsx）。コードパネルの数値も、実際に描いている値を出す。

import { Easings, Group, Text, progress, spring } from '@celesta/react';

import { line, partway, useFilmFrame } from '../timing.ts';
import { COLOR, FONT, FPS, alpha, solid } from '../theme.ts';
import { CodePanel } from '../ui/CodePanel.tsx';
import { backdropCues, blurAt } from './backdrop.tsx';
import type { SceneDefinition } from './types.ts';
import { compositionCode } from './void.tsx';

/** タイトルが出る・光る・影が付くフレーム。 */
function cues() {
  const title = line('e1').from + 6;
  return {
    title,
    /** 「ピカピカ光らせたいのだ」のあたりから光り始める */
    glow: title + 34,
    /** 「shadow で影」 */
    shadow: partway('e2', 0.3),
  };
}

/** glow の強さ（0〜1）。光り始めてから 1 秒で最大になる。 */
const glowAmount = (frame: number) => progress(frame, cues().glow, 30);
/** glow の半径（px）。強さに比例して広がり、脈打つ。 */
const glowRadius = (frame: number) => {
  const pulse = 0.65 + 0.35 * Math.sin((frame - cues().title) / 5);
  return 8 + 26 * glowAmount(frame) * pulse;
};

function Stage() {
  const frame = useFilmFrame('effects');
  const cue = cues();
  if (frame < cue.title) return null;
  const pop = spring({ frame: frame - cue.title, fps: FPS, config: { damping: 11 } });
  const glow = glowAmount(frame);
  const shadow = progress(frame, cue.shadow, 14, Easings.easeOutCubic);
  const outline = { paint: solid(COLOR.zundaDeep), width: 16 };
  return (
    <Group x={930} y={470} scale={0.55 + 0.45 * pop} opacity={Math.min(1, pop * 1.5)}>
      <Text y={-128} anchorX={0.5} anchorY={0.5}
        style={{ fontFamily: FONT.ja, fontSize: 56, fontWeight: 800, fill: solid(COLOR.white), stroke: { ...outline, width: 10 } }}>
        ずんだもんとめたんの
      </Text>
      <Text anchorX={0.5} anchorY={0.5}
        style={{ fontFamily: FONT.title, fontSize: 168, fill: solid(COLOR.white), stroke: outline }}
        glow={glow > 0 ? { color: alpha('#D4FF6E', glow), blur: glowRadius(frame) } : undefined}
        shadow={shadow > 0
          ? { color: alpha('#0A2A06', 0.7 * shadow), blur: 14 * shadow, offsetX: 0, offsetY: 16 * shadow }
          : undefined}>
        Celesta 入門
      </Text>
    </Group>
  );
}

function Overlay() {
  const frame = useFilmFrame('effects');
  const cue = cues();
  return (
    <CodePanel frame={frame} steps={[
      { at: 0, lines: [...compositionCode(), '  <Image src="./field.jpg" fit="cover" />'] },
      { at: cue.title, lines: ['  <Text style={{ fontSize: 168 }}', `    glow={{ color: '#D4FF6E', blur: ${Math.round(glowRadius(frame))} }}`] },
      { at: cue.shadow, lines: ['    shadow={{ blur: 14, offsetY: 16 }}>'] },
      { at: backdropCues().blur, lines: [`  <Image src="./field.jpg" blur={${blurAt(frame).toFixed(1)}} />`] },
    ]} />
  );
}

export const effectsScene: SceneDefinition = {
  id: 'effects',
  title: 'エフェクト',
  Stage,
  Overlay,
  strings: ['ずんだもんとめたんの', 'Celesta 入門'],
};
