import { Group, Image, Rect, Text } from '@celesta.studio/react';
import { expressionAt } from '@celesta.studio/character';
import type { DemoProps } from '../../components/ChapterShell';
import { STAGE, enter } from '../../components/ChapterShell';
import { C, FONT, FPS, solid } from '../../components/theme';
import { BAR, BEAT } from '../timeline';
import { TARGET_DBFS, voices } from '../voices';

// The repository's voice folder fixture, as importVoiceFolder() reads it:
// three .wav + .txt pairs in natural order (voice-10 after voice-2), with a
// manifest that gives the last line an expression cue 1.2 s in. Left, that
// line runs (silently: the score plays here) and the portrait changes at
// the cue, as expressionAt() and <Dialogue> pick it. Right, each line's
// speech level as matchVoiceLevels() measured it, and after its gain.
const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;
const PORTRAITS = '../assets/dialogue-demo/portraits';
const LINE_AT = BEAT;
const DB_FLOOR = -40;
const basename = (file: string) => file.split(/[\\/]/).pop() ?? file;

function Meter({ db, color, width }: { db: number | null | undefined; color: string; width: number }) {
  const level = db == null ? 0 : Math.max(0, Math.min(1, (db - DB_FLOOR) / -DB_FLOOR));
  return <Rect width={width * level} height={10} cornerRadius={2} fill={color} />;
}

export function VoicesDemo({ frame }: DemoProps) {
  const { lines, report } = voices();
  const line = lines[lines.length - 1];
  const seconds = Math.max(0, (frame - LINE_AT) / FPS);
  const expression = expressionAt(line.expression, seconds) ?? 'normal';
  const progress = Math.min(1, seconds / line.durationInSeconds);
  const cues = Array.isArray(line.expression) ? line.expression : [];
  const meter = 300;
  return (
    <Group>
      {/* The line: portrait, subtitle, and the cue on its timeline. */}
      <Group opacity={enter(frame, 0)}>
        <Rect width={380} height={STAGE.height} cornerRadius={10} fill={C.raised} />
        <Image src={`${PORTRAITS}/shizuku-${expression}.png`} x={10} y={40} width={360} height={400} />
        <Text x={190} y={470} anchorX={0.5} anchorY={0.5} maxWidth={340}
          style={{ fontFamily: FONT.sans, fontSize: 26, fontWeight: 700, lineHeight: 36, align: 'center', fill: solid(C.paper),
            stroke: [{ paint: solid(C.blue), width: 5 }, { paint: solid(C.ink), width: 10 }] }}>
          {line.text}
        </Text>
        <Group x={20} y={600}>
          <Rect width={340} height={6} fill={C.line} />
          <Rect width={340 * progress} height={6} fill={C.grey} />
          {cues.map((cue) => {
            const x = 340 * Math.min(1, cue.at / line.durationInSeconds);
            const hit = seconds >= cue.at;
            return (
              <Group key={cue.at} x={x}>
                <Rect x={-2} y={-12} width={4} height={30} fill={hit ? C.orange : C.dim} />
                <Text y={40} anchorX={0.5} anchorY={0.5} style={mono(16, hit ? C.orange : C.grey, 700)}>{`${cue.at}s ${cue.expression}`}</Text>
              </Group>
            );
          })}
          <Text y={86} style={mono(16, C.grey)}>{`expression: ${expression}`}</Text>
        </Group>
      </Group>
      {/* The folder, paired and ordered. */}
      <Group x={420} opacity={enter(frame, 4)}>
        <Text y={10} style={mono(18, C.grey)}>importVoiceFolder()</Text>
        {lines.map((l, i) => {
          const p = enter(frame, 6 + i * 4, 10);
          return (
            <Group key={l.id} y={52 + i * 44} opacity={p}>
              <Rect width={440} height={36} cornerRadius={6} fill={l === line ? '#F26A2E22' : C.raised} />
              <Text x={14} y={18} anchorY={0.5} style={mono(17, C.paper, 700)}>{basename(l.source.wav)}</Text>
              <Text x={190} y={18} anchorY={0.5} style={mono(17, C.grey)}>{`+ ${basename(l.source.txt)}`}</Text>
              <Text x={426} y={18} anchorX={1} anchorY={0.5} style={mono(16, C.dim)}>{`${l.durationInSeconds.toFixed(1)}s`}</Text>
            </Group>
          );
        })}
      </Group>
      {/* The levels, before and after the matching gain. */}
      <Group x={420} y={250} opacity={enter(frame, BAR)}>
        <Text style={mono(18, C.grey)}>matchVoiceLevels()</Text>
        <Text x={440} anchorX={1} style={mono(18, C.orange)}>{`target ${TARGET_DBFS} dBFS`}</Text>
        <Rect x={120 + meter * ((TARGET_DBFS - DB_FLOOR) / -DB_FLOOR)} y={44} width={2} height={report.lines.length * 70} fill={C.orange} opacity={0.7} />
        {report.lines.map((r, i) => {
          const after = enter(frame, BAR + 2 * BEAT + i * 4, 14);
          return (
            <Group key={r.id} y={56 + i * 70}>
              <Text anchorY={0.5} style={mono(16, C.paper, 700)}>{r.id}</Text>
              <Group x={120} y={-14}><Meter db={r.analysis?.rmsDbfs} color={C.dim} width={meter} /></Group>
              <Group x={120} y={4} opacity={after}><Meter db={r.rmsAfterDbfs} color={C.orange} width={meter} /></Group>
              <Text x={120} y={24} style={mono(15, C.grey)} opacity={after}>
                {`${r.analysis?.rmsDbfs?.toFixed(1) ?? '–'} → ${r.rmsAfterDbfs?.toFixed(1) ?? '–'} dBFS   ${r.gainDb >= 0 ? '+' : ''}${r.gainDb.toFixed(1)} dB`}
              </Text>
            </Group>
          );
        })}
      </Group>
      <Text x={420} y={STAGE.height - 30} opacity={enter(frame, 2 * BAR)} style={mono(16, C.dim)}>
        examples/assets/voice-folder/shizuku
      </Text>
    </Group>
  );
}
