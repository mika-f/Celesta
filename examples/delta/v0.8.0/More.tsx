import { useContext } from 'react';
import type { ReactNode } from 'react';
import { Easings, FreezeFrame, Group, Loop, Rect, Text, interpolate, progress, useCurrentFrame, useScene, useTextMetrics } from '@celesta.studio/react';
import { chromaticAberration, pixelate } from '@celesta.studio/shader';
import { useFitText } from '@celesta.studio/text';
import { Backdrop } from '../components/Backdrop';
import { Delta } from '../components/Delta';
import { C, FONT, W, ease, solid } from '../components/theme';
import { CAPTIONS } from './captions';
import { FilmContext } from './film-context';
import { useLayout } from './layout';
import { BEAT, MUSIC_BEAT, SCENE_LIST, chapterAt } from './timeline';

// MORE: eight cards for what did not get a chapter, two beats each, each on
// its own tile of the sheet (tiles.tsx). Each lands with a stab in the score; the title slams in through a pixelate
// and a channel split that settle within a beat. Cards alternate ink and
// orange, and every color below comes from the card's palette.
interface Palette {
  /** Main text and strokes. */
  ink: string;
  /** Secondary text. */
  sub: string;
  /** Highlights. */
  accent: string;
  /** Panels drawn on the card. */
  panel: string;
}

const DARK: Palette = { ink: C.paper, sub: C.grey, accent: C.orange, panel: '#FFFFFF10' };
const LIGHT: Palette = { ink: C.ink, sub: '#0C0D0FAA', accent: C.paper, panel: '#0C0D0F1C' };

interface VisualProps { f: number; c: Palette }

interface Card {
  title: string;
  line: string;
  prs: readonly number[];
  Visual: (props: VisualProps) => ReactNode;
}

const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;

/**
 * A frame of the film, small. Only frames that replay nothing themselves:
 * the outro replays this card, so the card must not replay the outro
 * (`<FreezeFrame>` throws on that cycle).
 */
function Thumb({ at, width, height }: { at: number; width: number; height: number }) {
  const Film = useContext(FilmContext);
  return (
    <Group clip={{ width, height, cornerRadius: 4 }}>
      <Group scale={width / W}><FreezeFrame frame={at}><Film /></FreezeFrame></Group>
    </Group>
  );
}

/**
 * The film's scenes on this very card (#419). The HUD reads them with
 * useScene() at the film's root; the card reads them on the film's clock
 * too (#447), wherever its tile is drawn: live, or resting on the sheet.
 */
function Scenes({ c }: VisualProps) {
  const now = useScene(SCENE_LIST, { clock: 'root' });
  const rows = SCENE_LIST.scenes;
  return (
    <Group y={-40}>
      {rows.map((scene, i) => {
        const on = scene.id === now?.id;
        return (
          <Group key={scene.id} y={i * 27} opacity={on ? 1 : 0.55}>
            <Text style={mono(17, on ? c.accent : c.sub, on ? 700 : 400)}>{String(scene.from).padStart(4, ' ')}</Text>
            <Text x={64} style={mono(17, c.ink, on ? 700 : 400)}>{scene.name}</Text>
            {on ? <Rect x={250} y={6} width={200 * (now?.progress ?? 0)} height={8} fill={c.accent} /> : null}
          </Group>
        );
      })}
      <Text x={480} y={0} style={mono(18, c.ink, 700)}>useScene(SCENE_LIST,</Text>
      <Text x={480} y={24} style={mono(18, c.ink, 700)}>{"  { clock: 'root' })"}</Text>
      <Text x={480} y={58} style={mono(17, c.sub)}>{`name      ${now?.name ?? '-'}`}</Text>
      <Text x={480} y={84} style={mono(17, c.sub)}>{`index     ${now?.index ?? '-'}`}</Text>
      <Text x={480} y={110} style={mono(17, c.sub)}>{`progress  ${(now?.progress ?? 0).toFixed(2)}`}</Text>
    </Group>
  );
}

/** A beat that starts over: <Loop> resets its children's clock every beat, MUSIC_BEAT frames (#412). */
function Beat({ c }: { c: Palette }) {
  const frame = useCurrentFrame();
  const p = frame / MUSIC_BEAT;
  return (
    <Group>
      <Rect x={-6} y={-150} width={12} height={150} cornerRadius={6} fill={c.panel} />
      <Group rotation={360 * p}><Rect x={-4} y={-140} width={8} height={140} cornerRadius={4} fill={c.accent} /></Group>
      <Text y={190} anchorX={0.5} style={mono(22, c.ink, 700)}>{`frame ${frame}`}</Text>
    </Group>
  );
}

function Loops({ f, c }: VisualProps) {
  return (
    <Group x={160} y={200} opacity={ease(f, 2, 10)}>
      <Rect x={-170} y={-170} width={340} height={340} cornerRadius={170} stroke={c.ink} strokeWidth={4} />
      <Loop durationInFrames={MUSIC_BEAT}><Beat c={c} /></Loop>
      <Text x={260} y={-120} style={mono(20, c.ink, 700)}>{'<Loop durationInFrames={20}>'}</Text>
      <Text x={260} y={-84} style={mono(17, c.sub)}>the clock, media and subtitles</Text>
      <Text x={260} y={-58} style={mono(17, c.sub)}>inside start over each cycle</Text>
      <Text x={260} y={-10} style={mono(17, c.sub)}>{'times={2.5} cuts the last one short'}</Text>
    </Group>
  );
}

/** A drag on the canvas, written back to the JSX: drawn. */
function Editor({ f, c }: VisualProps) {
  const t = progress(f, 4, 20, Easings.easeInOutCubic);
  const x = 40 + 220 * t;
  return (
    <Group>
      <Rect width={700} height={250} cornerRadius={10} fill={c.panel} />
      <Rect x={x} y={70} width={170} height={110} cornerRadius={8} fill={c.accent} />
      <Rect x={x - 4} y={66} width={178} height={118} stroke={c.ink} strokeWidth={2} dash={[8, 6]} />
      {[[0, 0], [1, 0], [0, 1], [1, 1]].map(([u, v]) => (
        <Rect key={`${u}${v}`} x={x - 9 + u * 178} y={61 + v * 118} width={10} height={10} fill={c.ink} />
      ))}
      <Group y={290}>
        <Text style={mono(20, c.sub)}>{'<Rect x={'}</Text>
        <Text x={110} style={mono(20, c.ink, 700)}>{String(Math.round(x))}</Text>
        <Text x={160} style={mono(20, c.sub)}>{'} y={70} … />'}</Text>
        <Text y={40} opacity={ease(f, 24, 8)} style={mono(17, c.sub)}>saved to film.tsx · Cmd+Z undoes it</Text>
      </Group>
    </Group>
  );
}

const STILLS = [
  ['poster.png', 'lossless'],
  ['poster.jpg', '--quality 85'],
  ['poster.webp', '--quality 85'],
] as const;

function Stills({ f, c }: VisualProps) {
  const size = { width: 208, height: 117 };
  return (
    <Group>
      {STILLS.map(([file, note], i) => {
        const p = ease(f, 3 + i * 4, 12);
        return (
          <Group key={file} x={i * (size.width + 30)} y={40 + 20 * (1 - p)} opacity={p}>
            <Rect x={-3} y={-3} width={size.width + 6} height={size.height + 6} cornerRadius={6} fill={c.ink} />
            <Thumb at={chapterAt(0) + 100} {...size} />
            <Text y={size.height + 30} style={mono(20, c.ink, 700)}>{file}</Text>
            <Text y={size.height + 58} style={mono(17, c.sub)}>{note}</Text>
          </Group>
        );
      })}
      <Text y={300} opacity={ease(f, 12, 12)} style={mono(20, c.ink, 700)}>Celesta-export --frame 0 poster.webp</Text>
      <Text y={334} opacity={ease(f, 14, 12)} style={mono(18, c.sub)}>{'MCP: render_frames { "format": "jpeg" }'}</Text>
    </Group>
  );
}

const SUBTITLE_FILES = [
  ['delta-0.8.0.ja.srt', CAPTIONS.ja],
  ['delta-0.8.0.en.srt', CAPTIONS.en],
  ['delta-0.8.0.ja.vtt', CAPTIONS.ja],
  ['delta-0.8.0.en.vtt', CAPTIONS.en],
] as const;

function SubtitleFiles({ f, c }: VisualProps) {
  return (
    <Group>
      {SUBTITLE_FILES.map(([file, cues], i) => {
        const p = ease(f, 3 + i * 3, 10);
        const text = String(cues[1].text);
        return (
          <Group key={file} y={i * 78} opacity={p} x={24 * (1 - p)}>
            <Rect width={700} height={64} cornerRadius={8} fill={c.panel} />
            <Text x={18} y={22} anchorY={0.5} style={mono(19, c.ink, 700)}>{file}</Text>
            <Text x={18} y={46} anchorY={0.5} style={mono(15, c.sub)}>{text.length > 46 ? `${text.slice(0, 45)}…` : text}</Text>
          </Group>
        );
      })}
      <Text y={340} opacity={ease(f, 14, 10)} style={mono(18, c.ink, 700)}>{"--subtitles 'delta-0.8.0.{lang}.srt' --subtitles '….vtt'"}</Text>
      <Text y={372} opacity={ease(f, 16, 10)} style={mono(17, c.sub)}>no video output: only the subtitles are written</Text>
    </Group>
  );
}

function Hdr({ f, c }: VisualProps) {
  // The curve's shape, drawn: linear light (nits) to an 8-bit sRGB code.
  const plot = { width: 330, height: 260 };
  const draw = ease(f, 3, 20);
  const curve = (nits: number) => 255 * (1 - Math.exp(-nits / 160)) / (1 - Math.exp(-1000 / 160));
  const points = Array.from({ length: 41 }, (_, i) => {
    const nits = (i / 40) * 1000 * draw;
    return [(nits / 1000) * plot.width, plot.height - (curve(nits) / 255) * plot.height] as const;
  });
  const phone = { width: 117, height: 208 };
  const turn = ease(f, 8, 16);
  return (
    <Group y={20}>
      <Rect y={plot.height} width={plot.width} height={2} fill={c.ink} />
      <Rect width={2} height={plot.height} fill={c.ink} />
      {points.slice(1).map(([x, y], i) => {
        const [px, py] = points[i];
        const length = Math.hypot(x - px, y - py);
        return <Rect key={i} x={px} y={py} width={length + 1} height={4} rotation={(Math.atan2(y - py, x - px) * 180) / Math.PI} fill={c.accent} />;
      })}
      <Text y={plot.height + 30} style={mono(17, c.sub)}>HLG / PQ  →  SDR, as Chrome draws it</Text>
      <Text y={plot.height + 58} style={mono(17, c.sub)}>203 nits → sRGB 188</Text>
      <Group x={470} y={40}>
        <Group x={phone.height / 2} y={phone.height / 2} rotation={-90 * (1 - turn)}>
          <Rect x={-phone.width / 2} y={-phone.height / 2} width={phone.width} height={phone.height} cornerRadius={14} stroke={c.ink} strokeWidth={4} fill={c.panel} />
          <Delta size={44} fill={c.accent} />
        </Group>
        <Text y={phone.height + 50} style={mono(17, c.ink, 700)}>rotated video</Text>
        <Text y={phone.height + 78} style={mono(17, c.sub)}>probed at 1080×1920</Text>
      </Group>
    </Group>
  );
}

// Each pair is a PR's own before / after, on its own scale.
const SPEEDS: readonly (readonly [string, number, number, string])[] = [
  ['#394 web perspective plane', 131, 37, 'ms / frame'],
  ['#390 flat layer transform', 18.4, 7.8, 'ms / 1M'],
];

function Faster({ f, c }: VisualProps) {
  return (
    <Group>
      {SPEEDS.map(([label, before, after, unit], i) => {
        const p = ease(f, 3 + i * 6, 16);
        const full = 640;
        return (
          <Group key={label} y={20 + i * 170} opacity={p}>
            <Text style={mono(20, c.ink, 700)}>{label}</Text>
            <Rect y={36} width={full} height={34} cornerRadius={4} fill={c.panel} />
            <Text x={full + 14} y={53} anchorY={0.5} style={mono(18, c.sub)}>{`${before} ${unit}`}</Text>
            <Rect y={80} width={full * (after / before) * p} height={34} cornerRadius={4} fill={c.accent} />
            <Text x={full * (after / before) + 14} y={97} anchorY={0.5} style={mono(18, c.ink, 700)}>{`${after} ${unit}`}</Text>
          </Group>
        );
      })}
      <Text y={370} opacity={ease(f, 14, 12)} style={mono(16, c.sub)}>{"Each PR's own measurement, on its own scale."}</Text>
    </Group>
  );
}

const FIX_LIST: readonly (readonly [number, string])[] = [
  [429, 'lines align without maxWidth'],
  [430, 'web text measures late fonts'],
  [386, 'spectrum ceiling follows the file'],
  [389, 'rotated videos probe upright'],
  [385, 'clippy passes on Rust 1.99'],
  [395, 'renderFps includes encoding'],
];

function Fixes({ f, c }: VisualProps) {
  return (
    <Group>
      {FIX_LIST.map(([n, text], i) => {
        const p = ease(f, 2 + i * 3, 10);
        return (
          <Group key={n} y={20 + i * 62} opacity={p} x={24 * (1 - p)}>
            <Rect y={-2} width={30} height={30} cornerRadius={6} fill={c.ink} />
            <Text x={15} y={13} anchorX={0.5} anchorY={0.5} style={mono(20, c === DARK ? C.ink : C.orange, 700)}>x</Text>
            <Text x={50} style={mono(26, c.accent, 700)}>{`#${n}`}</Text>
            <Text x={150} style={mono(26, c.ink)}>{text}</Text>
          </Group>
        );
      })}
    </Group>
  );
}

export const CARDS: readonly Card[] = [
  { title: 'SCENES', line: 'Name the chapters once; useScene() says where you are. The frame chrome does.', prs: [419], Visual: Scenes },
  { title: 'LOOP', line: 'Loop repeats its children, their media and their clock.', prs: [412], Visual: Loops },
  { title: 'EDITOR', line: 'Drag a layer on the canvas, and the JSX changes with it.', prs: [417], Visual: Editor },
  { title: 'STILLS', line: 'Stills as JPEG or WebP, from the exporter and the MCP server.', prs: [381, 382], Visual: Stills },
  { title: 'SUBTITLES', line: 'Every language and format in one export, or the subtitles alone.', prs: [384], Visual: SubtitleFiles },
  { title: 'HDR VIDEO', line: 'HDR footage tone mapped to SDR, and phone videos the right way up.', prs: [388, 389], Visual: Hdr },
  { title: 'FASTER', line: 'Perspective planes in the browser, and every flat layer.', prs: [394, 390, 391], Visual: Faster },
  { title: 'FIXES', line: 'And the things that were wrong.', prs: FIX_LIST.map(([n]) => n), Visual: Fixes },
];

/** Strings with Japanese in them, for the font subset. */
export const MORE_STRINGS: readonly string[] = SUBTITLE_FILES.map(([, cues]) => String(cues[1].text));

const TITLE_STYLE = { fontFamily: FONT.display } as const;
const LINE_STYLE = { fontFamily: FONT.sans, fontSize: 42, fontWeight: 700, lineHeight: 54 } as const;
const START_SPACING = 0.12;

/**
 * Where a card's parts go. The film: copy on the left, the visual on the
 * right. The short: copy stacked in the safe area, the visual below it, and
 * the title fitted to the safe area's width.
 */
function useCardLayout(card: Card) {
  const layout = useLayout();
  const width = layout.width - layout.safe.left - layout.safe.right - 48;
  // Fitted at rest; while it slams in, a long title may run past the safe area.
  const fit = useFitText(card.title, { width, height: 260, minFontSize: 90, maxFontSize: 260, maxLines: 1, style: TITLE_STYLE });
  const copy = useTextMetrics(card.line, LINE_STYLE, { maxWidth: width });
  if (!layout.portrait) {
    return { layout, x: 120, label: 150, title: 190, size: 220, line: 440, lineWidth: 760, prs: 960, visual: { x: 1020, y: 420, scale: 1 } };
  }
  const x = layout.safe.left + 24;
  const top = layout.safe.top + 60;
  const line = top + 40 + fit.fontSize * 1.05;
  return {
    layout, x, label: top, title: top + 40, size: fit.fontSize, line, lineWidth: width,
    // The visuals are about 700 px wide: at 1.15 this centres them on the frame.
    prs: layout.height - layout.safe.bottom - 50, visual: { x: (layout.width - 700 * 1.15) / 2, y: line + copy.height + 120, scale: 1.15 },
  };
}

/** MORE card `index`, `f` frames after it lands; each is laid on its own tile of the sheet. */
export function MoreCard({ index, f }: { index: number; f: number }) {
  const card = CARDS[index];
  const orange = index % 2 === 1;
  const c = orange ? LIGHT : DARK;
  const at = useCardLayout(card);
  const hit = 1 - progress(f, 0, BEAT, Easings.easeOutCubic);
  const title = { ...TITLE_STYLE, fontSize: at.size, fill: solid(c.ink) } as const;
  const spacing = interpolate(f, [0, 16], [START_SPACING, 0.02], { easing: Easings.easeOutCubic, extrapolateRight: 'clamp' });
  const line = ease(f, 4, 12);
  const { Visual } = card;
  return (
    <Group>
      <Backdrop fill={orange ? C.orange : C.ink} dots={orange ? '#0C0D0F18' : '#FFFFFF14'} width={at.layout.width} height={at.layout.height} />
      <Group shader={[pixelate(1 + 26 * hit ** 2), chromaticAberration({ offset: 22 * hit })]}>
        <Group x={at.x} y={at.label}>
          <Delta x={10} y={0} size={22} fill={orange ? C.ink : C.orange} />
          <Text x={34} y={0} anchorY={0.5} style={mono(22, orange ? C.ink : C.orange, 700)}>
            {`MORE  ${String(index + 1).padStart(2, '0')} / ${CARDS.length}`}
          </Text>
        </Group>
        <Text x={at.x - 4} y={at.title} style={{ ...title, letterSpacing: { em: spacing } }}>{card.title}</Text>
        <Text x={at.x} y={at.line} opacity={line} maxWidth={at.lineWidth}
          style={{ ...LINE_STYLE, fill: solid(c.ink) }}>{card.line}</Text>
        <Text x={at.x} y={at.prs} opacity={line} style={mono(22, c.sub)}>
          {card.prs.map((n) => `#${n}`).join('  ·  ')}
        </Text>
        <Group x={at.visual.x} y={at.visual.y} scale={at.visual.scale}>
          <Visual f={f} c={c} />
        </Group>
      </Group>
    </Group>
  );
}
