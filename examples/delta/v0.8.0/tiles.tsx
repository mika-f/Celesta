import { useContext } from 'react';
import type { ReactNode } from 'react';
import { FreezeFrame, Group, Rect, Sequence, Span, Text, useCurrentFrame } from '@celesta.studio/react';
import { Lottie } from '@celesta.studio/lottie';
import { Badge, Panel, PatternBackground } from '@celesta.studio/components';
import { useFitText } from '@celesta.studio/text';
import { Backdrop } from '../components/Backdrop';
import { CUT, STAGE } from '../components/ChapterShell';
import type { Chapter } from '../components/ChapterShell';
import { Delta } from '../components/Delta';
import { C, FONT, ease, solid } from '../components/theme';
import loaderCat from './assets/loader-cat.json';
import { CHAPTERS } from './chapters';
import { FilmContext } from './film-context';
import { CARDS, MoreCard } from './More';
import { PRS } from './prs';
import { scoreAt } from './score';
import { REST, SHEET, TILE, cardLive, cardSlot, featureLive, featureSlot } from './sheet';
import { DeltaTheme } from './theme';
import { CARD_LENGTH, CHAPTER_LENGTH, DURATION, FEATURES, MORE_AT, MUSIC_BEAT, chapterAt } from './timeline';

// What is printed on the sheet: a header, the twelve feature tiles and the
// eight MORE cards, each with crop marks and a caption underneath. Tiles
// are ink panels (Panel, with a shadow on the paper); the paper is a
// PatternBackground grid.

const DEMO_SCALE = 1.2;
const DEMO = { x: TILE.width - 90 - STAGE.width * DEMO_SCALE, y: (TILE.height - STAGE.height * DEMO_SCALE) / 2 };
const SHADOW = { color: '#0C0D0F55', blur: 60, offsetX: 0, offsetY: 30 };
const TITLE_STYLE = { fontFamily: FONT.display, letterSpacing: { em: 0.01 } } as const;
const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;

/** One feature, as a 1920×1080 frame: the name and what it does on the left, its demo large on the right. */
function FeatureBody({ chapter, index }: { chapter: Chapter; index: number }) {
  const frame = useCurrentFrame() - CUT;
  const fit = useFitText(chapter.title, { width: 680, height: 280, minFontSize: 110, maxFontSize: 260, maxLines: 1, style: TITLE_STYLE });
  const cap = fit.fontSize * 0.7;
  const title = ease(frame, -CUT, 22);
  const head = ease(frame, 6, 20);
  const { Demo } = chapter;
  const number = String(index + 1).padStart(2, '0');
  return (
    <Panel width={TILE.width} height={TILE.height} fill="ink" cornerRadius={28} clip>
      <Backdrop />
      <Text x={40} y={TILE.height + 70} anchorY={1}
        style={{ fontFamily: FONT.display, fontSize: 560, fill: solid('#00000000'), stroke: { paint: solid(C.line), width: 3 } }}>
        {number}
      </Text>
      <Group x={90} y={110}>
        <Delta x={12} y={0} size={26} />
        <Text x={40} anchorY={0.5} style={mono(26, C.orange, 700)}>{`Δ${number}`}</Text>
        <Text x={124} anchorY={0.5} style={mono(26, C.grey)}>{chapter.key}</Text>
      </Group>
      <Group x={86} y={160} clip={{ x: -20, y: -20, width: 760, height: cap + 40 }}>
        <Text y={(cap + 40) * (1 - title)} style={{ ...fit.style, fill: solid(C.paper) }}>{chapter.title}</Text>
      </Group>
      <Text x={90} y={160 + cap + 56 + 24 * (1 - head)} opacity={head} maxWidth={660}
        style={{ fontFamily: FONT.sans, fontSize: 50, fontWeight: 700, lineHeight: 62, fill: solid(C.paper) }}>
        {chapter.headline}
      </Text>
      {chapter.merged.slice(0, 4).map(([n, label], i, list) => (
        <Group key={n} opacity={ease(frame, 24 + i * 4, 14)}>
          <Badge x={90} y={TILE.height - 96 - (list.length - i) * 62} fill="raised" stroke="line" cornerRadius="pill"
            padding={{ top: 8, right: 20, bottom: 8, left: 20 }} fontFamily="mono" fontSize={22} color="paper">
            <Span color="orange" fontWeight={700}>{`#${n}`}</Span>{`  ${label}`}
          </Badge>
        </Group>
      ))}
      <Group x={DEMO.x} y={DEMO.y} scale={DEMO_SCALE}>
        <Rect x={-10} y={-10} width={STAGE.width + 20} height={STAGE.height + 20} cornerRadius={16} stroke={C.line} strokeWidth={2} />
        <Group clip={{ width: STAGE.width, height: STAGE.height, cornerRadius: 10 }}>
          <Rect width={STAGE.width} height={STAGE.height} fill={C.panel} />
          <Demo frame={frame} />
        </Group>
      </Group>
    </Panel>
  );
}

/** Crop marks at a tile's corners and its caption below, printed on the paper. */
function Marks({ x, y, caption }: { x: number; y: number; caption: string }) {
  const L = 70;
  const O = 30;
  const corners = [[0, 0, -1, -1], [TILE.width, 0, 1, -1], [0, TILE.height, -1, 1], [TILE.width, TILE.height, 1, 1]] as const;
  return (
    <Group x={x} y={y}>
      {corners.map(([cx, cy, sx, sy], i) => (
        <Group key={i} x={cx + sx * O} y={cy + sy * O}>
          <Rect x={sx < 0 ? -L : 0} y={-1.5} width={L} height={3} fill={C.ink} />
          <Rect x={-1.5} y={sy < 0 ? -L : 0} width={3} height={L} fill={C.ink} />
        </Group>
      ))}
      <Text y={TILE.height + 80} style={mono(40, C.ink, 700)}>{caption}</Text>
    </Group>
  );
}

/**
 * Children drawn as the film looked at `frame`. The film's <Theme> and
 * FilmContext are handed in again inside the <FreezeFrame>.
 */
function Rest({ frame, children }: { frame: number; children: ReactNode }) {
  const Film = useContext(FilmContext);
  return (
    <FreezeFrame frame={frame}>
      <FilmContext.Provider value={Film}><DeltaTheme>{children}</DeltaTheme></FilmContext.Provider>
    </FreezeFrame>
  );
}

/** A feature tile: live from just before the camera lands, else resting on one frame of its chapter. */
export function FeatureTile({ index }: { index: number }) {
  const frame = useCurrentFrame();
  const chapter = CHAPTERS[index];
  const at = chapterAt(index);
  const slot = featureSlot(index);
  const body = (
    <Sequence from={at - CUT}>
      <FeatureBody chapter={chapter} index={index} />
    </Sequence>
  );
  const live = featureLive(index, frame);
  // The rest frame dissolves into the live one over the cut's frames before the downbeat.
  const handoff = live && frame < at ? (at - frame) / CUT : 0;
  return (
    <Group>
      <Marks x={slot.x} y={slot.y} caption={`${String(index + 1).padStart(2, '0')} · ${chapter.title}   frames ${at}–${at + CHAPTER_LENGTH - 1}   ${chapter.merged.map(([n]) => `#${n}`).join(' ')}`} />
      <Group x={slot.x} y={slot.y}>
        <Rect width={TILE.width} height={TILE.height} cornerRadius={28} fill={C.ink} shadow={SHADOW} />
        {live ? body : <Rest frame={at + REST}>{body}</Rest>}
        {handoff > 0 ? <Group opacity={handoff}><Rest frame={at + REST}>{body}</Rest></Group> : null}
      </Group>
    </Group>
  );
}

/** A MORE card on its tile. */
export function CardTile({ index }: { index: number }) {
  const frame = useCurrentFrame();
  const slot = cardSlot(index);
  const start = MORE_AT + index * CARD_LENGTH;
  const f = cardLive(index, frame) ? frame - start : 30;
  return (
    <Group>
      <Marks x={slot.x} y={slot.y} caption={`+${String(index + 1).padStart(2, '0')} · ${CARDS[index].title}   ${CARDS[index].prs.map((n) => `#${n}`).join(' ')}`} />
      <Group x={slot.x} y={slot.y}>
        <Rect width={TILE.width} height={TILE.height} cornerRadius={28} fill={C.ink} shadow={SHADOW} />
        <Panel width={TILE.width} height={TILE.height} cornerRadius={28} clip>
          <MoreCard index={index} f={f} />
        </Panel>
      </Group>
    </Group>
  );
}

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
              fill={level > 0.7 ? C.orange : C.ink} opacity={0.3 + 0.7 * level} />
          </Group>
        );
      })}
    </Group>
  );
}

/** The sheet's header and footer: the release, printed large enough to read when the whole sheet is in view. */
function Header() {
  const frame = useCurrentFrame();
  const { x, y } = SHEET.header;
  const right = SHEET.width - SHEET.margin;
  const info = [
    'v0.7.0  →  v0.8.0',
    `${PRS.length} PULL REQUESTS`,
    `${FEATURES} FEATURES  +  ${CARDS.length} MORE`,
    `90 BPM  ·  ${DURATION} FRAMES`,
  ];
  return (
    <Group>
      <Text x={x - 30} y={y + 880} anchorY="baseline"
        style={{ fontFamily: FONT.display, fontSize: 1080, letterSpacing: { em: 0.01 }, fill: solid(C.ink) }}>
        DELTA <Span style={{ fill: C.orange }}>0.8.0</Span>
      </Text>
      <Text x={x} y={y + 1150} anchorY="baseline" style={{ fontFamily: FONT.sans, fontSize: 210, fontWeight: 700, fill: solid(C.ink) }}>
        Every update is a diff.
      </Text>
      <Group x={right - 2700} y={y + 120}>
        {info.map((line, i) => (
          <Text key={line} y={i * 150} style={mono(104, i === 0 ? C.orange : C.ink, 700)}>{line}</Text>
        ))}
        <Text y={info.length * 150 + 40} style={mono(64, '#0C0D0F99')}>a proof sheet of the release, one frame per feature</Text>
      </Group>
      <Group x={right - 520} y={y + 1080}>
        <Ring frame={frame} size={560} />
        <Delta size={300} fill={C.orange} />
      </Group>
      <Lottie data={loaderCat} x={right - 1500} y={y + 1290} anchorX={0.5} anchorY={1} width={760} />
      <Text x={x} y={SHEET.height - SHEET.margin / 2} anchorY={0.5} style={mono(64, '#0C0D0F99')}>
        {'celesta.natsuneko.cat   ·   github.com/mika-f/Celesta   ·   examples/delta   ·   Loader cat by Diane Picchiottino (LottieFiles, Lottie Simple License)'}
      </Text>
      <Text x={x} y={SHEET.moreTop - 180} style={mono(90, C.ink, 700)}>{`+ MORE   ${CARDS.length} cards, two beats each`}</Text>
      <Text x={right} y={SHEET.moreTop - 180} anchorX={1} style={mono(70, '#0C0D0F99')}>{`frames ${MORE_AT}–${MORE_AT + CARDS.length * CARD_LENGTH - 1}  ·  a beat is ${MUSIC_BEAT} frames`}</Text>
    </Group>
  );
}

/** The paper, and everything printed on it. `visible` skips tiles the camera cannot see. */
export function Sheet({ visible }: { visible: (x: number, y: number, width: number, height: number) => boolean }) {
  return (
    <Group>
      <Rect x={24} y={40} width={SHEET.width} height={SHEET.height} fill="#00000040" />
      <PatternBackground width={SHEET.width} height={SHEET.height} pattern="grid" spacing={100} size={2}
        background="paper" color="ink" opacity={0.07} />
      <Header />
      {CHAPTERS.map((_, i) => {
        const s = featureSlot(i);
        return visible(s.x, s.y, TILE.width, TILE.height + 140) ? <FeatureTile key={i} index={i} /> : null;
      })}
      {CARDS.map((_, k) => {
        const s = cardSlot(k);
        return visible(s.x, s.y, TILE.width, TILE.height + 140) ? <CardTile key={k} index={k} /> : null;
      })}
    </Group>
  );
}
