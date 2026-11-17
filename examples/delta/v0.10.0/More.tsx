import type { ReactNode } from 'react';
import { Easings, Group, Rect, Sequence, Text, interpolate, progress, useCurrentFrame } from '@celesta.studio/react';
import { Badge } from '@celesta.studio/components';
import { useFitText } from '@celesta.studio/text';
import { Backdrop } from '../components/Backdrop';
import { C, FONT, H, W, solid } from '../components/theme';
import { DEPENDENCY_UPDATES } from './prs';
import { CARD_LENGTH, MORE_AT } from './timeline';

// MORE: twelve cards for what did not get a module, two beats each, each on
// a stab in the score. The title slides in under a mask; the cards
// alternate ink and orange, and every color comes from the card's palette.

interface Palette { ink: string; sub: string; accent: string; panel: string }
const DARK: Palette = { ink: C.paper, sub: C.grey, accent: C.orange, panel: '#FFFFFF10' };
const LIGHT: Palette = { ink: C.ink, sub: '#0C0D0FAA', accent: C.paper, panel: '#0C0D0F1C' };

interface VisualProps { f: number; c: Palette }
interface Card { title: string; line: string; prs: readonly number[]; Visual: (props: VisualProps) => ReactNode }

const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;
const TITLE_STYLE = { fontFamily: FONT.display, letterSpacing: { em: 0.01 } } as const;

/** Lines of mono text, the first `bold` of them bold. */
function Lines({ lines, c, size = 24, gap = 46, bold = 0, accent = -1 }:
  { lines: readonly string[]; c: Palette; size?: number; gap?: number; bold?: number; accent?: number }) {
  return (
    <Group>
      {lines.map((line, i) => (
        <Text key={i} y={i * gap} style={mono(size, i === accent ? c.accent : i < bold ? c.ink : c.sub, i < bold || i === accent ? 700 : 400)}>{line}</Text>
      ))}
    </Group>
  );
}

/** libx264 out of the bundled FFmpeg (#810). */
function X264({ c }: VisualProps) {
  return (
    <Group>
      {[
        ['ffmpeg', 'bundled · LGPL', 'ProRes · VP9 · AAC · Opus · MP4 · MOV · WebM'],
        ['ffmpeg-x264', 'a download · GPL', 'H.264 (x264)'],
      ].map(([id, how, what], i) => (
        <Group key={id} y={i * 150}>
          <Rect width={620} height={124} fill={c.panel} />
          <Text x={20} y={18} style={mono(28, c.ink, 700)}>{id}</Text>
          <Text x={600} y={22} anchorX={1} style={mono(20, i === 1 ? c.accent : c.sub, 700)}>{how}</Text>
          <Text x={20} y={74} style={mono(20, c.sub)}>{what}</Text>
        </Group>
      ))}
      <Text y={330} style={mono(19, c.sub)}>Celesta-plugin-ffmpeg-x264-0.10.0-macos-aarch64.zip</Text>
    </Group>
  );
}

/** Color bars through the BT.709 matrix every export now uses (#797). */
function Bt709({ c }: VisualProps) {
  const bars = ['#BFBFBF', '#BFBF00', '#00BFBF', '#00BF00', '#BF00BF', '#BF0000', '#0000BF'];
  return (
    <Group>
      {bars.map((color, i) => <Rect key={color} x={i * 88} width={88} height={200} fill={color} />)}
      <Text y={232} style={mono(22, c.ink, 700)}>{"Y' = 16 + 219 · (0.2126 R + 0.7152 G + 0.0722 B)"}</Text>
      <Text y={276} style={mono(20, c.sub)}>MP4 colr nclx · MOV colr nclc · WebM Colour</Text>
      <Text y={316} style={mono(20, c.sub)}>primaries, transfer and matrix: 1 (BT.709)</Text>
    </Group>
  );
}

/** Stills and sequences through the built-in image components (#795). */
function Stills({ c }: VisualProps) {
  return (
    <Group>
      {['builtin/png', 'builtin/jpeg', 'builtin/webp'].map((id, i) => (
        <Group key={id} x={i * 212}>
          <Rect width={196} height={140} fill={c.panel} />
          <Text x={98} y={70} anchorX={0.5} anchorY={0.5} style={{ fontFamily: FONT.display, fontSize: 72, fill: solid(c.accent) }}>{id.split('/')[1].toUpperCase()}</Text>
          <Text y={156} style={mono(17, c.sub)}>{id}</Text>
        </Group>
      ))}
      <Text y={236} style={mono(22, c.ink, 700)}>builtin/imageSequence</Text>
      <Text y={276} style={mono(22, c.sub)}>frames/frame-%06d.png</Text>
      <Text y={316} style={mono(20, c.sub)}>no media plugin needed for a still</Text>
    </Group>
  );
}

/** Export presets, the user's and the project's (#795). */
function Presets({ c }: VisualProps) {
  return <Lines c={c} size={22} gap={40} bold={1} lines={[
    'export-presets.json',
    '{ "version": 1, "presets": [',
    '  { "name": "YouTube 1080p",',
    '    "muxer": "ffmpeg/mp4",',
    '    "video": { "component": … } }],',
    '  "default": "YouTube 1080p" }',
    'Celesta-export --preset "YouTube 1080p"',
  ]} accent={6} />;
}

/** The npm scope (#666). */
function Scope({ c }: VisualProps) {
  return (
    <Group>
      <Text style={mono(26, c.sub)}>{"from '@celesta/react'"}</Text>
      <Rect y={18} width={350} height={3} fill={c.accent === C.paper ? C.ink : C.red} />
      <Text y={60} style={mono(30, c.ink, 700)}>{"from '@celesta.studio/react'"}</Text>
      <Text y={150} style={mono(22, c.sub)}>the old names still resolve, with a warning;</Text>
      <Text y={186} style={mono(22, c.sub)}>scripts/migrate-packages.mjs rewrites them</Text>
    </Group>
  );
}

/** The crates on crates.io (#665 #830). */
function Crates({ c }: VisualProps) {
  const crates = ['celesta-composition', 'celesta-plugin', 'celesta-renderer', 'celesta-gpu-renderer',
    'celesta-media', 'celesta-registry-format', 'celesta-exporter', '…'];
  return (
    <Group>
      {crates.map((name, i) => (
        <Text key={name} x={(i % 2) * 330} y={Math.floor(i / 2) * 44} style={mono(21, i < 2 ? c.ink : c.sub, i < 2 ? 700 : 400)}>{name}</Text>
      ))}
      <Text y={220} style={mono(22, c.ink, 700)}>cargo add celesta-plugin</Text>
      <Text y={262} style={mono(20, c.sub)}>published from CI with Trusted Publishing</Text>
    </Group>
  );
}

/** @font-face descriptors, applied natively (#625). */
function FontFace({ c }: VisualProps) {
  return <Lines c={c} size={22} gap={42} bold={1} lines={[
    '@font-face {',
    '  font-family: "Brand";',
    '  font-weight: 100 900;',
    '  font-style: italic;',
    '  unicode-range: U+3000-30FF;',
    '}',
    'applied by the native renderer;',
    'installed families hidden',
  ]} />;
}

/** A whole component instance, selected from the canvas (#758). */
function Instance({ f, c }: VisualProps) {
  const p = progress(f, 4, 10);
  return (
    <Group>
      <Rect x={40} y={30} width={300} height={170} cornerRadius={10} fill={c.accent} />
      <Rect x={70} y={60} width={160} height={24} fill={c.panel} />
      <Rect x={70} y={100} width={220} height={16} fill={c.panel} />
      <Rect x={250} y={150} width={70} height={32} cornerRadius={16} fill={c.ink} />
      <Rect x={26} y={16} width={328} height={198} stroke={c.ink} strokeWidth={3} dash={[12, 8]} opacity={p} />
      <Text x={26} y={236} opacity={p} style={mono(22, c.ink, 700)}>{'<PriceTag> · instance'}</Text>
      <Text y={300} style={mono(20, c.sub)}>one click selects the call, not a rect in it</Text>
    </Group>
  );
}

/** The editing gaps closed (#748 #753 #755 #756 #757 #752). */
function Gaps({ c }: VisualProps) {
  const rows = [
    ['#753', 'adding elements'], ['#748', 'the edit scope'], ['#755', 'timing on the timeline'],
    ['#756', 'every timing window instance'], ['#757', 'scope choices in changed files'], ['#752', 'scene ends inside later scenes'],
  ];
  return (
    <Group>
      {rows.map(([n, text], i) => (
        <Group key={n} y={i * 52}>
          <Text style={mono(22, c.accent, 700)}>{n}</Text>
          <Text x={96} style={mono(22, c.ink)}>{text}</Text>
        </Group>
      ))}
    </Group>
  );
}

/** NaN refused (#729). */
function NotANumber({ c }: VisualProps) {
  return (
    <Group>
      <Text style={mono(26, c.ink, 700)}>{'<Sequence from={NaN}>'}</Text>
      <Rect y={56} width={640} height={110} fill={c.panel} />
      <Text x={20} y={74} maxWidth={600} style={mono(20, c.accent === C.paper ? C.ink : C.red, 700)}>{'<Sequence> `from` must be a finite'}</Text>
      <Text x={20} y={108} maxWidth={600} style={mono(20, c.accent === C.paper ? C.ink : C.red, 700)}>number of frames, not NaN</Text>
      <Text y={210} style={mono(20, c.sub)}>timing, media and animation inputs throw</Text>
      <Text y={246} style={mono(20, c.sub)}>instead of quietly becoming 0</Text>
    </Group>
  );
}

/** The workspace's tools (#715 #607 #844 #843 #779). */
function Tooling({ c }: VisualProps) {
  const rows = [
    ['cargo fmt --check', 'CI formats the workspace'],
    ['cargo clippy', 'on every system'],
    ['cargo nextest', "the plugins' tests"],
    ['actions.lock', 'every action pinned'],
    ['mise.toml', 'exact Node, pnpm, Rust'],
  ];
  return (
    <Group>
      {rows.map(([tool, what], i) => (
        <Group key={tool} y={i * 56}>
          <Text style={mono(23, c.ink, 700)}>{tool}</Text>
          <Text x={300} style={mono(20, c.sub)}>{what}</Text>
        </Group>
      ))}
      <Text y={300} style={mono(20, c.sub)}>{`${DEPENDENCY_UPDATES} dependency updates by Renovate`}</Text>
    </Group>
  );
}

/** The fixes. */
function Fixes({ c }: VisualProps) {
  const lines = [
    ['#733', 'stalled downloads time out'],
    ['#738', 'a hung plugin is killed'],
    ['#739', 'images reload when their file changes'],
    ['#750', 'a fitted image keeps its resolution'],
    ['#713', 'MCP inspect is capped and cancellable'],
    ['#744', 'replaced preview frames are freed'],
  ];
  return (
    <Group>
      {lines.map(([n, text], i) => (
        <Group key={n} y={i * 54}>
          <Text style={mono(22, c.accent, 700)}>{n}</Text>
          <Text x={96} maxWidth={560} style={mono(21, c.ink)}>{text}</Text>
        </Group>
      ))}
    </Group>
  );
}

export const CARDS: readonly Card[] = [
  { title: 'X264', line: 'libx264 is a download of its own; the bundled FFmpeg stays LGPL.', prs: [810, 824], Visual: X264 },
  { title: 'BT.709', line: 'Every export converts and tags its video as BT.709.', prs: [797], Visual: Bt709 },
  { title: 'STILLS', line: 'PNG, JPEG and WebP stills and sequences, built in.', prs: [795], Visual: Stills },
  { title: 'PRESETS', line: "Export settings saved as presets, the user's or the project's.", prs: [795, 813], Visual: Presets },
  { title: '@CELESTA.STUDIO', line: 'The packages took the @celesta.studio npm scope.', prs: [666], Visual: Scope },
  { title: 'CRATES.IO', line: 'The crates are on crates.io, published by Trusted Publishing.', prs: [665, 830], Visual: Crates },
  { title: 'FONT FACE', line: '@font-face descriptors applied natively, installed families hidden.', prs: [625], Visual: FontFace },
  { title: 'INSTANCES', line: 'Select a whole component instance from the canvas.', prs: [758], Visual: Instance },
  { title: 'EDIT GAPS', line: 'The known gaps of adding, scope and timing edits, closed.', prs: [753, 748, 755], Visual: Gaps },
  { title: 'NAN', line: 'NaN timing, media and animation inputs are refused.', prs: [729], Visual: NotANumber },
  { title: 'TOOLING', line: 'rustfmt, Clippy and nextest in CI; actions and tools pinned.', prs: [715, 607, 844, 843], Visual: Tooling },
  { title: 'FIXES', line: 'And the things that were wrong.', prs: [733, 738, 739, 750], Visual: Fixes },
];

/** Card `index` at frame `f` of its two beats. */
export function MoreCard({ index, f }: { index: number; f: number }) {
  const card = CARDS[index];
  const light = index % 2 === 1;
  const c = light ? LIGHT : DARK;
  const slide = interpolate(f, [0, 8], [1, 0], { easing: Easings.easeOutExpo, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  const fit = useFitText(card.title, { width: 960, height: 250, minFontSize: 120, maxFontSize: 230, maxLines: 1, style: TITLE_STYLE });
  const { Visual } = card;
  return (
    <Group>
      {light ? <Rect width={W} height={H} fill="orange" /> : <Backdrop />}
      <Text x={110} y={130} style={{ fontFamily: FONT.display, fontSize: 110, fill: solid(c.accent) }}>{`+${String(index + 1).padStart(2, '0')}`}</Text>
      <Text x={290} y={168} style={mono(30, c.sub, 700)}>MORE</Text>
      <Group x={100} y={290} clip={{ x: -10, y: 0, width: 1000, height: 250 }}>
        <Text x={-900 * slide} y={(230 - fit.fontSize) * 0.7} style={{ ...fit.style, fill: solid(c.ink) }}>{card.title}</Text>
      </Group>
      <Text x={110} y={580} maxWidth={820} opacity={progress(f, 3, 8)}
        style={{ fontFamily: FONT.sans, fontSize: 50, fontWeight: 700, lineHeight: 62, fill: solid(c.ink) }}>{card.line}</Text>
      <Group x={110} y={860} opacity={progress(f, 6, 8)}>
        {card.prs.map((n, i) => (
          <Badge key={n} x={i * 150} fill={light ? 'ink' : 'raised'} stroke={light ? 'ink' : 'line'} cornerRadius="pill"
            padding={{ top: 8, right: 20, bottom: 8, left: 20 }} fontFamily="mono" fontSize={26} color="orange">
            {`#${n}`}
          </Badge>
        ))}
      </Group>
      <Group x={1160} y={300} opacity={progress(f, 2, 8)}>
        <Visual f={f} c={c} />
      </Group>
    </Group>
  );
}

function CardAt({ index }: { index: number }) {
  return <MoreCard index={index} f={useCurrentFrame()} />;
}

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
