import { Span } from '@celesta.studio/react';
import type { Chapter } from '../components/ChapterShell';
import { C } from '../components/theme';
import { CodeDemo } from './demos/CodeDemo';
import { FontStackDemo } from './demos/FontStackDemo';
import { FreezeDemo } from './demos/FreezeDemo';
import { InstancesDemo } from './demos/InstancesDemo';
import { LoopDemo } from './demos/LoopDemo';
import { PackagesDemo } from './demos/PackagesDemo';
import { ShaderDemo } from './demos/ShaderDemo';
import { SpanDemo } from './demos/SpanDemo';
import { SpeedDemo } from './demos/SpeedDemo';
import { TemplatesDemo } from './demos/TemplatesDemo';
import { TenBitDemo } from './demos/TenBitDemo';
import { TransitionsDemo } from './demos/TransitionsDemo';

// The twelve features of v0.6.0, in running order. Each is one chapter of
// two bars: what it is, the lines that use it, the PRs, and a live demo.
const accent = { fill: C.orange };

export const CHAPTERS: readonly Chapter[] = [
  {
    key: 'shader',
    title: 'SHADER',
    headline: <>Write your own <Span style={accent}>WGSL</Span>.</>,
    body: 'Custom filters run on the GPU in preview and export, on any layer or group, and in components on project timelines.',
    file: 'film.tsx',
    code: "const ripple = defineShader({ wgsl, params });\n<Text shader={ripple({ time })}>WGSL</Text>",
    merged: [[217, 'WGSL filters'], [218, '@celesta.studio/shader'], [248, 'on timelines']],
    Demo: ShaderDemo,
  },
  {
    key: 'instances',
    title: 'INSTANCES',
    headline: <>30,000 particles, <Span style={accent}>one layer</Span>.</>,
    body: 'Rects, circles or one image for every particle, from typed-array columns you fill in place each frame.',
    file: 'film.tsx',
    code: '<Instances count={30_000} x={xs} y={ys}\n  color={rgba} shape="circle" />',
    merged: [[259, '<Instances>'], [265, 'in shorts-explainer']],
    Demo: InstancesDemo,
  },
  {
    key: 'transitions',
    title: 'TRANSITIONS',
    headline: <>Scenes joined, <Span style={accent}>overlaps computed</Span>.</>,
    body: 'Crossfade, slide or wipe between scenes. TransitionSeries works out where each scene starts and how long the series runs.',
    file: 'film.tsx',
    code: '<TransitionSeries.Transition type="wipe"\n  from="right" durationInFrames={8} />',
    merged: [[190, 'TransitionSeries']],
    Demo: TransitionsDemo,
  },
  {
    key: 'rich text',
    title: 'SPAN',
    headline: <>Style a word, <Span style={accent}>not a layer</Span>.</>,
    body: 'Fill, weight and family change inside one Text, which still wraps, aligns and measures as one paragraph.',
    file: 'film.tsx',
    code: '<Text maxWidth={width}>\n  many <Span style={{ fontWeight: 700 }}>\n  weights</Span> in one line</Text>',
    merged: [[186, 'color runs'], [207, '<Span>'], [208, 'in the web preview']],
    Demo: SpanDemo,
  },
  {
    key: 'font stack',
    title: 'FONT STACK',
    headline: <>Latin and 日本語, <Span style={accent}>one style</Span>.</>,
    body: 'List families in fontFamily and each character takes the first one that has it. Spacing in em follows the size.',
    file: 'film.tsx',
    code: "fontFamily: ['Space Grotesk', 'Noto Sans JP'],\nletterSpacing: { em: 0.12 },",
    merged: [[255, 'font stacks'], [254, 'letterSpacing in em']],
    Demo: FontStackDemo,
  },
  {
    key: 'code',
    title: 'CODE',
    headline: <>Code's colors, <Span style={accent}>your staging</Span>.</>,
    body: "highlightCode() returns each line with <Code>'s color runs, to draw, move and mark yourself. Every diff in this film is drawn that way.",
    file: 'film.tsx',
    code: 'const lines = highlightCode(src);\n<Text style={{ colorRuns: line.runs }}>\n  {line.text}</Text>',
    merged: [[257, 'highlightCode()'], [265, 'in shorts-explainer']],
    Demo: CodeDemo,
  },
  {
    key: 'freeze',
    title: 'FREEZE',
    headline: <>Any frame, <Span style={accent}>drawn anywhere</Span>.</>,
    body: 'FreezeFrame draws its children as the composition looked at that frame: rewinds, flashbacks, thumbnail strips.',
    file: 'film.tsx',
    code: '<FreezeFrame frame={240}>\n  <Film />\n</FreezeFrame>',
    merged: [[169, '<FreezeFrame>']],
    Demo: FreezeDemo,
  },
  {
    key: 'loop',
    title: 'LOOP',
    headline: <>Seamless loops, <Span style={accent}>checked</Span>.</>,
    body: "loopNoise repeats over the loop's length, and inspect.mjs --loop compares the frames past the end with the start.",
    file: 'film.tsx',
    code: 'const t = frame / 20;\nconst n = loopNoise(seed, t, LOOP / 20);',
    merged: [[251, 'loopNoise, --loop'], [250, 'PERFECT LOOP']],
    Demo: LoopDemo,
  },
  {
    key: 'dither',
    title: '10-BIT',
    headline: <>Gradients <Span style={accent}>without bands</Span>.</>,
    body: 'Frames are composited in half float and dithered once. --bit-depth 10 writes H.264 High 10.',
    file: 'terminal',
    language: 'bash',
    code: 'Celesta-export --react film.tsx out.mp4 \\\n  --bit-depth 10',
    merged: [[252, 'dither, 10-bit H.264'], [270, 'one copy less']],
    Demo: TenBitDemo,
  },
  {
    key: 'templates',
    title: 'TEMPLATES',
    headline: <>Start from a <Span style={accent}>template</Span>.</>,
    body: '--init --template sets up a video and its props.json. Edit the values in the Properties panel, or per render with --props.',
    file: 'film.tsx',
    code: "defineProjectProperties({\n  title: { type: 'string', label: 'Title' },\n});",
    merged: [[245, 'templates'], [246, 'Properties panel'], [191, '--props']],
    Demo: TemplatesDemo,
  },
  {
    key: 'packages',
    title: 'PACKAGES',
    headline: <>Import <Span style={accent}>what you use</Span>.</>,
    body: '@celesta.studio/react is split into focused packages, and a codemod moves your imports to them.',
    file: 'film.tsx',
    code: "import { Circle } from '@celesta.studio/shapes';\nimport { Center } from '@celesta.studio/layout';",
    merged: [[212, 'package split, codemod'], [182, '@celesta.studio/voicevox']],
    Demo: PackagesDemo,
  },
  {
    key: 'speed',
    title: 'SPEED',
    headline: <>Faster <Span style={accent}>where it counts</Span>.</>,
    body: 'Particles in one layer, still subtrees built once, and a leaner path from React to the GPU.',
    file: 'film.tsx',
    code: '<Static deps={[theme]}>\n  <Background />\n</Static>',
    merged: [[272, '<Static>'], [180, 'GPU export'], [266, 'per-layer cost'], [189, 'pipelining']],
    Demo: SpeedDemo,
  },
];
