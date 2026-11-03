import { useContext } from 'react';
import type { ReactNode } from 'react';
import { Easings, FreezeFrame, Group, Rect, Text, formatSubtitles, interpolate, parseSubtitles, progress, useCurrentFrame, useTextMetrics } from '@celesta.studio/react';
import { safeAreaInsets } from '@celesta.studio/layout';
import { chromaticAberration, pixelate } from '@celesta.studio/shader';
import { useFitText } from '@celesta.studio/text';
import { Backdrop } from '../components/Backdrop';
import { CUT } from '../components/ChapterShell';
import { Delta } from '../components/Delta';
import { C, FONT, FPS, W, ease, solid } from '../components/theme';
import { FilmContext } from './film-context';
import { useLayout } from './layout';
import { BAR, BEAT, DURATION, chapterAt } from './timeline';

// MORE: eight one-bar cards for what did not get a chapter. Each lands on a
// downbeat with a stab in the score; the title slams in through a pixelate
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

function Compositions({ f, c }: VisualProps) {
  const p = ease(f, 4, 14);
  const q = ease(f, 10, 14);
  return (
    <Group>
      <Group opacity={p} y={20 * (1 - p)}>
        <Rect y={60} width={384} height={216} stroke={c.ink} strokeWidth={4} cornerRadius={8} fill={c.panel} />
        <Text x={192} y={168} anchorX={0.5} anchorY={0.5} style={mono(26, c.ink, 700)}>id="film"</Text>
        <Text y={310} style={mono(20, c.sub)}>{`1920×1080 · ${FPS} fps · ${DURATION} frames`}</Text>
      </Group>
      <Group opacity={q} y={20 * (1 - q)}>
        <Rect x={440} y={60} width={216} height={122} stroke={c.accent} strokeWidth={4} cornerRadius={8} fill={c.panel} />
        <Text x={548} y={121} anchorX={0.5} anchorY={0.5} style={mono(22, c.accent, 700)}>id="poster"</Text>
        <Text x={440} y={216} style={mono(20, c.sub)}>the outro, still</Text>
      </Group>
      <Text y={390} opacity={q} style={mono(22, c.ink, 700)}>Celesta-export --composition poster</Text>
    </Group>
  );
}

function GoogleFonts({ f, c }: VisualProps) {
  const rows = [
    ['Bebas Neue', { fontFamily: 'Bebas Neue', fontSize: 96 }],
    ['Space Grotesk', { fontFamily: 'Space Grotesk', fontSize: 60, fontWeight: 700 }],
    ['JetBrains Mono', { fontFamily: 'JetBrains Mono', fontSize: 50, fontWeight: 700 }],
  ] as const;
  return (
    <Group>
      {rows.map(([name, style], i) => {
        const p = ease(f, 3 + i * 4, 12);
        return (
          <Group key={name} y={40 + i * 140} opacity={p} x={30 * (1 - p)}>
            <Text style={mono(20, c.sub)}>{`<Font google="${name}" />`}</Text>
            <Text y={34} style={{ ...style, fill: solid(i === 0 ? c.accent : c.ink) }}>{name}</Text>
          </Group>
        );
      })}
    </Group>
  );
}

// The SUBTITLES card's sample: an English SRT timed to the SPECTRUM chapter,
// written with formatSubtitles(), read back with parseSubtitles(), and drawn
// over that chapter playing (at four times speed, so both cues fit in the
// card's bar).
const SAMPLE_AT = chapterAt(8);
const SAMPLE_SRT = formatSubtitles([
  { from: SAMPLE_AT, durationInFrames: 2 * BAR, text: "These bars are this film's own score." },
  { from: SAMPLE_AT + 2 * BAR, durationInFrames: 2 * BAR, text: '32 bands and the loudness, frame by frame.' },
], { fps: FPS, format: 'srt' });
const SAMPLE_CUES = parseSubtitles(SAMPLE_SRT, { fps: FPS });
const SPEED = 4;
const PREVIEW_SIZE = { width: 600, height: 338 };

function SubtitleFile({ f, c }: VisualProps) {
  const Film = useContext(FilmContext);
  const at = SAMPLE_AT + Math.max(0, f) * SPEED;
  const current = SAMPLE_CUES.findIndex((cue) => cue.from <= at && at < cue.from + cue.durationInFrames);
  const lines = SAMPLE_SRT.trim().split('\n');
  const scale = PREVIEW_SIZE.width / W;
  return (
    <Group y={-60}>
      {/* The cues on the film, as <Sequence>s of the parsed frames would show them. */}
      <Group opacity={ease(f, 2, 10)}>
        <Rect x={-3} y={-3} width={PREVIEW_SIZE.width + 6} height={PREVIEW_SIZE.height + 6} cornerRadius={8} fill={c.ink} />
        <Group clip={{ width: PREVIEW_SIZE.width, height: PREVIEW_SIZE.height, cornerRadius: 6 }}>
          <Group scale={scale}><FreezeFrame frame={at}><Film /></FreezeFrame></Group>
          {current >= 0 ? (
            <Group>
              <Rect x={60} y={PREVIEW_SIZE.height - 64} width={PREVIEW_SIZE.width - 120} height={42} cornerRadius={6} fill="#000000B8" />
              <Text x={PREVIEW_SIZE.width / 2} y={PREVIEW_SIZE.height - 43} anchorX={0.5} anchorY={0.5}
                style={{ fontFamily: FONT.sans, fontSize: 20, fontWeight: 500, fill: solid('#FFFFFF') }}>
                {SAMPLE_CUES[current].text}
              </Text>
            </Group>
          ) : null}
          <Text x={PREVIEW_SIZE.width - 14} y={14} anchorX={1} style={mono(14, '#FFFFFFAA')}>{`${SPEED}×`}</Text>
        </Group>
      </Group>
      <Group y={PREVIEW_SIZE.height + 30}>
        <Text opacity={ease(f, 2, 10)} style={mono(18, c.sub)}>captions.srt  →  parseSubtitles(srt, {'{'} fps: 30 {'}'})</Text>
        {lines.map((line, i) => {
          const cue = i < 4 ? 0 : 1;
          const on = cue === current;
          return (
            <Text key={i} y={32 + i * 27} opacity={ease(f, 3 + i * 2, 10) * (on ? 1 : 0.45)}
              style={mono(19, i % 4 === 1 ? c.accent : c.ink, on && i % 4 === 2 ? 700 : 400)}>{line}</Text>
          );
        })}
      </Group>
    </Group>
  );
}

function SafeAreas({ f, c }: VisualProps) {
  const inset = safeAreaInsets('vertical-social', 1080, 1920);
  const s = 0.24;
  const p = ease(f, 4, 16);
  return (
    <Group x={20} y={-20}>
      <Rect width={1080 * s} height={1920 * s} cornerRadius={24} stroke={c.ink} strokeWidth={4} fill={c.panel} />
      <Group opacity={p}>
        <Rect x={(1080 - inset.right) * s} y={inset.top * s} width={inset.right * s} height={(1920 - inset.top - inset.bottom) * s} fill="#0C0D0F40" />
        <Rect y={(1920 - inset.bottom) * s} width={1080 * s} height={inset.bottom * s} cornerRadius={24} fill="#0C0D0F40" />
        <Rect x={inset.left * s} y={inset.top * s} width={(1080 - inset.left - inset.right) * s}
          height={(1920 - inset.top - inset.bottom) * s} stroke={c.accent} strokeWidth={4} dash={[12, 8]} dashOffset={-f * 2} />
      </Group>
      <Group x={330} y={60} opacity={p}>
        {[['top', inset.top], ['right', inset.right], ['bottom', inset.bottom], ['left', inset.left]].map(([edge, px], i) => (
          <Text key={edge} y={i * 50} style={mono(28, c.ink, 700)}>{`${String(edge).padEnd(7)} ${Math.round(Number(px))} px`}</Text>
        ))}
        <Text y={230} style={mono(20, c.sub)}>{"padding='vertical-social'"}</Text>
        <Text y={262} style={mono(20, c.sub)}>at 1080×1920</Text>
      </Group>
    </Group>
  );
}

const MORAS = ['ア', 'イ', 'ヴィ', 'ス', 'ス', 'ピー', 'チ'];

function Moras({ f, c }: VisualProps) {
  const lit = Math.floor(f / 5);
  return (
    <Group>
      <Group y={110}>
        {MORAS.map((mora, i) => {
          const on = i <= lit;
          return (
            <Group key={i} x={i * 100}>
              <Rect width={88} height={88} cornerRadius={10} fill={on ? c.accent : c.panel} stroke={c.ink} strokeWidth={on ? 0 : 2} />
              <Text x={44} y={46} anchorX={0.5} anchorY={0.5}
                style={{ fontFamily: FONT.sans, fontSize: 34, fontWeight: 700, fill: solid(on ? (c === DARK ? C.ink : C.orange) : c.ink) }}>{mora}</Text>
            </Group>
          );
        })}
      </Group>
      <Text y={260} style={mono(20, c.sub)}>timingFromAivisSpeech(query, wav).moras</Text>
      <Text y={292} style={mono(20, c.sub)}>timingFromVoicevox(query).moras</Text>
    </Group>
  );
}

function React19({ f, c }: VisualProps) {
  const spin = f * 3;
  return (
    <Group x={260} y={200}>
      {[0, 60, 120].map((angle) => (
        <Rect key={angle} anchorX={0.5} anchorY={0.5} width={380} height={130} cornerRadius={65}
          rotation={angle + spin} stroke={c.ink} strokeWidth={6} />
      ))}
      <Text anchorX={0.5} anchorY={0.5} style={{ fontFamily: FONT.display, fontSize: 130, fill: solid(c.accent) }}>19</Text>
    </Group>
  );
}

function Website({ f, c }: VisualProps) {
  const p = ease(f, 3, 16);
  return (
    <Group opacity={p} y={20 * (1 - p)}>
      <Rect width={680} height={380} cornerRadius={12} fill="#0C0D0F" stroke={c.ink} strokeWidth={3} />
      <Rect width={680} height={40} cornerRadius={12} fill="#1E2026" />
      {[C.red, C.amber, C.green].map((color, i) => <Rect key={color} x={20 + i * 22} y={14} width={12} height={12} cornerRadius={6} fill={color} />)}
      <Text x={110} y={20} anchorY={0.5} style={mono(17, C.grey)}>celesta.natsuneko.cat</Text>
      <Text x={40} y={130} style={{ fontFamily: FONT.display, fontSize: 96, fill: solid(C.paper) }}>CELESTA</Text>
      <Text x={40} y={214} style={{ fontFamily: FONT.sans, fontSize: 26, fontWeight: 500, fill: solid(C.grey) }}>{WEBSITE_LINE}</Text>
      <Rect x={40} y={284} width={170} height={48} cornerRadius={24} fill={C.orange} />
      <Text x={125} y={308} anchorX={0.5} anchorY={0.5} style={mono(18, C.ink, 700)}>Download</Text>
    </Group>
  );
}
const WEBSITE_LINE = 'Videos from code. Docs in English and 日本語.';

const FIX_LIST: readonly (readonly [number, string])[] = [
  [275, 'Windows: startup, title bar, paths'],
  [331, 'bash: words and flags kept whole'],
  [347, 'a missing media file is named'],
  [353, 'web gradients match native'],
  [358, 'a faded group fades as one'],
  [337, 'the template list scrolls'],
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
  { title: 'COMPOSITIONS', line: 'One entry, several videos. This one has two.', prs: [340], Visual: Compositions },
  { title: 'GOOGLE FONTS', line: 'Ask for a family by name. Every Latin face here is.', prs: [341], Visual: GoogleFonts },
  { title: 'SUBTITLES', line: 'SRT and WebVTT in, --subtitles out. This film writes its own, in two languages.', prs: [342], Visual: SubtitleFile },
  { title: 'SAFE AREAS', line: 'Insets for Shorts, TikTok and Reels, guides in PNG checks.', prs: [281, 220], Visual: SafeAreas },
  { title: 'AIVISSPEECH', line: 'Mora and phoneme timing from VOICEVOX and AivisSpeech.', prs: [274], Visual: Moras },
  { title: 'REACT 19', line: 'The workspace and every example on one React.', prs: [330, 345, 361], Visual: React19 },
  { title: 'WEBSITE', line: 'Rebuilt with Makit, theme Noir, the README pointing to it.', prs: [323, 360], Visual: Website },
  { title: 'FIXES', line: 'And the things that were wrong.', prs: [275, 331, 347, 353, 358, 337], Visual: Fixes },
];

/** Strings with Japanese in them, for the font subset. */
export const MORE_STRINGS = [...MORAS, WEBSITE_LINE];

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

function CardView({ card, index, f }: { card: Card; index: number; f: number }) {
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

export function More() {
  const frame = useCurrentFrame() - CUT;
  const index = Math.max(0, Math.min(CARDS.length - 1, Math.floor(frame / BAR)));
  return <CardView card={CARDS[index]} index={index} f={frame - index * BAR} />;
}
