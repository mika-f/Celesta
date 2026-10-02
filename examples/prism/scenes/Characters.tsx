import { CharacterView, Dialogue, Group, Rect, Sequence, useCurrentFrame, useLipSync } from '@celesta/react';
import { VOICE, akane, akaneView, lipSync } from '../character';
import { Label } from '../components/Label';
import { Rails } from '../components/Rails';
import { GREY, INK, PAPER, RED } from '../constants';

function TalkingPortrait() {
  const mouth = useLipSync(lipSync);
  return <>
    <CharacterView ref={akaneView} character={akane} x={1410} y={650}
      anchorX={0.5} anchorY={0.5} scale={0.18} />
    <Dialogue character={akaneView} audio={VOICE} lipSync={lipSync} volume={0.9}>
      Hello! This is a lip-sync demo.
    </Dialogue>
    {['a', 'i', 'u', 'e', 'o', 'closed'].map((v, i) => <Group key={v}>
      <Rect x={80 + i * 137} y={693} width={120} height={61}
        fill={mouth === v ? RED : '#30352F'} />
      <Label x={140 + i * 137} y={724} center size={20} mono>{v.toUpperCase()}</Label>
    </Group>)}
  </>;
}

export function Characters() {
  const f = useCurrentFrame();
  return <>
    <Rails chapter="05 / CHARACTERS & VOICE" />
    <Label y={170} size={185}>GIVE IT A VOICE.</Label>
    <Rect x={990} y={377} width={850} height={503} fill={PAPER} />
    <Label x={1018} y={402} size={17} mono color={INK}>LAYERED PSD / LIVE LIP SYNC</Label>
    <Label y={419} size={72}>PSD PORTRAITS.</Label>
    <Label y={511} size={72}>STYLED SUBTITLES.</Label>
    <Label y={603} size={72} color={RED}>AUTOMATIC LIP SYNC.</Label>
    {(f < 30 || f >= 135) && <CharacterView character={akane} x={1410} y={650}
      anchorX={0.5} anchorY={0.5} scale={0.18} />}
    <Sequence from={30} durationInFrames={105}><TalkingPortrait /></Sequence>
    <Label x={80} y={913} size={19} mono color={GREY}>VOICE → VOWELS → MOUTH SHAPES</Label>
    <Label x={990} y={913} size={16} mono color={GREY}>ART: AZISABASA / KOTONOHA AKANE</Label>
    {f >= 135 && <Label x={80} y={825} size={27} mono>ONE LINE. PICTURE, VOICE, SUBTITLE.</Label>}
  </>;
}
