// 0 · 真っ黒なコンポジション
// 何もない画面に、空の <Composition> のコードが打ち込まれる。シーンの終わりに
// コードパネルが左上へ移り、次のシーンからはそこに 1 枚ずつ足されていく。

import { Easings, Group, Rect, Text, interpolate, progress } from '@celesta/react';

import { DURATION, sceneEnd, useFilmFrame } from '../timing.ts';
import { COLOR, FONT, FPS, alpha, solid } from '../theme.ts';
import { CodePanel } from '../ui/CodePanel.tsx';
import type { SceneDefinition } from './types.ts';

/** 「この動画」のコンポジション。以降のシーンのコードパネルも、この行から始まる。 */
export const COMPOSITION_CODE = [
  '<Composition width={1920} height={1080}',
  `  fps={${FPS}} durationInFrames={${DURATION}}>`,
];

function Stage() {
  const frame = useFilmFrame('void');
  const caretOn = Math.floor(frame / 15) % 2 === 0;
  const fade = 1 - progress(frame, sceneEnd('void') - 20, 20);
  return (
    <Group opacity={fade}>
      {/* 空のキャンバスの輪郭。カーソルのように明滅する */}
      <Rect x={160} y={90} width={1600} height={900} cornerRadius={8}
        stroke={alpha(COLOR.white, caretOn ? 0.18 : 0.1)} strokeWidth={2} />
      <Text x={176} y={124} anchorY="baseline" style={{ fontFamily: FONT.mono, fontSize: 18, fill: solid('#FFFFFF55') }}>
        {`1920 × 1080 · ${FPS} fps · frame ${frame}`}
      </Text>
    </Group>
  );
}

function Overlay() {
  const frame = useFilmFrame('void');
  const move = progress(frame, sceneEnd('void') - 22, 22, Easings.easeInOutCubic);
  return (
    <CodePanel frame={frame} steps={[{ at: 10, lines: [...COMPOSITION_CODE, '</Composition>'] }]}
      x={interpolate(move, [0, 1], [630, 40])} y={interpolate(move, [0, 1], [430, 36])} />
  );
}

export const voidScene: SceneDefinition = { id: 'void', Stage, Overlay };
