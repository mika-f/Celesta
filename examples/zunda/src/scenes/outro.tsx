// 8 · おわり
// ロゴ、キャッチコピー、クレジット。クレジットは素材の利用条件で必要なので、
// 画面のどこかに必ず出す。

import { Group, Line, Rect, Text, TextReveal, progress, spring } from '@celesta/react';

import { SCENE_AT, line, useFilmFrame } from '../timing.ts';
import { CANVAS, COLOR, FONT, FPS, solid } from '../theme.ts';
import type { SceneDefinition } from './types.ts';

const { width: W, height: H } = CANVAS;

const TAGLINE = '動画は、コードで書ける。';
const CREDITS = [
  '音声　VOICEVOX:ずんだもん　VOICEVOX:四国めたん',
  '立ち絵　東北ずん子・ずんだもんプロジェクト 公式イラスト',
  '映像素材　Mixkit　　BGM　make-score.py（オリジナル）',
  'フォント　M PLUS Rounded 1c / Dela Gothic One / JetBrains Mono',
];

function Stage() {
  const frame = useFilmFrame('outro');
  const logo = spring({ frame: frame - SCENE_AT.outro - 4, fps: FPS, config: { damping: 10 } });
  const credits = progress(frame, line('o2').at + 10, 20);
  return (
    <>
      <Rect width={W} height={H} fill={{
        type: 'radial', center: { x: 960, y: 380 }, radius: 1100,
        stops: [{ offset: 0, color: '#E6F7CF' }, { offset: 1, color: COLOR.zunda }],
      }} />
      {/* ゆっくり回る放射状の光 */}
      {Array.from({ length: 18 }, (_, i) => {
        const angle = (i / 18) * Math.PI * 2 + frame / 90;
        return <Line key={i} x1={960 + 260 * Math.cos(angle)} y1={330 + 260 * Math.sin(angle)}
          x2={960 + 1400 * Math.cos(angle)} y2={330 + 1400 * Math.sin(angle)}
          stroke={COLOR.white} strokeWidth={60} cap="butt" opacity={0.18} />;
      })}
      <Group x={960} y={300} scale={0.5 + 0.5 * logo} opacity={Math.min(1, logo * 1.5)}>
        <Text anchorX={0.5} anchorY={0.5}
          style={{ fontFamily: FONT.title, fontSize: 200, fill: solid(COLOR.white),
            stroke: { paint: solid(COLOR.zundaDeep), width: 18 } }}
          glow={{ color: '#FFFFFFAA', blur: 24 }}>
          Celesta
        </Text>
      </Group>
      {/* TextReveal の from はシーンの先頭からのフレーム */}
      <TextReveal x={960} y={440} align={0.5} from={line('o1').at - SCENE_AT.outro} stagger={4} durationInFrames={22}
        style={{ fontFamily: FONT.ja, fontSize: 64, fontWeight: 800, lineHeight: 84, fill: solid(COLOR.zundaDeep) }}>
        {TAGLINE}
      </TextReveal>
      <Group x={960} y={600} opacity={credits}>
        {CREDITS.map((credit, i) => (
          <Text key={i} y={i * 38} anchorX={0.5} anchorY="baseline"
            style={{ fontFamily: FONT.ja, fontSize: 24, fontWeight: 500, fill: solid('#1F4A14') }}>
            {credit}
          </Text>
        ))}
      </Group>
    </>
  );
}

export const outroScene: SceneDefinition = { id: 'outro', wipeIn: true, Stage, strings: [TAGLINE, ...CREDITS] };
