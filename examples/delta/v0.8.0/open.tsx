import { Easings, Group, Rect, Text, interpolate, progress, useCurrentFrame } from '@celesta.studio/react';
import { PatternBackground } from '@celesta.studio/components';
import { C, FONT, solid } from '../components/theme';
import { PRS } from './prs';
import { scoreAt } from './score';
import { MUSIC_BAR, MUSIC_BEAT } from './timeline';

// The first two bars, over the sheet: this release's pull requests ride
// away on a page tipped back in 3D (rotationX and a perspective), and on
// the second downbeat 0.8.0 is stamped across them, outlined three times in
// one layer. Then the cover lifts away and the camera is already over the sheet.
const ROW = 64;
const STAMP_AT = MUSIC_BAR;
const LIFT = { from: 2 * MUSIC_BAR - 6, length: 30 };

const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;

/** A title cut to `columns` monospace columns; CJK characters take two. */
function clip(title: string, columns: number) {
  let used = 0;
  for (let i = 0; i < title.length; i++) {
    used += title.charCodeAt(i) > 0x2000 ? 2 : 1;
    if (used > columns - 1) return `${title.slice(0, i)}…`;
  }
  return title;
}

function Crawl({ frame, width, height }: { frame: number; width: number; height: number }) {
  const scroll = interpolate(frame, [0, 2 * MUSIC_BAR], [ROW * 3, ROW * (PRS.length + 6)], { easing: Easings.easeInOutSine, extrapolateRight: 'clamp' });
  const wide = width > height;
  return (
    <Group x={width / 2} y={height - (wide ? 30 : 380)} rotationX={wide ? 62 : 56} perspective={wide ? 1300 : 1500}>
      <Group y={-scroll}>
        {PRS.map(([n, title], i) => {
          const y = i * ROW - scroll;
          if (y > ROW) return null;
          return (
            <Group key={n} y={i * ROW} opacity={Math.min(1, (ROW - y) / ROW)}>
              <Text x={wide ? -760 : -480} anchorY={0.5} style={mono(44, C.orange, 700)}>{`#${n}`}</Text>
              <Text x={wide ? -620 : -340} anchorY={0.5} style={mono(44, C.ink)}>{clip(title, wide ? 52 : 32)}</Text>
            </Group>
          );
        })}
      </Group>
    </Group>
  );
}

function Stamp({ frame, width, height }: { frame: number; width: number; height: number }) {
  if (frame < STAMP_AT) return null;
  const f = frame - STAMP_AT;
  const slam = progress(f, 0, 8, Easings.easeOutCubic);
  const shake = f < 10 ? Math.sin(f * 2.6) * (10 - f) : 0;
  const size = Math.min(width * 0.36, 620);
  const outline = [{ paint: solid(C.orange), width: 8 }, { paint: solid(C.paper), width: 20 }, { paint: solid(C.orange), width: 28 }];
  return (
    <Group x={width / 2 + shake} y={height / 2} rotation={-7} scale={2.6 - 1.6 * slam} opacity={slam}>
      <Rect x={-size * 1.05} y={-size * 0.62} width={size * 2.1} height={size * 1.24} cornerRadius={24} stroke={C.orange} strokeWidth={14} />
      <Text anchorX={0.5} anchorY={0.5} y={-size * 0.06}
        style={{ fontFamily: FONT.display, fontSize: size, fill: solid('#F26A2E22'), stroke: outline }}>0.8.0</Text>
      <Text anchorX={0.5} anchorY={0.5} y={size * 0.44} style={mono(size * 0.075, C.orange, 700)}>{`PROOF  ·  ${PRS.length} PULL REQUESTS`}</Text>
    </Group>
  );
}

export function Open({ width, height }: { width: number; height: number }) {
  const frame = useCurrentFrame();
  if (frame >= LIFT.from + LIFT.length) return null;
  const lift = progress(frame, LIFT.from, LIFT.length, Easings.easeInCubic);
  const { rms } = scoreAt(frame);
  return (
    // The cover page lifts off the sheet: up and turned away about its top edge.
    <Group originX={width / 2} originY={0} perspective={2 * width} rotationX={70 * lift} opacity={1 - lift}>
      <PatternBackground width={width} height={height} pattern="grid" spacing={48} size={1} background="paper" color="ink" opacity={0.08} />
      <Crawl frame={frame} width={width} height={height} />
      <Rect width={width} height={height * 0.34} fill={C.paper} opacity={0.94} />
      <Text x={width / 2} y={height * 0.12} anchorX={0.5} anchorY={0.5} style={mono(width > height ? 30 : 26, C.ink, 700)}>
        {`CELESTA  ·  v0.7.0 → v0.8.0  ·  ${PRS.length} PULL REQUESTS`}
      </Text>
      <Text x={width / 2} y={height * 0.2} anchorX={0.5} anchorY={0.5} opacity={progress(frame, MUSIC_BEAT, MUSIC_BEAT)}
        style={{ fontFamily: FONT.display, fontSize: width > height ? 150 : 130, letterSpacing: { em: 0.04 + 0.04 * rms }, fill: solid(C.ink) }}>
        DELTA
      </Text>
      <Stamp frame={frame} width={width} height={height} />
    </Group>
  );
}
