import { Group, Rect, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { blinkPhase } from '@celesta/character';
import { Label } from '../components/Label';
import { BODY, Panel } from '../components/Panel';
import { CAST } from '../character';
import { C } from '../constants';
import { lineAt, plan } from '../voice';

export const TEXT = ['レイヤーを切り替え', '目', '口', '表情', 'ひらき', '半目', 'とじ', 'ふつう', 'えがお', 'にっこり', 'ドヤ', 'こまり'];

const FACES = [['normal', 'ふつう'], ['smile', 'えがお'], ['happy', 'にっこり'], ['smug', 'ドヤ'], ['puzzled', 'こまり']] as const;

// The PSD's layer folders as they are switched on this frame for the
// speaker: eyes from the same blinkPhase() schedule the portrait uses, the
// mouth from the line's lip sync, and the face from the script.
export function PsdScene() {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const absolute = frame + plan.scene('psd').from;
  const planned = lineAt(absolute);
  const line = planned.line;
  const cast = CAST[line.speaker];
  const face = line.script.expression ?? 'normal';
  // A portrait blinks with its character's id as the seed; <Character> has no
  // `id` here, so the id is `name`. Keep the two in step if an id is added.
  const eyes = face === 'happy' ? 'happy' : blinkPhase(absolute, fps, { seed: cast.name });
  const shape = line.lipSync.mouthAtSeconds((absolute - planned.from) / fps);
  const open = shape === 'a' || shape === 'e' || shape === 'o';
  const rows: { folder: string; items: [string, boolean][] }[] = [
    { folder: '目', items: [['ひらき', eyes === 'open'], ['半目', eyes === 'half'], ['とじ', eyes === 'closed' || eyes === 'happy']] },
    { folder: '口', items: [['とじ', !open], ['ひらき', open]] },
    { folder: '表情', items: FACES.map(([id, label]) => [label, face === id]) },
  ];
  return (
    <Panel index="04" title="レイヤーを切り替え" accent={C.violet}>
      <Label x={BODY.x} y={BODY.y + 6} ay={0.5} size={28} font="mono" weight={700} color={C.soft}>
        {`assets/${line.speaker}.psd`}
      </Label>
      {rows.map((row, r) => {
        const y = BODY.y + 50 + r * 170;
        return (
          <Group key={row.folder} y={y}>
            <Label x={BODY.x} y={30} ay={0.5} size={40} weight={900} color={cast.color}>{`▼ ${row.folder}`}</Label>
            {row.items.map(([label, on], i) => {
              // Up to three wide cells in a row, or five narrow ones.
              const narrow = row.items.length > 3;
              const cellW = narrow ? 160 : 270;
              return (
                <Group key={label} x={BODY.x + i * (cellW + (narrow ? 12 : 20))} y={70}>
                  <Rect width={cellW} height={74} cornerRadius={18} fill={on ? cast.color : C.paper} stroke={C.ink} strokeWidth={on ? 5 : 3} />
                  {!narrow && <Rect x={14} y={21} width={32} height={32} cornerRadius={8} fill={on ? C.paper : C.line} />}
                  <Label x={narrow ? cellW / 2 : 58 + (cellW - 58) / 2} y={37} ax={0.5} ay={0.5} size={narrow ? 30 : 36} weight={900}
                    color={on ? C.paper : C.soft}>{label}</Label>
                </Group>
              );
            })}
          </Group>
        );
      })}
    </Panel>
  );
}
