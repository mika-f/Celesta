import { Group, Rect, interpolate, spring, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { Label } from '../components/Label';
import { BODY } from '../components/Panel';
import { C, SAFE, SAFE_CX, SAFE_W } from '../constants';

export const TAGLINE = '動画も、コードで。';
export const CREDITS = [
  'VOICEVOX:東北きりたん　VOICEVOX:東北ずん子',
  '立ち絵：東北ずん子・ずんだもんプロジェクト公式',
];
export const TEXT = [TAGLINE, ...CREDITS];

// The end card: the name, the line to remember, where to get it, and the credits.
export function Outro() {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const logo = spring({ frame, fps, config: { damping: 10, stiffness: 160 } });
  const tag = spring({ frame: frame - 8, fps, config: { damping: 12 } });
  const fade = (from: number) => interpolate(frame, [from, from + 10], [0, 1], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  return (
    <Group>
      <Label x={SAFE_CX} y={SAFE.top + 110} ax={0.5} ay={0.5} size={170} font="display" weight={400} color={C.ink}
        scale={logo}>Celesta</Label>
      <Rect x={SAFE_CX} y={SAFE.top + 220} anchorX={0.5} width={520 * logo} height={14} cornerRadius={7} fill={C.yellow} />
      <Label x={SAFE_CX} y={SAFE.top + 330} ax={0.5} ay={0.5} size={84} weight={900} scale={tag} opacity={Math.min(1, tag * 1.5)}>
        {TAGLINE}
      </Label>
      <Group opacity={fade(16)}>
        <Rect x={SAFE.left} y={SAFE.top + 420} width={SAFE_W} height={96} cornerRadius={48} fill={C.code} />
        <Label x={SAFE_CX} y={SAFE.top + 468} ax={0.5} ay={0.5} size={40} font="mono" weight={700} color={C.codeText}>
          github.com/mika-f/celesta
        </Label>
      </Group>
      <Group opacity={fade(24)}>
        {CREDITS.map((credit, i) => (
          <Label key={credit} x={SAFE_CX} y={BODY.y + BODY.h - 70 + i * 48} ax={0.5} ay={0.5} size={30} weight={700} color={C.soft}>
            {credit}
          </Label>
        ))}
      </Group>
    </Group>
  );
}
