import { Easings, Group, Rect, Span, Text, interpolate, progress, useCurrentFrame } from '@celesta/react';
import { Backdrop } from '../components/Backdrop';
import { Delta } from '../components/Delta';
import { C, FONT, solid } from '../components/theme';
import { CHAPTERS } from './chapters';
import { PORTRAIT, useLayout } from './layout';
import { CARDS } from './More';
import { Wall, wallCamera } from './multiview';
import { COMMITS, meters } from './open';
import { DEPENDENCY_UPDATES, PRS } from './prs';
import { scoreAt } from './score';
import { DeltaTheme } from './theme';
import { DURATION, FEATURES, MUSIC_BAR, OUTRO_AT, SCENE_LIST } from './timeline';

// The outro: the end card on air for a bar, then the camera pulls back out
// of PGM onto the multiviewer, every source running. On the last bar the
// studio goes off air: the tallies go out and the sources hold a frame.
// The last frame, the whole wall, is the poster.

const PULL = MUSIC_BAR;
const OFF_AIR = 3 * MUSIC_BAR;
/** Where each source holds once off air: a frame of its chapter that shows it best. */
const HOLD = [40, 70, 80, 120, 70, 120, 75, 120, 50, 60, 70, 130];

/** The score's bands in a ring around the Δ, at a frame of the film. */
function Ring({ frame, size }: { frame: number; size: number }) {
  const { bands } = scoreAt(frame);
  const count = 60;
  return (
    <Group>
      {Array.from({ length: count }, (_, i) => {
        const level = bands[Math.floor((i < count / 2 ? i : count - 1 - i) * 2 / 3)];
        const length = size * (0.05 + 0.3 * level);
        return (
          <Group key={i} rotation={(i / count) * 360}>
            <Rect x={-size * 0.012} y={-size * 0.6 - length} width={size * 0.024} height={length} cornerRadius={size * 0.012}
              fill={level > 0.7 ? C.orange : C.paper} opacity={0.25 + 0.75 * level} />
          </Group>
        );
      })}
    </Group>
  );
}

/** The end card: the release, the line, the numbers. `frame` is the film's frame. */
export function EndCard({ frame }: { frame: number }) {
  const f = frame - OUTRO_AT;
  const rise = progress(f, 0, 16, Easings.easeOutExpo);
  const info = [
    'v0.8.0  →  main',
    `${PRS.length} pull requests  +  ${DEPENDENCY_UPDATES} dependency updates`,
    `${COMMITS} commits`,
    `${FEATURES} features  +  ${CARDS.length} more  ·  100 BPM  ·  ${DURATION} frames`,
  ];
  if (useLayout().portrait) return <PortraitEndCard frame={frame} rise={rise} />;
  return (
    <Group>
      <Backdrop />
      <Text x={116} y={150} style={{ fontFamily: FONT.mono, fontSize: 34, fontWeight: 700, fill: solid(C.grey) }}>CELESTA</Text>
      <Group x={104} y={200} clip={{ x: -20, y: 0, width: 1060, height: 240 }}>
        <Text y={240 * (1 - rise)} style={{ fontFamily: FONT.display, fontSize: 240, letterSpacing: { em: 0.01 }, fill: solid(C.paper) }}>
          DELTA <Span style={{ fill: C.orange }}>0.9.0</Span>
        </Text>
      </Group>
      <Text x={116} y={470} opacity={progress(f, 6, 14)}
        style={{ fontFamily: FONT.sans, fontSize: 72, fontWeight: 700, fill: solid(C.paper) }}>Every update is a diff.</Text>
      {info.map((line, i) => (
        <Text key={line} x={120} y={630 + i * 48} opacity={progress(f, 10 + i * 3, 12)}
          style={{ fontFamily: FONT.mono, fontSize: 30, fontWeight: i === 0 ? 700 : 400, fill: solid(i === 0 ? C.orange : C.grey) }}>{line}</Text>
      ))}
      <Group x={1500} y={540}>
        <Ring frame={frame} size={420} />
        <Delta size={230} fill={C.orange} />
      </Group>
      <Text x={120} y={1000} style={{ fontFamily: FONT.mono, fontSize: 22, fill: solid(C.dim) }}>
        {'celesta.natsuneko.cat   ·   github.com/mika-f/Celesta   ·   examples/delta'}
      </Text>
    </Group>
  );
}

/** The end card at 9:16: the title stacked, the numbers and the ring under it. */
function PortraitEndCard({ frame, rise }: { frame: number; rise: number }) {
  const f = frame - OUTRO_AT;
  const { width, height, safe } = PORTRAIT;
  const x = safe.left + 24;
  const display = { fontFamily: FONT.display, fontSize: 300, letterSpacing: { em: 0.01 } } as const;
  const info = [
    'v0.8.0  →  main',
    `${PRS.length} pull requests`,
    `+ ${DEPENDENCY_UPDATES} dependency updates`,
    `${COMMITS} commits`,
    `${FEATURES} features + ${CARDS.length} more`,
  ];
  return (
    <Group>
      <Backdrop width={width} height={height} />
      <Text x={x} y={safe.top + 40} style={{ fontFamily: FONT.mono, fontSize: 34, fontWeight: 700, fill: solid(C.grey) }}>CELESTA</Text>
      <Group x={x - 10} y={safe.top + 90} clip={{ x: -20, y: 0, width, height: 560 }}>
        <Text y={560 * (1 - rise)} style={{ ...display, fill: solid(C.paper) }}>DELTA</Text>
        <Text y={280 + 560 * (1 - rise)} style={{ ...display, fill: solid(C.orange) }}>0.9.0</Text>
      </Group>
      <Text x={x} y={safe.top + 680} opacity={progress(f, 6, 14)}
        style={{ fontFamily: FONT.sans, fontSize: 64, fontWeight: 700, fill: solid(C.paper) }}>Every update is a diff.</Text>
      {info.map((line, i) => (
        <Text key={line} x={x} y={safe.top + 800 + i * 44} opacity={progress(f, 10 + i * 3, 12)}
          style={{ fontFamily: FONT.mono, fontSize: 28, fontWeight: i === 0 ? 700 : 400, fill: solid(i === 0 ? C.orange : C.grey) }}>{line}</Text>
      ))}
      <Group x={x + 120} y={safe.top + 1145}>
        <Ring frame={frame} size={120} />
        <Delta size={66} fill={C.orange} />
      </Group>
      <Text x={x} y={height - safe.bottom - 70} style={{ fontFamily: FONT.mono, fontSize: 22, fill: solid(C.dim) }}>celesta.natsuneko.cat</Text>
      <Text x={x} y={height - safe.bottom - 36} style={{ fontFamily: FONT.mono, fontSize: 22, fill: solid(C.dim) }}>github.com/mika-f/Celesta</Text>
    </Group>
  );
}

/** PVW off air: the rundown of the show, every scene aired. */
function RundownScreen() {
  const rows = SCENE_LIST.scenes;
  if (useLayout().portrait) return <PortraitRundown />;
  return (
    <Group>
      <Backdrop />
      <Text x={110} y={90} style={{ fontFamily: FONT.display, fontSize: 120, fill: solid(C.paper) }}>RUNDOWN</Text>
      <Text x={1810} y={150} anchorX={1} style={{ fontFamily: FONT.mono, fontSize: 30, fill: solid(C.grey) }}>{`${rows.length} scenes · ${DURATION} frames`}</Text>
      {rows.map((row, i) => (
        <Group key={row.id} x={110} y={260 + i * 50}>
          <Rect y={4} width={22} height={22} fill={i === 0 || i === rows.length - 1 ? 'tally' : 'orange'} />
          <Text x={50} style={{ fontFamily: FONT.mono, fontSize: 30, fill: solid(C.grey) }}>{String(row.from).padStart(4, '0')}</Text>
          <Text x={190} style={{ fontFamily: FONT.mono, fontSize: 30, fontWeight: 700, fill: solid(C.paper) }}>{row.name}</Text>
          <Rect x={560} y={16} width={(row.durationInFrames / DURATION) * 3400} height={2} fill="line" />
          <Text x={1810} anchorX={1} style={{ fontFamily: FONT.mono, fontSize: 26, fill: solid(C.dim) }}>AIRED</Text>
        </Group>
      ))}
    </Group>
  );
}

/** The rundown at 9:16. */
function PortraitRundown() {
  const rows = SCENE_LIST.scenes;
  const { width, height, safe } = PORTRAIT;
  const x = safe.left + 24;
  return (
    <Group>
      <Backdrop width={width} height={height} />
      <Text x={x} y={safe.top + 20} style={{ fontFamily: FONT.display, fontSize: 140, fill: solid(C.paper) }}>RUNDOWN</Text>
      <Text x={x} y={safe.top + 180} style={{ fontFamily: FONT.mono, fontSize: 30, fill: solid(C.grey) }}>{`${rows.length} scenes · ${DURATION} frames`}</Text>
      {rows.map((row, i) => (
        <Group key={row.id} x={x} y={safe.top + 270 + i * 64}>
          <Rect y={6} width={26} height={26} fill={i === 0 || i === rows.length - 1 ? 'tally' : 'orange'} />
          <Text x={52} style={{ fontFamily: FONT.mono, fontSize: 34, fill: solid(C.grey) }}>{String(row.from).padStart(4, '0')}</Text>
          <Text x={200} style={{ fontFamily: FONT.mono, fontSize: 34, fontWeight: 700, fill: solid(C.paper) }}>{row.name}</Text>
          <Text x={width - x} anchorX={1} style={{ fontFamily: FONT.mono, fontSize: 28, fill: solid(C.dim) }}>AIRED</Text>
        </Group>
      ))}
    </Group>
  );
}

export function Outro() {
  const frame = useCurrentFrame();
  const f = frame - OUTRO_AT;
  const camera = wallCamera(interpolate(f, [PULL, 2 * MUSIC_BAR], [1, 0], { easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' }), useLayout());
  const off = f >= OFF_AIR;
  return (
    <DeltaTheme>
      <Group {...camera}>
        <Wall pvw={<RundownScreen />} pgm={<EndCard frame={frame} />}
          pvwLabel="PVW  RUNDOWN" pgmLabel={off ? 'PGM  OFF AIR' : 'PGM  CELESTA 0.9.0'}
          source={(i) => {
            const Demo = CHAPTERS[i].Demo;
            return <Demo frame={off ? HOLD[i] : (HOLD[i] + f - OFF_AIR + 144 * 4) % 144} live={false} />;
          }}
          power={() => 1} onAir={!off} sourceTally={() => 'off'}
          ticker={(Math.min(f, OFF_AIR) + OUTRO_AT) * 9} lead={`CELESTA 0.9.0   EVERY UPDATE IS A DIFF   ${PRS.length} PULL REQUESTS`}
          meters={off ? [0, 0] : meters(frame)} />
      </Group>
    </DeltaTheme>
  );
}
