import { Span } from '@celesta.studio/react';
import type { Chapter } from '../components/ChapterShell';
import { C } from '../components/theme';
import { BlendDemo } from './demos/BlendDemo';
import { ColorDemo } from './demos/ColorDemo';
import { EditorDemo } from './demos/EditorDemo';
import { EffectsDemo } from './demos/EffectsDemo';
import { MaskDemo } from './demos/MaskDemo';
import { McpDemo } from './demos/McpDemo';
import { MediaDemo } from './demos/MediaDemo';
import { MotionBlurDemo } from './demos/MotionBlurDemo';
import { PathsDemo } from './demos/PathsDemo';
import { ShapesDemo } from './demos/ShapesDemo';
import { SpectrumDemo } from './demos/SpectrumDemo';
import { TransitionsDemo } from './demos/TransitionsDemo';

// The twelve features of v0.7.0, in running order. Each is one chapter of
// four bars: what it is, the lines that use it, the PRs, and a live demo.
// MORE, the eight one-bar cards after them, is in More.tsx.
const accent = { fill: C.orange };

export const CHAPTERS: readonly Chapter[] = [
  {
    key: 'transitions',
    title: 'TRANSITIONS',
    headline: <>Seven new cuts, <Span style={accent}>and your own</Span>.</>,
    body: 'Iris, clock wipe, zoom, dissolve, blur, flip and cube join the four, and defineTransition() makes more. No two cuts in this film are alike.',
    file: 'film.tsx',
    code: 'const delta = defineTransition({ present });\n<TransitionSeries.Transition type={delta}\n  durationInFrames={8} />',
    merged: [[350, 'seven types, defineTransition(), series size']],
    Demo: TransitionsDemo,
  },
  {
    key: 'masks',
    title: 'MASK',
    headline: <>Show it <Span style={accent}>through anything</Span>.</>,
    body: 'A Group takes a mask: text, a path, an image or any subtree, by alpha or by luminance, inverted or not. A faded group now fades as one picture.',
    file: 'film.tsx',
    code: '<Group mask={<Text>0.7.0</Text>}>\n  <Film />\n</Group>',
    merged: [[350, '<Mask> in React and the browser'], [358, 'group opacity']],
    Demo: MaskDemo,
  },
  {
    key: 'motion blur',
    title: 'MOTION BLUR',
    headline: <>Fast things <Span style={accent}>look fast</Span>.</>,
    body: 'MotionBlur averages the moments an open shutter sees; Trail leaves fading copies behind. Preview and export draw the same frame.',
    file: 'film.tsx',
    code: '<MotionBlur shutterAngle={270} samples={12}>\n  {(frame) => <Wheel angle={frame * 24} />}\n</MotionBlur>',
    merged: [[357, '<MotionBlur>, <Trail>'], [359, 'noise inside the blur']],
    Demo: MotionBlurDemo,
  },
  {
    key: 'effects',
    title: 'EFFECTS',
    headline: <>Stylize <Span style={accent}>without WGSL</Span>.</>,
    body: 'Halftone, pixelate, chromatic aberration, wave, noise, scanlines and zoom blur are layer props, and chromaKey takes the green out.',
    file: 'film.tsx',
    code: '<Image src="shot.png" halftone={10}\n  chromaticAberration={6} scanlines={0.4} />',
    merged: [[355, 'stylize effects, chroma key']],
    Demo: EffectsDemo,
  },
  {
    key: 'color',
    title: 'COLOR',
    headline: <>Grade it, <Span style={accent}>then chain it</Span>.</>,
    body: 'Brightness, contrast, saturation, hue, grayscale, invert, tint and vignette are props. shader takes an array that runs in order.',
    file: 'film.tsx',
    code: "shader={[ripple({ time }), grayscale(),\n  tint({ color: '#F26A2E' }), vignette(0.6)]}",
    merged: [[325, 'color filters'], [324, 'shader arrays']],
    Demo: ColorDemo,
  },
  {
    key: 'blend',
    title: 'BLEND',
    headline: <>Seventeen ways <Span style={accent}>to mix a layer</Span>.</>,
    body: 'blendMode takes every W3C mode, from multiply and screen to hue and luminosity, and PSD portraits keep their own.',
    file: 'film.tsx',
    code: '<Circle radius={140} fill="#5AA9FF"\n  blendMode="difference" />',
    merged: [[343, 'W3C blend modes'], [351, 'PSD blend layers']],
    Demo: BlendDemo,
  },
  {
    key: 'shapes',
    title: 'SHAPES',
    headline: <>Stars, pies, hearts, <Span style={accent}>dashes</Span>.</>,
    body: 'Triangle, Polygon, Star, Pie and Heart round their corners, and every stroke can be dashed or dotted, flowing with dashOffset.',
    file: 'film.tsx',
    code: '<Star radius={90} points={5} cornerRadius={8}\n  stroke="#F26A2E" dash={[18, 10]} />',
    merged: [[346, 'closed shapes'], [349, 'dashed strokes']],
    Demo: ShapesDemo,
  },
  {
    key: 'paths',
    title: 'PATHS',
    headline: <>Draw it on, <Span style={accent}>morph it</Span>.</>,
    body: 'Trim a path with progress, ride it with pointOnPath(), read SVG path data, and blend any two outlines with interpolatePath().',
    file: 'film.tsx',
    code: "const route = parseSvgPath('M0 0 C90-80 …');\n<Path commands={interpolatePath(a, b, t)} />",
    merged: [[344, 'measure, trim, morph, SVG paths']],
    Demo: PathsDemo,
  },
  {
    key: 'spectrum',
    title: 'SPECTRUM',
    headline: <>The music, <Span style={accent}>frame by frame</Span>.</>,
    body: "loadAudioSpectrum() analyzes a file once and useAudioSpectrum() gives every frame its bands and loudness. These bars are this film's score.",
    file: 'film.tsx',
    code: 'const { bands, rms } = useAudioSpectrum(score);\n<Rect height={bands[i] * 300} />',
    merged: [[348, 'spectrum and loudness']],
    Demo: SpectrumDemo,
  },
  {
    key: 'media',
    title: 'MEDIA',
    headline: <>Videos with sound, <Span style={accent}>GIFs in time</Span>.</>,
    body: 'A Video plays its soundtrack, Video and Audio loop, animated GIF, APNG and WebP follow the timeline, and .mov or .webm keep the alpha.',
    file: 'film.tsx',
    code: '<Video src="clip.mp4" volume={0.6} loop />\n<Image src="spinner.png" />',
    merged: [[320, '<Video> sound'], [332, 'loop'], [326, 'animated images'], [327, 'transparent video']],
    Demo: MediaDemo,
  },
  {
    key: 'agents',
    title: 'MCP',
    headline: <>Hand the timeline <Span style={accent}>to an agent</Span>.</>,
    body: 'Celesta-mcp gives coding agents inspect, render_frames, contact_sheet and export_video, and returns the frames as images.',
    file: 'terminal',
    language: 'bash',
    code: 'claude mcp add celesta -- \\\n  Celesta.app/Contents/MacOS/Celesta-mcp',
    merged: [[322, 'Celesta MCP server'], [338, 'Node errors kept']],
    Demo: McpDemo,
  },
  {
    key: 'editor',
    title: 'EDITOR',
    headline: <>A timeline <Span style={accent}>that reads React</Span>.</>,
    body: "A composition's audio and videos show as clips, swipes and pinches pan and zoom, a click opens the layer's source, and Problems keeps every error.",
    file: 'film.tsx',
    code: 'const preview = useIsPreview();\n// true in the editor, false in export',
    merged: [[334, 'audio clips'], [354, 'video tracks'], [333, 'swipe, pinch'], [335, 'open source'], [336, 'Problems'], [282, 'useIsPreview()']],
    Demo: EditorDemo,
  },
];
