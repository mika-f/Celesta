// "36 Days." A 44-second data film about how Celesta was built, drawn from
// this repository's own Git history. 120 BPM, 30 fps: one beat is 15 frames,
// one bar is 60, and every scene starts on a downbeat.
// Open this file in Celesta, or export it with:
//   Celesta-export --react examples/36-days/film.tsx 36-days.mp4
import { noise, random } from '@celesta/math';
import type { ReactNode } from 'react';

import {
  Assets,
  Audio,
  Camera,
  Composition,
  Easings,
  Font,
  Group,
  Line,
  Polyline,
  Rect,
  Series,
  Stagger,
  Text,
  TextReveal,
  Transition,
  computeSeries,
  cueAt,
  frameToTimecode,
  interpolate,
  pointOnPolyline,
  progress,
  useBeat,
  useCountUp,
  useCue,
  useCurrentFrame,
  useTypewriter,
  useVideoConfig,
} from '@celesta/react';
import type { PolylinePoint, TextStyle } from '@celesta/react';

const W = 1920;
const H = 1080;
const FPS = 30;
const BPM = 120;
const BEAT = 15;
const BAR = BEAT * 4;

const C = {
  ink: '#0A0F0D',
  line: '#FFFFFF1F',
  paper: '#ECEFE8',
  soft: '#B4BDB6',
  grey: '#7D867F',
  dim: '#26302B',
  mint: '#7CF29C',
  coral: '#FF6B4A',
} as const;

const FONT = {
  display: 'Archivo Black',
  mono: 'JetBrains Mono',
  ja: 'Noto Sans JP',
} as const;
// JetBrains Mono advances every glyph by 0.6 em, so mono text can be measured.
const MONO_ADVANCE = 0.6;

// ── Data ─────────────────────────────────────────────────────────────────
// git log --no-merges --date=short --format=%ad | sort | uniq -c
const FIRST_DAY = Date.UTC(2026, 7, 25);
const COMMITS_BY_DATE: Record<string, number> = {
  '2026-08-25': 25, '2026-08-26': 6, '2026-08-28': 11, '2026-08-29': 7,
  '2026-08-30': 10, '2026-08-31': 7, '2026-09-07': 21, '2026-09-08': 7,
  '2026-09-24': 4, '2026-09-25': 17, '2026-09-26': 1, '2026-09-28': 15,
  '2026-09-29': 6,
};
const DAYS = Array.from({ length: 36 }, (_, i) => {
  const date = new Date(FIRST_DAY + i * 86_400_000).toISOString().slice(0, 10);
  return { date, commits: COMMITS_BY_DATE[date] ?? 0 };
});
const TOTAL_COMMITS = 163; // git rev-list --count HEAD, merges included
const NON_MERGE = DAYS.reduce((n, d) => n + d.commits, 0);
// find crates/<name> -name '*.rs' | xargs cat | wc -l
const SOURCES = [
  { name: 'editor', lines: 5176 },
  { name: 'gpu-renderer', lines: 4610 },
  { name: '@celesta/react', lines: 3418, ts: true },
  { name: 'renderer', lines: 2859 },
  { name: 'exporter', lines: 2601 },
  { name: 'react-bridge', lines: 2248 },
  { name: 'media', lines: 1263 },
  { name: 'composition', lines: 1261 },
  { name: 'project', lines: 1120 },
  { name: 'evaluator', lines: 728 },
  { name: 'editor-core', lines: 707 },
  { name: 'remote', lines: 682 },
  { name: 'editor-theme', lines: 77 },
];
const RUST_LINES = SOURCES.filter((s) => !s.ts).reduce((n, s) => n + s.lines, 0);
const TS_LINES = SOURCES.filter((s) => s.ts).reduce((n, s) => n + s.lines, 0);

// ── Text ─────────────────────────────────────────────────────────────────

const style = (font: keyof typeof FONT, size: number, color: string = C.paper, weight = 400): TextStyle => ({
  fontFamily: FONT[font], fontSize: size, fontWeight: weight, fill: { type: 'solid', color },
});

type TProps = {
  children: string | number;
  x?: number;
  y?: number;
  size: number;
  font?: keyof typeof FONT;
  weight?: number;
  color?: string;
  ax?: number;
  ay?: number | 'baseline';
  opacity?: number;
  lineHeight?: number;
};

function T({ children, x = 0, y = 0, size, font = 'display', weight = 400, color = C.paper,
  ax = 0, ay = 0, opacity = 1, lineHeight }: TProps) {
  return (
    <Text x={x} y={y} anchorX={ax} anchorY={ay} opacity={opacity}
      style={{ ...style(font, size, color, weight), lineHeight }}>
      {children}
    </Text>
  );
}

// ── Shared pieces ────────────────────────────────────────────────────────

const fmt = (n: number) => n.toLocaleString('en-US');

function Exit({ children }: { children: ReactNode }) {
  return (
    <Transition type={['fade', 'slide']} direction="out" slideFrom="top" distance={30}
      durationInFrames={10} easing={Easings.easeInCubic}>
      {children}
    </Transition>
  );
}

function Header({ en, ja }: { en: string; ja: string }) {
  const f = useCurrentFrame();
  return (
    <Group x={160} y={150} opacity={progress(f, 0, 14, Easings.easeOutExpo)}>
      <Rect y={-6} width={12} height={12} fill={C.mint} />
      <T x={30} size={24} font="mono" weight={700} ay={0.5}>{en}</T>
      <T x={30 + (en.length + 2) * 24 * MONO_ADVANCE} size={22} font="ja" weight={500} color={C.grey} ay={0.5}>{ja}</T>
    </Group>
  );
}

function DotGrid() {
  const { pulse } = useBeat({ bpm: BPM });
  const dots: ReactNode[] = [];
  for (let gx = 0; gx < 25; gx += 1) {
    for (let gy = 0; gy < 14; gy += 1) {
      dots.push(<Rect key={`${gx}-${gy}`} x={40 + gx * 80} y={20 + gy * 80} width={3} height={3} fill={C.paper}
        opacity={0.08 + 0.12 * pulse * random(gx * 31 + gy)} />);
    }
  }
  return <>{dots}</>;
}

function Caret({ x, size, visible }: { x: number; size: number; visible: boolean }) {
  return visible ? <Rect x={x} y={-size * 0.55} width={size * MONO_ADVANCE} height={size * 1.1} fill={C.mint} /> : null;
}

// ── 1 · Cold open: type the command, read the first line ─────────────────

// Keep in sync with the key clicks in make-score.py.
const PROMPT = 'git log --reverse --date=short --format=%ad';
const LOG = DAYS.flatMap((d) => Array.from({ length: d.commits }, () => d.date));

function Open() {
  const f = useCurrentFrame();
  const typed = useTypewriter(PROMPT, { from: 6, blinkFrames: BEAT });
  const size = 40;
  const cell = size * MONO_ADVANCE;
  const x0 = 160;
  const y0 = 300;
  const firstY = y0 + 90;

  const shown = f < 60 ? 0 : Math.min(LOG.length, Math.floor((f - 60) * 3) + 1);
  const rows = 8;
  // The first line stays put; the rest of the log scrolls up beneath it.
  const scroll = Math.max(1, shown - rows);
  // From frame 96 the camera flies to the first line and dives into it.
  const aim = progress(f, 96, 18, Easings.easeInOutCubic);
  const dive = progress(f, 96, 24, Easings.easeInExpo);

  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Camera x={interpolate(aim, [0, 1], [W / 2, x0 + 5 * 28 * MONO_ADVANCE])}
        y={interpolate(aim, [0, 1], [H / 2, firstY])} zoom={1 + 7 * dive}>
        <Group x={x0} y={y0}>
          <T size={size} font="mono" weight={700} color={C.mint} ay={0.5}>$</T>
          <T x={cell * 2} size={size} font="mono" ay={0.5}>{typed.text}</T>
          <Caret x={cell * (2 + typed.length)} size={size} visible={f < 60 && typed.caretVisible} />
        </Group>
        {shown > 0 && <T x={x0} y={firstY} size={28} font="mono" ay={0.5}>{LOG[0]}</T>}
        <Group y={firstY + 24} clip={{ x: -W, width: W * 3, height: rows * 44 }}>
          {LOG.slice(scroll, shown).map((date, i) => (
            <T key={scroll + i} x={x0} y={20 + i * 44} size={28} font="mono" color={C.grey} ay={0.5}
              opacity={1 - 0.8 * progress(f, 84, 10)}>{date}</T>
          ))}
        </Group>
        <T x={x0 + 260} y={firstY} size={22} font="mono" color={C.mint} ay={0.5}
          opacity={progress(f, 84, 10)}>{'← day 1'}</T>
        <T x={x0} y={firstY + rows * 44 + 70} size={22} font="mono" color={C.grey} ay={0.5}
          opacity={progress(f, 72, 10)}>{`${shown} commits`}</T>
      </Camera>
    </>
  );
}

// ── 2 · Title ────────────────────────────────────────────────────────────

function Title() {
  const f = useCurrentFrame();
  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <DotGrid />
      <Exit>
        <T x={160} y={250} size={26} font="mono" weight={700} color={C.mint} ay={0.5}
          opacity={progress(f, BEAT, 10)}>{`${DAYS[0].date}  →  ${DAYS[DAYS.length - 1].date}`}</T>
        <TextReveal x={160} y={300} lineHeight={200} baseline={0.84} from={2} stagger={6}
          style={style('display', 210)}>
          {'36 DAYS\nOF CELESTA'}
        </TextReveal>
        <T x={160} y={800} size={34} font="ja" weight={700} ay={0.5} opacity={progress(f, BEAT * 2, 12)}>
          Git の履歴で振り返る、Celesta の 36 日間。
        </T>
        <T x={W - 160} y={800} size={20} font="mono" color={C.grey} ax={1} ay={0.5}
          opacity={progress(f, BEAT * 3, 12)}>A DATA FILM, MADE WITH CELESTA</T>
      </Exit>
    </>
  );
}

// ── 3 · By the numbers ───────────────────────────────────────────────────

const STATS = [
  { value: TOTAL_COMMITS, en: 'COMMITS', ja: 'マージを含む、すべてのコミット' },
  { value: SOURCES.length - 1, en: 'RUST CRATES', ja: 'ワークスペースのクレート' },
  { value: RUST_LINES, en: 'LINES OF RUST', ja: 'レンダラー、書き出し、エディタ' },
  { value: TS_LINES, en: 'LINES OF TYPESCRIPT', ja: '@celesta/react のランタイム' },
];

// Written for its own frame 0; <Stagger> starts each one a beat apart.
function Stat({ index, x, y }: { index: number; x: number; y: number }) {
  const f = useCurrentFrame();
  const stat = STATS[index];
  const value = useCountUp(stat.value, { durationInFrames: 40 });
  return (
    <Group x={x} y={y} opacity={progress(f, 0, 12, Easings.easeOutExpo)}>
      <Rect width={760 * progress(f, 0, 24, Easings.easeInOutCubic)} height={2} fill={C.mint} />
      <T y={40} size={24} font="mono" weight={700} color={C.mint}>{`0${index + 1}`}</T>
      <T x={60} y={40} size={24} font="mono" weight={700}>{stat.en}</T>
      <T y={200} size={140} ay="baseline">{fmt(value)}</T>
      <T y={250} size={24} font="ja" weight={500} color={C.soft} ay={0.5}>{stat.ja}</T>
    </Group>
  );
}

function Numbers() {
  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Exit>
        <Header en="BY THE NUMBERS" ja="数字で見る" />
        <Stagger from={BEAT - 6} each={BEAT}>
          {STATS.map((stat, i) => (
            <Stat key={stat.en} index={i} x={160 + (i % 2) * 820} y={260 + Math.floor(i / 2) * 340} />
          ))}
        </Stagger>
      </Exit>
    </>
  );
}

// ── 4 · Activity: commits per day, and the running total ─────────────────

const CHART = { x: 160, base: 860, step: 45.5, bar: 36, unit: 20, top: 300 };
const barX = (i: number) => CHART.x + i * CHART.step;
const CUMULATIVE: PolylinePoint[] = (() => {
  let total = 0;
  return DAYS.map((d, i) => {
    total += d.commits;
    return [barX(i) + CHART.bar / 2, CHART.base - (total / NON_MERGE) * (CHART.base - CHART.top)];
  });
})();

const CALLOUTS = [
  { at: 150, day: 0, text: '25 — 初日', lift: 60, days: [0, 0] },
  { at: 170, day: 13, text: '21 — エディタを作り直す', lift: 60, days: [13, 13] },
  { at: 190, day: 22, text: '15 日間の空白', lift: 120, days: [15, 29] },
];

function Activity() {
  const f = useCurrentFrame();
  const drawn = progress(f, 70, 70, Easings.easeInOutCubic);
  const [tipX, tipY] = pointOnPolyline(CUMULATIVE, drawn);
  const total = useCountUp(NON_MERGE, { delay: 70, durationInFrames: 70 });
  const active = useCue(CALLOUTS);
  const callout = active?.cue;
  const lit = (i: number) => callout !== undefined && i >= callout.days[0] && i <= callout.days[1];

  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Exit>
        <Header en="ACTIVITY" ja="日ごとのコミット数と、その累計" />
        <Rect x={CHART.x - 10} y={CHART.base} width={DAYS.length * CHART.step + 10} height={2} fill={C.line} />
        {DAYS.map((d, i) => {
          const grow = progress(f, 8 + i * 1.5, 20, Easings.easeOutBack);
          if (d.commits === 0) {
            return (
              <Rect key={d.date} x={barX(i) + CHART.bar / 2} y={CHART.base - 10} anchorX={0.5} anchorY={0.5}
                width={6} height={6} cornerRadius={3} fill={lit(i) ? C.paper : C.dim} opacity={Math.min(1, grow)} />
            );
          }
          return (
            <Rect key={d.date} x={barX(i)} y={CHART.base} anchorY={1} width={CHART.bar}
              height={Math.max(0.01, d.commits * CHART.unit * grow)} fill={lit(i) ? C.paper : C.mint}
              opacity={callout && !lit(i) ? 0.45 : 0.9} />
          );
        })}
        {[0, 7, 14, 21, 28, 35].map((i) => (
          <T key={i} x={barX(i)} y={CHART.base + 36} size={18} font="mono" color={C.grey} ay={0.5}
            opacity={progress(f, 20, 12)}>{DAYS[i].date.slice(5)}</T>
        ))}
        <Polyline points={CUMULATIVE} progress={drawn} strokeWidth={4} stroke={C.coral} />
        {f >= 70 && (
          <Group x={tipX} y={tipY}>
            <Rect anchorX={0.5} anchorY={0.5} width={16} height={16} cornerRadius={8} fill={C.coral} />
            <T x={-18} y={-26} size={30} ax={1} ay="baseline" color={C.coral}>{total}</T>
          </Group>
        )}
        {active && callout && (() => {
          const p = progress(active.frame, 0, 12, Easings.easeOutExpo);
          const x = barX(callout.day) + CHART.bar / 2;
          const top = CHART.base - DAYS[callout.day].commits * CHART.unit - 14;
          return (
            <Group opacity={p}>
              <Line x1={x} y1={top} x2={x} y2={top - callout.lift * p} stroke={C.paper} cap="butt" />
              <T x={x + 12} y={top - callout.lift} size={28} font="ja" weight={700} ay={0.5}>{callout.text}</T>
            </Group>
          );
        })()}
      </Exit>
    </>
  );
}

// ── 5 · Milestones: a camera travels along the timeline ──────────────────

const MILESTONES = [
  { day: 0, en: 'FIRST COMMIT', ja: '合成モデル、評価器、GPUI エディタ、MP4 書き出し。\n初日だけで 25 コミット。' },
  { day: 1, en: '<SEQUENCE>', ja: 'React で時間を組む。\n<Sequence> と <Audio> が入る。' },
  { day: 4, en: 'LIP SYNC', ja: '5 母音の口パクと PSD 立ち絵。\nキャラクターがしゃべり出す。' },
  { day: 13, en: 'GPUI-KIT', ja: 'エディタを gpui-kit で作り直す。\nこの日だけで 21 コミット。' },
  { day: 31, en: 'CELESTA', ja: 'Frameweave から Celesta へ。\nWeb サイトと配布パッケージ。' },
  { day: 34, en: 'FILMS', ja: 'Reel、Afterimage、Signal、Feature Tour。\n作品を作って確かめる。' },
].map((m, i) => ({ ...m, at: 30 + i * 45 }));
const SPACING = 1000;
// Days are unevenly spaced so that every milestone gets the same room.
const worldX = (day: number) =>
  interpolate(day, [...MILESTONES.map((m) => m.day), 35], [...MILESTONES.map((_, i) => i * SPACING), 5 * SPACING + 300]);
const RAIL = 560;

function Milestones() {
  const f = useCurrentFrame();
  const stop = useCue(MILESTONES);
  const from = stop?.previous ? worldX(stop.previous.day) : -900;
  const to = stop ? worldX(stop.cue.day) : -900;
  const move = stop ? progress(stop.frame, 0, 24, Easings.easeInOutCubic) : 1;

  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Exit>
        <Header en="MILESTONES" ja="36 日間の道のり" />
        <T x={W - 160} y={150} size={22} font="mono" color={C.grey} ax={1} ay={0.5}>
          {`0${(stop?.index ?? 0) + 1} / 0${MILESTONES.length}`}
        </T>
        <Camera x={from + (to - from) * move + 420} y={RAIL} zoom={1 - 0.06 * Math.sin(Math.PI * move)} shake={4}>
          <Rect x={-2000} y={RAIL - 1} width={5 * SPACING + 4400} height={2} fill={C.line} />
          {DAYS.map((d, i) => (
            <Group key={d.date} x={worldX(i)} y={RAIL}>
              <Rect x={-1} y={-8} width={2} height={16} fill={C.grey} opacity={0.6} />
              {d.commits > 0 && (
                <Rect y={-40} anchorX={0.5} anchorY={0.5} width={4 + d.commits * 1.2} height={4 + d.commits * 1.2}
                  cornerRadius={20} fill={C.mint} opacity={0.35} />
              )}
            </Group>
          ))}
          {MILESTONES.map((m, i) => {
            const active = i === stop?.index;
            return (
              <Group key={m.en} x={worldX(m.day)} y={RAIL}>
                <Rect anchorX={0.5} anchorY={0.5} width={26} height={26} cornerRadius={13}
                  fill={active ? C.mint : C.ink} stroke={active ? undefined : C.grey} strokeWidth={active ? undefined : 3} />
                <T y={-100} size={24} font="mono" weight={700} color={active ? C.mint : C.grey} ay={0.5}>
                  {DAYS[m.day].date}
                </T>
                <TextReveal y={50} lineHeight={90} baseline={0.84} from={active ? m.at + 10 : -BAR}
                  style={style('display', 80, active ? C.paper : C.dim)}>
                  {m.en}
                </TextReveal>
                <T y={170} size={28} font="ja" weight={500} lineHeight={46} color={C.soft}
                  opacity={active ? progress(f, m.at + 16, 14) : 0}>{m.ja}</T>
              </Group>
            );
          })}
        </Camera>
      </Exit>
    </>
  );
}

// ── 6 · Where the code lives ─────────────────────────────────────────────

const MAX_LINES = SOURCES[0].lines;

function CrateRow({ index }: { index: number }) {
  const f = useCurrentFrame();
  const s = SOURCES[index];
  const lines = useCountUp(s.lines);
  const width = 1000 * (s.lines / MAX_LINES) * progress(f, 0, 30, Easings.easeOutExpo);
  return (
    <Group y={240 + index * 50} opacity={progress(f, 0, 8)}>
      <T x={160} size={24} font="mono" color={s.ts ? C.coral : C.paper} ay={0.5}>{s.name}</T>
      <Rect x={520} y={-12} width={Math.max(0.01, width)} height={24} fill={s.ts ? C.coral : C.mint} />
      <T x={540 + width} size={22} font="mono" color={C.soft} ay={0.5}>{fmt(lines)}</T>
    </Group>
  );
}

function Crates() {
  const f = useCurrentFrame();
  const total = useCountUp(RUST_LINES + TS_LINES, { delay: 100, durationInFrames: 40 });
  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Exit>
        <Header en="WHERE THE CODE LIVES" ja="クレートごとのソース行数" />
        <Stagger from={10} each={3}>
          {SOURCES.map((s, i) => <CrateRow key={s.name} index={i} />)}
        </Stagger>
        <Group opacity={progress(f, 100, 12)}>
          <T x={W - 160} y={930} size={22} font="mono" color={C.grey} ax={1} ay={0.5}>
            {`RUST ${fmt(RUST_LINES)}  +  TYPESCRIPT ${fmt(TS_LINES)}`}
          </T>
          <T x={W - 160} y={880} size={64} ax={1} ay="baseline">{fmt(total)}</T>
        </Group>
      </Exit>
    </>
  );
}

// ── 7 · Outro: what comes next ───────────────────────────────────────────

const NEXT = 'next: higher-level components & hooks';

function Outro() {
  const f = useCurrentFrame();
  const { durationInFrames } = useVideoConfig();
  const { pulse } = useBeat({ bpm: BPM });
  const typed = useTypewriter(NEXT, { from: 8, framesPerChar: 1 / 1.2, blinkFrames: BEAT });
  const size = 36;
  const cell = size * MONO_ADVANCE;

  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Camera zoom={1 + 0.06 * (f / durationInFrames)}>
        {Array.from({ length: 70 }, (_, i) => {
          const x = random(`star-${i}-x`) * W + noise(i, f / 40) * 40;
          const y = (((random(`star-${i}-y`) * H - f * (0.6 + random(`star-${i}-v`))) % H) + H) % H;
          const s = 2 + random(`star-${i}-s`) * 4;
          return <Rect key={i} x={x} y={y} width={s} height={s} cornerRadius={s / 2} fill={i % 5 === 0 ? C.mint : C.paper}
            opacity={0.15 + 0.35 * random(`star-${i}-o`) + (i % 4 === 0 ? 0.3 * pulse : 0)} />;
        })}
        <TextReveal x={W / 2} y={380} lineHeight={230} baseline={0.84} align={0.5} from={BEAT * 4}
          style={style('display', 230)}>
          Celesta
        </TextReveal>
        <T x={W / 2} y={700} size={24} font="mono" color={C.grey} ax={0.5} ay={0.5}
          opacity={progress(f, BEAT * 5, 12)}>{`36 DAYS  ·  ${TOTAL_COMMITS} COMMITS  ·  1 REPOSITORY`}</T>
        <Group x={W / 2 - (NEXT.length * cell) / 2} y={800}>
          <T size={size} font="mono" color={C.mint} ay={0.5}>{typed.text}</T>
          <Caret x={typed.length * cell + 4} size={size} visible={typed.caretVisible} />
        </Group>
      </Camera>
      <Rect width={W} height={H} fill="#000000" opacity={progress(f, durationInFrames - 24, 24, Easings.easeInCubic)} />
    </>
  );
}

// ── Scenes and HUD ───────────────────────────────────────────────────────

const SCENES = [
  { name: 'COLD OPEN', durationInFrames: BAR * 2, Scene: Open },
  { name: 'TITLE', durationInFrames: BAR * 2, Scene: Title },
  { name: 'NUMBERS', durationInFrames: BAR * 3, Scene: Numbers },
  { name: 'ACTIVITY', durationInFrames: BAR * 4, Scene: Activity },
  { name: 'MILESTONES', durationInFrames: BAR * 5, Scene: Milestones },
  { name: 'CRATES', durationInFrames: BAR * 3, Scene: Crates },
  { name: 'NEXT', durationInFrames: BAR * 3, Scene: Outro },
];
const TIMING = computeSeries(SCENES);
const SCENE_CUES = SCENES.map((s, i) => ({ at: TIMING.sequences[i].from, index: i, name: s.name }));

function Hud() {
  const f = useCurrentFrame();
  const { beatInBar, pulse } = useBeat({ bpm: BPM });
  const scene = cueAt(SCENE_CUES, f)?.cue;
  // Only over the scenes with a light frame around them.
  if (!scene || scene.index === 0 || scene.index === SCENES.length - 1) return null;
  const m = 56;
  return (
    <>
      <Group blendMode="difference" opacity={0.8}>
        <T x={m} y={m} size={18} font="mono" weight={700} ay={0.5}>CELESTA / 36 DAYS</T>
        <T x={W - m} y={m} size={18} font="mono" ax={1} ay={0.5}>{frameToTimecode(f, FPS)}</T>
        <T x={m} y={H - m} size={18} font="mono" ay={0.5}>{`0${scene.index + 1}  ${scene.name}`}</T>
        <T x={W - m - 120} y={H - m} size={18} font="mono" ax={1} ay={0.5}>{`${BPM} BPM`}</T>
        {[0, 1, 2, 3].map((b) => (
          <Rect key={b} x={W - m - 96 + b * 26} y={H - m - 8} width={16} height={16}
            fill={C.paper} opacity={b === beatInBar ? 0.4 + 0.6 * pulse : 0.2} />
        ))}
      </Group>
      <Rect x={m} y={H - 24} width={(W - m * 2) * (f / (TIMING.durationInFrames - 1))} height={2} fill={C.mint} opacity={0.8} />
    </>
  );
}

export default function Film() {
  return (
    <Composition width={W} height={H} fps={FPS} durationInFrames={TIMING.durationInFrames}>
      <Assets>
        <Font src="https://fonts.googleapis.com/css2?family=Archivo+Black&family=JetBrains+Mono:wght@400;700&family=Noto+Sans+JP:wght@500;700" />
      </Assets>
      <Series>
        {SCENES.map(({ name, durationInFrames, Scene }) => (
          <Series.Sequence key={name} durationInFrames={durationInFrames}><Scene /></Series.Sequence>
        ))}
      </Series>
      <Hud />
      <Audio src="./score.wav" />
    </Composition>
  );
}
