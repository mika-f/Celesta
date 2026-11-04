import { Span } from '@celesta.studio/react';
import type { Chapter } from '../components/ChapterShell';
import { C } from '../components/theme';
import { ComponentsDemo } from './demos/ComponentsDemo';
import { DepthDemo } from './demos/DepthDemo';
import { EdgesDemo } from './demos/EdgesDemo';
import { ExporterDemo } from './demos/ExporterDemo';
import { FallbackDemo } from './demos/FallbackDemo';
import { LottieDemo } from './demos/LottieDemo';
import { MetricsDemo } from './demos/MetricsDemo';
import { PitchDemo } from './demos/PitchDemo';
import { StrokesDemo } from './demos/StrokesDemo';
import { ThemeDemo } from './demos/ThemeDemo';
import { TitlesDemo } from './demos/TitlesDemo';
import { VoicesDemo } from './demos/VoicesDemo';

// The twelve features of v0.8.0, in running order. Each is one chapter of
// four bars: what it is, the lines that use it, the PRs, and a live demo.
// MORE, the eight one-bar cards after them, is in More.tsx.
const accent = { fill: C.orange };

export const CHAPTERS: readonly Chapter[] = [
  {
    key: '3d transforms',
    title: '3D',
    headline: <>Turn it over, <Span style={accent}>lean it back</Span>.</>,
    body: 'Layers and groups take rotationX, rotationY, perspective and skew about any originX and originY, and backfaceVisibility hides the side turned away.',
    file: 'film.tsx',
    code: '<Group rotationX={58} perspective={900}>\n  <Crawl />\n</Group>',
    merged: [[390, 'skew and 3D transforms'], [411, 'transform origin'], [393, 'density bands'], [394, 'web planes']],
    Demo: DepthDemo,
  },
  {
    key: 'lottie',
    title: 'LOTTIE',
    headline: <>After Effects, <Span style={accent}>in step</Span>.</>,
    body: 'Lottie draws a Bodymovin file at every frame as paths, groups and masks, so scrubbing equals playing, at any speed, in any direction.',
    file: 'film.tsx',
    code: "import cat from './loader-cat.json';\n<Lottie data={cat} playbackRate={2} />",
    merged: [[409, '@celesta.studio/lottie']],
    meta: 'Loader cat · Lottie Simple License',
    Demo: LottieDemo,
  },
  {
    key: 'components',
    title: 'COMPONENTS',
    headline: <>The parts you kept <Span style={accent}>rewriting</Span>.</>,
    body: '@celesta.studio/components: counters, stat cards, bar charts, badges, panels, patterns, frame chrome and media frames, each measured, timed and themed.',
    file: 'film.tsx',
    code: '<StatCard label="PULL REQUESTS" value={49}\n  delay={4} fill="orange" />',
    merged: [[432, 'Counter, StatCard, BarChart'], [431, 'Badge'], [426, 'Panel'], [424, 'PatternBackground'], [423, 'FrameChrome'], [436, 'MediaFrame']],
    Demo: ComponentsDemo,
  },
  {
    key: 'titles and motion',
    title: 'TITLES',
    headline: <>Titles that <Span style={accent}>come and go</Span>.</>,
    body: 'TitleCard lays out a chapter screen, Motion gives anything an entrance, a hold and an exit, and TypewriterText puts its caret on the measured line.',
    file: 'film.tsx',
    code: '<Motion enter="fade-up" exit="fade-down">\n  <TitleCard title="Code is the title" />\n</Motion>',
    merged: [[434, 'TitleCard'], [427, 'Motion'], [428, 'TypewriterText']],
    Demo: TitlesDemo,
  },
  {
    key: 'theme',
    title: 'THEME',
    headline: <>Name it once, <Span style={accent}>use it everywhere</Span>.</>,
    body: 'Text takes fontFamily, fontSize, fontWeight and color as props, and a Theme names the fonts and colors they look up, nested themes winning.',
    file: 'film.tsx',
    code: '<Theme colors={{ accent: \'#F26A2E\' }}>\n  <Text fontFamily="display" color="accent" />',
    merged: [[421, 'Text props, Theme'], [422, 'Theme in fitted text']],
    Demo: ThemeDemo,
  },
  {
    key: 'text strokes',
    title: 'STROKES',
    headline: <>Outline it <Span style={accent}>twice</Span>.</>,
    body: "A text style's stroke takes a list. Each width is measured from the glyph, so the double outline subtitles use is one layer.",
    file: 'film.tsx',
    code: 'stroke: [{ paint: orange, width: 8 },\n  { paint: white, width: 18 }]',
    merged: [[403, 'stroke lists']],
    Demo: StrokesDemo,
  },
  {
    key: 'text metrics',
    title: 'METRICS',
    headline: <>Know where <Span style={accent}>the ink</Span> is.</>,
    body: "measureText() and useTextMetrics() report the font's cap and x heights and each glyph's ink, with kerning taken out of the advances.",
    file: 'film.tsx',
    code: 'const { capHeight, xHeight, glyphs } =\n  useTextMetrics(word, style);',
    merged: [[401, 'cap/x height, ink'], [396, 'web line starts']],
    Demo: MetricsDemo,
  },
  {
    key: 'audio pitch',
    title: 'PITCH',
    headline: <>Transpose <Span style={accent}>what you hear</Span>.</>,
    body: 'Audio takes pitch in semitones, keyframed or not, and preservePitch lets playbackRate change only the speed. This riff is one file, moved.',
    file: 'film.tsx',
    code: '<Audio src="riff.wav" pitch={-4} />\n<Audio src="riff.wav" pitch={7}\n  playbackRate={2} preservePitch />',
    merged: [[392, 'pitch, preservePitch']],
    meta: 'listen  ·  one riff, four pitches',
    Demo: PitchDemo,
  },
  {
    key: 'font fallback',
    title: 'FALLBACK',
    headline: <>Missing glyphs <Span style={accent}>come from your fonts</Span>.</>,
    body: 'fontFallback="loaded" takes what a face lacks from the fonts you loaded, not the machine\'s, and <Font google text> loads only the characters drawn.',
    file: 'film.tsx',
    code: '<Composition fontFallback="loaded">\n  <Font google="Noto Sans JP" text={used} />',
    merged: [[399, 'fontFallback="loaded"'], [387, '<Font google text>']],
    Demo: FallbackDemo,
  },
  {
    key: 'shader edges',
    title: 'EDGES',
    headline: <>Read past <Span style={accent}>the edge</Span>.</>,
    body: 'In WGSL, source_at_clamped() holds the edge pixel and source_at_mirrored() folds the picture back, so a full-frame lens leaves no seams.',
    file: 'lens.ts',
    language: 'ts',
    code: 'return source_at_mirrored(\n  center + offset * zoom + wave);',
    merged: [[402, 'source_at_clamped, source_at_mirrored']],
    Demo: EdgesDemo,
  },
  {
    key: 'node api',
    title: 'EXPORTER',
    headline: <>Export <Span style={accent}>from Node.js</Span>.</>,
    body: '@celesta.studio/exporter runs the exporter for you: videos, stills and composition info, with progress events and an AbortSignal. Stills can be JPEG or WebP.',
    file: 'export.mjs',
    language: 'ts',
    code: "await exportVideo({ source: 'film-0.8.0.tsx',\n  output: 'delta.mp4', onProgress, signal });",
    merged: [[400, '@celesta.studio/exporter'], [381, 'JPEG, WebP'], [382, 'MCP stills'], [398, '--loop']],
    Demo: ExporterDemo,
  },
  {
    key: 'character',
    title: 'VOICES',
    headline: <>Drop in a folder, <Span style={accent}>even the levels</Span>.</>,
    body: 'importVoiceFolder() pairs a speech tool\'s .wav and .txt files and reloads on save, matchVoiceLevels() evens the loudness, and expressions change at cues.',
    file: 'film.tsx',
    code: "const lines = await importVoiceFolder({\n  directory: 'voices/shizuku' });",
    merged: [[406, 'voice folders'], [404, 'level matching'], [397, 'expression cues'], [407, 'PSD face parts']],
    Demo: VoicesDemo,
  },
];
