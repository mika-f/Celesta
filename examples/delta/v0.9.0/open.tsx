import { Easings, FreezeFrame, Group, Rect, Sequence, Span, Text, interpolate, progress, useCurrentFrame } from '@celesta/react';
import { Pie } from '@celesta/shapes';
import { Backdrop } from '../components/Backdrop';
import { Delta } from '../components/Delta';
import { C, FONT, solid } from '../components/theme';
import { CHAPTERS } from './chapters';
import { PORTRAIT, useLayout } from './layout';
import { bedGain } from './mix';
import { Bars, Wall, wallCamera } from './multiview';
import { Program } from './program';
import { ShortProgram } from './ShortProgram';
import { DEPENDENCY_UPDATES, PRS } from './prs';
import { scoreAt } from './score';
import { DeltaTheme } from './theme';
import { CHAPTER_LENGTH, MUSIC_BAR, MUSIC_BEAT, OPEN_LENGTH, chapterAt } from './timeline';

// The open, four bars on the multiviewer. Bar 1: the monitors light, one a
// sixteenth. Bar 2: PGM counts down 3, 2, 1 while PVW holds the title.
// Bar 3: TAKE — the title goes to air, the ticker starts, PVW cues the first
// feature. Bar 4: TAKE again, and the camera dives into PGM, which is the
// first chapter by the downbeat.

export const COMMITS = 520;
const TAKE = 2 * MUSIC_BAR;
const DIVE = 3 * MUSIC_BAR;
const sixteenth = MUSIC_BEAT / 4;

/** The title card PGM takes to air. */
export function TitleCard({ frame }: { frame: number }) {
  const rise = progress(frame, 0, 14, Easings.easeOutExpo);
  if (useLayout().portrait) return <PortraitTitleCard frame={frame} rise={rise} />;
  return (
    <Group>
      <Backdrop />
      <Group x={120} y={150}>
        <Delta x={36} y={34} size={64} />
        <Text x={92} y={34} anchorY={0.5} style={{ fontFamily: FONT.mono, fontSize: 40, fontWeight: 700, fill: solid(C.paper) }}>CELESTA</Text>
      </Group>
      <Group x={110} y={250} clip={{ x: -20, y: 0, width: 1800, height: 470 }}>
        <Text y={470 * (1 - rise)} style={{ fontFamily: FONT.display, fontSize: 470, letterSpacing: { em: 0.01 }, fill: solid(C.paper) }}>
          DELTA <Span style={{ fill: C.orange }}>0.9.0</Span>
        </Text>
      </Group>
      <Text x={120} y={790} opacity={progress(frame, 6, 12)}
        style={{ fontFamily: FONT.mono, fontSize: 44, fontWeight: 700, fill: solid(C.grey) }}>
        {`v0.8.0 → main   ${PRS.length} pull requests   ${COMMITS} commits`}
      </Text>
      <Group x={1560} y={160} opacity={progress(frame, 2, 6)}>
        <Rect width={240} height={88} cornerRadius={6} fill="tally" />
        <Text x={120} y={46} anchorX={0.5} anchorY={0.5} style={{ fontFamily: FONT.display, fontSize: 72, fill: solid(C.paper) }}>ON AIR</Text>
      </Group>
    </Group>
  );
}

/** The title card at 9:16, stacked. */
function PortraitTitleCard({ frame, rise }: { frame: number; rise: number }) {
  const { width, height, safe } = PORTRAIT;
  const x = safe.left + 24;
  const display = { fontFamily: FONT.display, fontSize: 380, letterSpacing: { em: 0.01 } } as const;
  return (
    <Group>
      <Backdrop width={width} height={height} />
      <Group x={x} y={safe.top + 60}>
        <Delta x={36} y={34} size={64} />
        <Text x={92} y={34} anchorY={0.5} style={{ fontFamily: FONT.mono, fontSize: 40, fontWeight: 700, fill: solid(C.paper) }}>CELESTA</Text>
      </Group>
      <Group x={x - 10} y={safe.top + 160} clip={{ x: -20, y: 0, width: width, height: 760 }}>
        <Text y={760 * (1 - rise)} style={{ ...display, fill: solid(C.paper) }}>DELTA</Text>
        <Text y={380 + 760 * (1 - rise)} style={{ ...display, fill: solid(C.orange) }}>0.9.0</Text>
      </Group>
      {['v0.8.0 → main', `${PRS.length} pull requests`, `${COMMITS} commits`].map((line, i) => (
        <Text key={line} x={x} y={safe.top + 960 + i * 60} opacity={progress(frame, 6 + i * 2, 12)}
          style={{ fontFamily: FONT.mono, fontSize: 44, fontWeight: 700, fill: solid(i === 0 ? C.orange : C.grey) }}>{line}</Text>
      ))}
      <Group x={x} y={safe.top + 1180} opacity={progress(frame, 2, 6)}>
        <Rect width={240} height={88} cornerRadius={6} fill="tally" />
        <Text x={120} y={46} anchorX={0.5} anchorY={0.5} style={{ fontFamily: FONT.display, fontSize: 72, fill: solid(C.paper) }}>ON AIR</Text>
      </Group>
    </Group>
  );
}

/** PGM before the take: a leader counting the beats down. */
function Countdown({ frame }: { frame: number }) {
  const { width: W, height: H } = useLayout();
  const beat = Math.floor(frame / MUSIC_BEAT);
  const into = (frame % MUSIC_BEAT) / MUSIC_BEAT;
  const label = beat === 0 ? 'DELTA' : String(4 - beat);
  return (
    <Group>
      <Rect width={W} height={H} fill="#16171B" />
      <Rect x={W / 2 - 1} width={2} height={H} fill="dim" />
      <Rect y={H / 2 - 1} width={W} height={2} fill="dim" />
      <Pie x={W / 2} y={H / 2} anchorX={0.5} anchorY={0.5} radius={420} startAngle={0} endAngle={360 * into} fill="#FFFFFF14" />
      <Rect x={W / 2 - 400} y={H / 2 - 400} width={800} height={800} cornerRadius={400} stroke="grey" strokeWidth={6} />
      <Rect x={W / 2 - 330} y={H / 2 - 330} width={660} height={660} cornerRadius={330} stroke="dim" strokeWidth={3} />
      <Text x={W / 2} y={H / 2 + 20} anchorX={0.5} anchorY={0.5}
        style={{ fontFamily: FONT.display, fontSize: beat === 0 ? 220 : 520, fill: solid(beat === 0 ? C.orange : C.paper) }}>{label}</Text>
    </Group>
  );
}

/** A chapter's program as it looks at `frame` of the chapter, wherever it is drawn. */
export function ProgramAt({ index, frame }: { index: number; frame: number }) {
  const at = chapterAt(index);
  const Page = useLayout().portrait ? ShortProgram : Program;
  return (
    <FreezeFrame frame={at + frame}>
      <DeltaTheme>
        <Sequence from={at}><Page chapter={CHAPTERS[index]} index={index} live={false} /></Sequence>
      </DeltaTheme>
    </FreezeFrame>
  );
}

/** The meters on PGM: the score's level as the mix plays it. */
export function meters(frame: number): [number, number] {
  const { bands, rms } = scoreAt(frame);
  const gain = bedGain(frame);
  const level = Math.min(1, rms * 2.6 * gain);
  const tilt = (bands[2] - bands[20]) * 0.08;
  return [Math.max(0, level + tilt), Math.max(0, level - tilt)];
}

export function Open() {
  const frame = useCurrentFrame();
  const layout = useLayout();
  const camera = wallCamera(interpolate(frame, [DIVE, OPEN_LENGTH], [0, 1], { easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' }), layout);
  // Monitor m lights on sixteenth m: PVW, PGM, then the sources.
  const power = (m: number) => progress(frame, Math.round(m * sixteenth) + 1, 5);
  const pgm = frame < MUSIC_BAR ? <Bars label="PGM" />
    : frame < TAKE ? <Countdown frame={frame - MUSIC_BAR} />
      : frame < DIVE ? <TitleCard frame={frame - TAKE} />
        : <ProgramAt index={0} frame={0} />;
  const pvw = frame < MUSIC_BAR ? <Bars label="PVW" />
    : frame < TAKE ? <TitleCard frame={20} />
      : frame < DIVE ? <ProgramAt index={0} frame={0} />
        : <ProgramAt index={1} frame={60} />;
  const ticker = Math.max(0, frame - TAKE) * 9;
  return (
    <DeltaTheme>
      <Group {...camera}>
        <Wall pvw={pvw} pgm={pgm} pvwLabel={frame < DIVE ? 'PVW' : 'PVW  02 CALLOUT'}
          pgmLabel={frame < TAKE ? 'PGM  BLACK' : frame < DIVE ? 'PGM  DELTA 0.9.0' : 'PGM  01 LOWER THIRD'}
          source={(i) => {
            const Demo = CHAPTERS[i].Demo;
            return <Demo frame={(frame + i * 23) % CHAPTER_LENGTH} live={false} />;
          }}
          power={power} onAir={frame >= TAKE}
          sourceTally={(i) => (frame >= DIVE && i === 0 ? 'program' : (frame >= TAKE && frame < DIVE && i === 0) || (frame >= DIVE && i === 1) ? 'preview' : 'off')}
          ticker={ticker} lead={`CELESTA 0.9.0   ${PRS.length} PULL REQUESTS   ${DEPENDENCY_UPDATES} DEPENDENCY UPDATES`}
          meters={meters(frame)} />
      </Group>
    </DeltaTheme>
  );
}
