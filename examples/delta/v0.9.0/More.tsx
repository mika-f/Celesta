import type { ReactNode } from 'react';
import { Easings, Group, Rect, Sequence, Text, Video, interpolate, progress, useCurrentFrame, useScene } from '@celesta/react';
import { Badge } from '@celesta/components';
import { Polyline } from '@celesta/shapes';
import { useFitText } from '@celesta/text';
import { Delta } from '../components/Delta';
import { Backdrop } from '../components/Backdrop';
import { C, FONT, solid } from '../components/theme';
import { useLayout } from './layout';
import type { Layout } from './layout';
import { EmphasisDemo } from './demos/EmphasisDemo';
import { LigaturesDemo } from './demos/LigaturesDemo';
import { ThemeDemo } from './demos/ThemeDemo';
import type { DemoProps } from './program';
import { STAGE } from './program';
import { DEPENDENCY_UPDATES } from './prs';
import { CARD_LENGTH, MORE_AT, SCENE_LIST } from './timeline';

// MORE: twelve cards for what did not get a chapter, two beats each, each
// on a stab in the score. The title slides in under a mask; the cards
// alternate ink and orange, and every color comes from the card's palette.

interface Palette { ink: string; sub: string; accent: string; panel: string }
const DARK: Palette = { ink: C.paper, sub: C.grey, accent: C.orange, panel: '#FFFFFF10' };
const LIGHT: Palette = { ink: C.ink, sub: '#0C0D0FAA', accent: C.paper, panel: '#0C0D0F1C' };

interface VisualProps { f: number; c: Palette }
interface Card { title: string; line: string; prs: readonly number[]; Visual: (props: VisualProps) => ReactNode }

const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;
const TITLE_STYLE = { fontFamily: FONT.display, letterSpacing: { em: 0.01 } } as const;

/** A program's demo on a card: its stage scaled down, run `speed` times as fast. */
function demoCard(Demo: (props: DemoProps) => ReactNode, speed: number) {
  return function DemoCard({ f }: VisualProps) {
    return (
      <Group scale={0.62}>
        <Rect width={STAGE.width} height={STAGE.height} fill={C.ink} stroke={C.line} strokeWidth={2} />
        <Demo frame={f * speed} live={false} />
      </Group>
    );
  };
}

const FOOTAGE = '../greenscreen/assets/footage.mp4';

/** <Video> sized by width or height, the other side following the footage (#553). */
function VideoSize({ c }: VisualProps) {
  return (
    <Group>
      <Video src={FOOTAGE} width={360} />
      <Text y={220} style={mono(22, c.ink)}>{'<Video width={360} />'}</Text>
      <Group x={390}>
        <Video src={FOOTAGE} height={140} />
        <Text y={160} style={mono(22, c.ink)}>{'height={140}'}</Text>
      </Group>
      <Text y={290} style={mono(22, c.sub)}>one side alone keeps the aspect;</Text>
      <Text y={324} style={mono(22, c.sub)}>no more scale to size the footage</Text>
    </Group>
  );
}

/** The film's chapters as the preview lists them (#495): this composition declares chapters={SCENE_LIST}. */
function Chapters({ c }: VisualProps) {
  const rows = SCENE_LIST.scenes;
  const now = rows.findIndex((s) => s.name === 'MORE');
  return (
    <Group>
      <Text style={mono(22, c.sub, 700)}>Chapters</Text>
      {rows.map((scene, i) => (
        <Group key={scene.id} y={44 + i * 38}>
          {i === now ? <Rect x={-12} y={-6} width={620} height={36} fill={c.panel} /> : null}
          <Text style={mono(22, i === now ? c.accent : c.sub, i === now ? 700 : 400)}>{String(scene.from).padStart(4, ' ')}</Text>
          <Text x={90} style={mono(22, c.ink, i === now ? 700 : 400)}>{scene.name}</Text>
        </Group>
      ))}
    </Group>
  );
}

/** The scene list read from inside the card's Sequence, on the composition's clock (#472). */
function SceneClock({ c }: VisualProps) {
  const local = useCurrentFrame();
  const scene = useScene(SCENE_LIST, { clock: 'root' });
  return (
    <Group>
      <Text style={mono(26, c.ink, 700)}>{"useScene(SCENE_LIST, { clock: 'root' })"}</Text>
      {[
        ['name', scene?.name ?? '-'],
        ['index', String(scene?.index ?? '-')],
        ['from', String(scene?.from ?? '-')],
        ['progress', (scene?.progress ?? 0).toFixed(2)],
        ['useCurrentFrame()', `${local}   (the Sequence's own clock)`],
      ].map(([k, v], i) => (
        <Group key={k} y={70 + i * 52}>
          <Text style={mono(26, c.sub)}>{k}</Text>
          <Text x={300} style={mono(26, i === 4 ? c.sub : c.accent, 700)}>{v}</Text>
        </Group>
      ))}
      <Rect y={350} width={600} height={14} fill={c.panel} />
      <Rect y={350} width={600 * (scene?.progress ?? 0)} height={14} fill={c.accent} />
    </Group>
  );
}

/** A contact sheet with a frame that failed: still written, the failed tile marked (#482). */
function FailedFrames({ c }: VisualProps) {
  return (
    <Group>
      {Array.from({ length: 12 }, (_, i) => {
        const failed = i === 7;
        return (
          <Group key={i} x={(i % 4) * 160} y={Math.floor(i / 4) * 104}>
            <Rect width={150} height={84} fill={failed ? '#FF5F5633' : c.panel} stroke={failed ? C.red : c.sub} strokeWidth={failed ? 3 : 1} />
            {failed ? null : <Delta x={100} y={40} size={34} rotation={i * 30} fill={c.accent} />}
            <Text x={8} y={64} style={mono(15, failed ? C.red : c.sub, 700)}>{failed ? 'FAILED' : String(i * 10)}</Text>
            {failed ? <Text x={8} y={10} style={mono(15, C.red)}>frame 70</Text> : null}
          </Group>
        );
      })}
      <Text y={340} style={mono(22, c.ink)}>{'"code": "frames_failed",'}</Text>
      <Text y={374} style={mono(22, c.ink)}>{'"failedFrames": [{ "frame": 70, … }]'}</Text>
    </Group>
  );
}

/** Keyframed volume and playback rate, now in browser export too (#463). */
function WebAudio({ f, c }: VisualProps) {
  const keys: [number, number][] = [[0, 200], [140, 60], [300, 60], [420, 190], [600, 120]];
  const head = (f / CARD_LENGTH) * 600;
  return (
    <Group>
      <Text style={mono(22, c.sub, 700)}>{'exportMp4() in the browser'}</Text>
      <Group y={60}>
        <Rect width={600} height={240} fill={c.panel} />
        <Polyline points={keys} stroke={c.accent} strokeWidth={5} join="round" />
        {keys.map(([x, y], i) => <Rect key={i} x={x - 8} y={y - 8} width={16} height={16} fill={c.ink} />)}
        <Rect x={head} y={-10} width={3} height={260} fill={c.ink} />
      </Group>
      <Text y={340} style={mono(22, c.ink)}>{'volume={keyframes}  playbackRate={keyframes}'}</Text>
      <Text y={374} style={mono(22, c.sub)}>the same curves as the native mixer</Text>
    </Group>
  );
}

/** The keyframe ease-in-out is now Easings.easeInOut, the quadratic (#474). */
function Easing({ f, c }: VisualProps) {
  const pts = Array.from({ length: 41 }, (_, i): [number, number] => [i * 15, 300 - Easings.easeInOut(i / 40) * 280]);
  const t = Math.min(1, Math.max(0, f / (CARD_LENGTH - 6)));
  return (
    <Group>
      <Rect width={600} height={300} fill={c.panel} />
      <Polyline points={pts} stroke={c.accent} strokeWidth={5} join="round" />
      <Rect x={t * 600 - 9} y={300 - Easings.easeInOut(t) * 280 - 9} width={18} height={18} cornerRadius={9} fill={c.ink} />
      <Text y={340} style={mono(22, c.ink)}>{"easing: 'ease-in-out'  ===  Easings.easeInOut"}</Text>
      <Text y={374} style={mono(22, c.sub)}>keyframes, the mixer and the web, one curve</Text>
    </Group>
  );
}

/** examples/zunda, a VOICEVOX dialogue explainer of Celesta (#75). */
function Zunda({ c }: VisualProps) {
  const tree = ['examples/zunda/', '  film.tsx', '  script.ts', '  src/timing.ts', '  src/cast/Cast.tsx', '  src/scenes/  void · layers · effects', '               voice · rewind · outro', '  make-score.py'];
  return (
    <Group>
      {tree.map((line, i) => <Text key={i} y={i * 44} style={mono(26, i === 0 ? c.accent : c.ink, i === 0 ? 700 : 400)}>{line}</Text>)}
    </Group>
  );
}

/** Tests in TypeScript under Vitest (#468), dependencies kept by Renovate (#473 on). */
function Tooling({ c }: VisualProps) {
  return (
    <Group>
      <Text style={mono(26, c.ink, 700)}>vitest run</Text>
      <Text y={44} style={mono(24, c.sub)}>every JS and TS test in TypeScript, testing src/</Text>
      <Text y={130} style={mono(26, c.ink, 700)}>renovate</Text>
      <Text y={174} style={mono(24, c.sub)}>{`${DEPENDENCY_UPDATES} dependency updates since v0.8.0`}</Text>
      <Text y={214} style={mono(24, c.sub)}>3 days after release · grouped · OSV alerts</Text>
    </Group>
  );
}

/** The fixes. */
function Fixes({ c }: VisualProps) {
  const lines = [
    ['#455', 'FreezeFrame names a cycle instead of running out of heap'],
    ['#470', 'a playback-rate segment that starts before 0'],
    ['#474', 'keyframe ease-in-out matches Easings.easeInOut'],
    ['#510', 'Theme is a wrapper in the source chain'],
    ['#457', 'the bundled Node.js follows mise'],
  ];
  return (
    <Group>
      {lines.map(([n, text], i) => (
        <Group key={n} y={i * 62}>
          <Text style={mono(24, c.accent, 700)}>{n}</Text>
          <Text x={96} maxWidth={560} style={mono(22, c.ink)}>{text}</Text>
        </Group>
      ))}
    </Group>
  );
}

export const CARDS: readonly Card[] = [
  { title: 'EMPHASIS', line: 'Emphasis as grapheme ranges of a string; timing as half-open frame ranges.', prs: [450], Visual: demoCard(EmphasisDemo, 4) },
  { title: 'LIGATURES', line: 'fontFeatures switches ligatures, tabular figures and other OpenType features.', prs: [459], Visual: demoCard(LigaturesDemo, 2) },
  { title: 'THEME', line: 'Rect and Path paints, and every shape on them, take Theme color names.', prs: [481], Visual: demoCard(ThemeDemo, 2) },
  { title: 'VIDEO SIZE', line: 'Size a Video by width and height, as an Image.', prs: [553], Visual: VideoSize },
  { title: 'CHAPTERS', line: 'The preview lists the chapters you declare, and seeks to them.', prs: [495], Visual: Chapters },
  { title: 'SCENE CLOCK', line: "Read the scene list on the composition's clock from any Sequence.", prs: [472], Visual: SceneClock },
  { title: 'FAILED FRAMES', line: 'A contact sheet carries on past a frame that fails, and names it.', prs: [482], Visual: FailedFrames },
  { title: 'WEB AUDIO', line: 'Keyframed volume and playback rate in browser export.', prs: [463], Visual: WebAudio },
  { title: 'EASING', line: 'One ease-in-out: keyframes, the mixer and Easings agree.', prs: [474, 470], Visual: Easing },
  { title: 'ZUNDA', line: 'A two-minute VOICEVOX dialogue explainer, in examples/.', prs: [75], Visual: Zunda },
  { title: 'TOOLING', line: 'Tests in TypeScript under Vitest. Dependencies kept by Renovate.', prs: [468, 473, 536, 544, 545], Visual: Tooling },
  { title: 'FIXES', line: 'And the things that were wrong.', prs: [455, 470, 510, 457], Visual: Fixes },
];

/** Where a card's parts go, at 16:9 or inside the short's safe area. */
function placement(layout: Layout) {
  if (!layout.portrait) {
    return { number: { x: 110, y: 130 }, more: { x: 290, y: 168 }, title: { x: 100, y: 290, width: 960 }, line: { x: 110, y: 580, width: 820, size: 50 },
      prs: { x: 110, y: 860 }, visual: { x: 1160, y: 300, scale: 1 } };
  }
  const x = layout.safe.left + 24;
  const width = layout.width - layout.safe.left - layout.safe.right - 48;
  const top = layout.safe.top + 30;
  return { number: { x, y: top }, more: { x: x + 180, y: top + 38 }, title: { x: x - 10, y: top + 140, width }, line: { x, y: top + 400, width, size: 46 },
    prs: { x, y: layout.height - layout.safe.bottom - 86 }, visual: { x, y: top + 610, scale: 1.18 } };
}

/** Card `index` at frame `f` of its two beats. */
export function MoreCard({ index, f }: { index: number; f: number }) {
  const card = CARDS[index];
  const layout = useLayout();
  const at = placement(layout);
  const light = index % 2 === 1;
  const c = light ? LIGHT : DARK;
  const slide = interpolate(f, [0, 8], [1, 0], { easing: Easings.easeOutExpo, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  const fit = useFitText(card.title, { width: at.title.width, height: 250, minFontSize: 120, maxFontSize: 230, maxLines: 1, style: TITLE_STYLE });
  const { Visual } = card;
  return (
    <Group>
      {light ? <Rect width={layout.width} height={layout.height} fill="orange" /> : <Backdrop width={layout.width} height={layout.height} />}
      <Text {...at.number} style={{ fontFamily: FONT.display, fontSize: 110, fill: solid(c.accent) }}>{`+${String(index + 1).padStart(2, '0')}`}</Text>
      <Text {...at.more} style={mono(30, c.sub, 700)}>MORE</Text>
      <Group x={at.title.x} y={at.title.y} clip={{ x: -10, y: 0, width: at.title.width + 40, height: 250 }}>
        <Text x={-900 * slide} y={(230 - fit.fontSize) * 0.7} style={{ ...fit.style, fill: solid(c.ink) }}>{card.title}</Text>
      </Group>
      <Text x={at.line.x} y={at.line.y} maxWidth={at.line.width} opacity={progress(f, 3, 8)}
        style={{ fontFamily: FONT.sans, fontSize: at.line.size, fontWeight: 700, lineHeight: at.line.size * 1.24, fill: solid(c.ink) }}>{card.line}</Text>
      <Group {...at.prs} opacity={progress(f, 6, 8)}>
        {card.prs.map((n, i) => (
          <Badge key={n} x={i * 150} fill={light ? 'ink' : 'raised'} stroke={light ? 'ink' : 'line'} cornerRadius="pill"
            padding={{ top: 8, right: 20, bottom: 8, left: 20 }} fontFamily="mono" fontSize={26} color={light ? 'orange' : 'orange'}>
            {`#${n}`}
          </Badge>
        ))}
      </Group>
      <Group {...at.visual} opacity={progress(f, 2, 8)}>
        <Visual f={f} c={c} />
      </Group>
    </Group>
  );
}

function CardAt({ index }: { index: number }) {
  return <MoreCard index={index} f={useCurrentFrame()} />;
}

/** The cards, each in its own Sequence: SCENE CLOCK reads the film's clock from inside one. */
export function More() {
  return (
    <>
      {CARDS.map((_, i) => (
        <Sequence key={i} from={MORE_AT + i * CARD_LENGTH} durationInFrames={CARD_LENGTH}>
          <CardAt index={i} />
        </Sequence>
      ))}
    </>
  );
}

/** Japanese the cards draw. */
export const MORE_STRINGS: string[] = [];
