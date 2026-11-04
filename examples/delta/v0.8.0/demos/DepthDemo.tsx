import { Easings, Group, Rect, Text, interpolate } from '@celesta.studio/react';
import type { DemoProps } from '../../components/ChapterShell';
import { STAGE, enter } from '../../components/ChapterShell';
import { Delta } from '../../components/Delta';
import { C, FONT, solid } from '../../components/theme';
import { PRS } from '../prs';
import { BAR, BEAT } from '../timeline';

// Top left: a card turning over on rotationY, its back drawn with
// backfaceVisibility="hidden" on both faces. Top right: bars leaning on
// skewX. Below: this release's pull requests as a crawl, one text plane
// tipped back with rotationX and a perspective, drawn in density bands (#393).
const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;
const CARD = { width: 300, height: 200 };

function Face({ back }: { back?: boolean }) {
  return (
    <Group x={-CARD.width / 2} y={-CARD.height / 2}>
      <Rect width={CARD.width} height={CARD.height} cornerRadius={14} fill={back ? C.paper : C.orange} />
      {back ? (
        <Text x={CARD.width / 2} y={CARD.height / 2} anchorX={0.5} anchorY={0.5}
          style={{ fontFamily: FONT.display, fontSize: 92, fill: solid(C.ink) }}>0.8.0</Text>
      ) : (
        <Delta x={CARD.width / 2} y={CARD.height / 2} size={110} fill={C.ink} />
      )}
    </Group>
  );
}

function Card({ frame }: { frame: number }) {
  // Half a turn every two beats, easing into each face.
  const turns = Math.floor(Math.max(0, frame) / (2 * BEAT));
  const t = interpolate(frame % (2 * BEAT), [0, 8], [0, 1], { easing: Easings.easeInOutCubic, extrapolateRight: 'clamp' });
  const angle = (turns + (frame >= 0 ? t : 0)) * 180;
  const tilt = 14 * Math.sin(frame / 20);
  return (
    <Group x={200} y={170} perspective={900} rotationY={angle} rotationX={tilt}>
      <Group backfaceVisibility="hidden"><Face /></Group>
      <Group rotationY={180} backfaceVisibility="hidden"><Face back /></Group>
    </Group>
  );
}

function Skews({ frame }: { frame: number }) {
  return (
    <Group x={440} y={60}>
      {Array.from({ length: 6 }, (_, i) => {
        const skew = 34 * Math.sin(frame / 9 - i * 0.5);
        return (
          <Group key={i} x={40 + i * 48} skewX={skew}>
            <Rect y={0} width={36} height={220} cornerRadius={4} fill={i % 2 ? C.dim : C.paper} />
          </Group>
        );
      })}
      <Text y={260} style={mono(18, C.grey)}>{`skewX={${Math.round(34 * Math.sin(frame / 9))}}`}</Text>
    </Group>
  );
}

const ROW = 54;

/** A title cut to `columns` monospace columns; CJK characters take two. */
function clip(title: string, columns: number) {
  let used = 0;
  for (let i = 0; i < title.length; i++) {
    used += title.charCodeAt(i) > 0x2000 ? 2 : 1;
    if (used > columns - 1) return `${title.slice(0, i)}…`;
  }
  return title;
}

function Crawl({ frame }: { frame: number }) {
  // The list rides away from the viewer: from its first rows at the near
  // edge to its last, over the chapter.
  const scroll = interpolate(frame, [-10, 4 * BAR], [ROW * 4, ROW * (PRS.length + 2)], { extrapolateLeft: 'clamp' });
  // The plane's origin is its near edge; it recedes to a horizon above.
  return (
    <Group x={STAGE.width / 2} y={STAGE.height - 6} rotationX={58} perspective={900}>
      <Group y={-scroll}>
        {PRS.map(([n, title], i) => {
          // Rows still nearer than the edge would pass the eye: not drawn yet.
          const y = i * ROW - scroll;
          if (y > ROW) return null;
          return (
            <Group key={n} y={i * ROW} opacity={Math.min(1, (ROW - y) / ROW)}>
              <Text x={-400} anchorY={0.5} style={mono(32, C.orange, 700)}>{`#${n}`}</Text>
              <Text x={-290} anchorY={0.5} style={mono(32, C.paper)}>{clip(title, 36)}</Text>
            </Group>
          );
        })}
      </Group>
    </Group>
  );
}

export function DepthDemo({ frame }: DemoProps) {
  return (
    <Group>
      <Group opacity={enter(frame, 0)}>
        <Card frame={frame} />
        <Text x={200} y={318} anchorX={0.5} style={mono(18, C.grey)}>{'rotationY · backfaceVisibility'}</Text>
      </Group>
      <Group opacity={enter(frame, 4)}><Skews frame={frame} /></Group>
      <Group opacity={enter(frame, BEAT)} clip={{ y: 360, width: STAGE.width, height: STAGE.height - 360 }}>
        <Rect y={360} width={STAGE.width} height={STAGE.height - 360} fill={C.ink} />
        <Crawl frame={frame} />
        <Text x={STAGE.width - 20} y={384} anchorX={1} style={mono(18, C.grey)}>{'rotationX={58} perspective={900}'}</Text>
      </Group>
    </Group>
  );
}
