import { Group, Rect, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { Label } from '../components/Label';
import { BODY, Panel } from '../components/Panel';
import { CAST } from '../character';
import { C } from '../constants';
import { lineAt, plan } from '../voice';

export const TEXT = ['声から口パク', '口', 'あいうえおん', 'とじ'];

const KANA = { a: 'あ', i: 'い', u: 'う', e: 'え', o: 'お', closed: 'ん' } as const;
const COLUMNS = 8;
const CHIP = 94;
const GAP = (BODY.w - COLUMNS * CHIP) / (COLUMNS - 1);

// The AudioQuery of the line being spoken, one chip per mora, lit as the
// voice reaches it; below, the mouth shape lipSyncFromVoicevox() picks for
// this exact frame, which is what the portrait shows.
export function VoiceScene() {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const absolute = frame + plan.scene('voice').from;
  const planned = lineAt(absolute);
  const line = planned.line;
  const seconds = (absolute - planned.from) / fps;
  const mouth = line.lipSync.mouthAtSeconds(seconds);
  const color = CAST[line.speaker].color;
  const moras = line.moras.slice(0, COLUMNS * 4);
  return (
    <Panel index="03" title="声から口パク" accent={C.pink}>
      <Label x={BODY.x} y={BODY.y + 6} ay={0.5} size={28} font="mono" weight={700} color={C.soft}>
        {`voices/${line.id}.json · ${line.moras.length} moras`}
      </Label>
      {moras.map((mora, i) => {
        const x = BODY.x + (i % COLUMNS) * (CHIP + GAP);
        const y = BODY.y + 40 + Math.floor(i / COLUMNS) * (CHIP + 12);
        const now = seconds >= mora.from && seconds < mora.to;
        const past = seconds >= mora.to;
        const open = mora.mouth === 'a' || mora.mouth === 'e' || mora.mouth === 'o';
        return (
          <Group key={i} x={x + CHIP / 2} y={y + CHIP / 2} scale={now ? 1.12 : 1}>
            <Rect x={-CHIP / 2} y={-CHIP / 2} width={CHIP} height={CHIP} cornerRadius={20}
              fill={now ? color : past ? `${color}33` : C.paper} stroke={C.ink} strokeWidth={now ? 6 : 3} />
            <Label x={0} y={-8} ax={0.5} ay={0.5} size={44} weight={900} color={now ? C.paper : C.ink}>{mora.text}</Label>
            <Rect x={-16} y={26} width={32} height={8} cornerRadius={4} fill={open ? (now ? C.paper : color) : C.line} />
          </Group>
        );
      })}
      <Group y={BODY.y + BODY.h - 120}>
        <Rect x={BODY.x} width={BODY.w} height={120} cornerRadius={30} fill={C.code} />
        <Label x={BODY.x + 36} y={60} ay={0.5} size={30} font="mono" weight={700} color={C.codeDim}>lipSyncFromVoicevox →</Label>
        <Rect x={BODY.x + BODY.w - 140} y={14} width={110} height={92} cornerRadius={22} fill={mouth === 'closed' ? C.codeLine : color} />
        <Label x={BODY.x + BODY.w - 85} y={60} ax={0.5} ay={0.5} size={56} weight={900} color={C.paper}>{KANA[mouth]}</Label>
      </Group>
    </Panel>
  );
}
