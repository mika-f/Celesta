// The documentation's structure. It has no JSX or imports so vite.config.ts
// can read it too: every page below is built as /docs/<slug>/.

export interface DocPage {
  slug: string;
  title: string;
  description: string;
  keywords: string;
}

export interface DocGroup {
  title: string;
  pages: DocPage[];
}

export const docGroups: DocGroup[] = [
  {
    title: 'Getting started',
    pages: [
      { slug: 'installation', title: 'Install & get started', description: 'Packages for macOS and Windows, with the runtime included.', keywords: 'setup download installer binary macOS Windows Linux dmg portable zip updates' },
      { slug: 'create-project', title: 'Create a project', description: 'Initialize a React project from the app or CLI and add dependencies.', keywords: 'new project init --init directory folder film.tsx package.json pnpm npm dependencies ag-psd node assets typescript workspace' },
      { slug: 'preview', title: 'Preview your work', description: 'Open, reload, play, and scrub in the desktop app.', keywords: 'keyboard shortcuts open reload play pause scrub inspector audio mute solo' },
      { slug: 'react-compositions', title: 'Your first React composition', description: 'A five-second title card with Composition, Text, and useCurrentFrame.', keywords: 'tsx jsx components typescript setup codegen build title' },
    ],
  },
  {
    title: 'Guides',
    pages: [
      { slug: 'animation', title: 'Frames, timing & animation', description: 'Make motion a function of the frame.', keywords: 'interpolate easing spring transition sequence hooks useCurrentFrame duration fps opacity animation' },
      { slug: 'motion-toolkit', title: 'Scenes, cues & beats', description: 'Sequencing, staggering, and beat sync.', keywords: 'Series Stagger computeSeries progress useBeat beatAt bpm music useCue cueAt scenes cascade frameToTimecode timecode' },
      { slug: 'text-camera-lines', title: 'Text effects, camera & lines', description: 'Reveals, typewriters, counters, camera moves, and drawn paths.', keywords: 'TextReveal useTypewriter useCountUp Camera shake zoom Line Polyline Path Circle Ellipse Arrow pointOnPolyline chart counter typewriter draw on' },
      { slug: 'layout', title: 'Shapes & layout', description: 'Position layers, group them, clip them, and arrange a scene.', keywords: 'Rect Group blendMode blend mode multiply screen overlay difference anchor rotation scale Center SafeArea Stack Grid Fit clip mask layout coordinates' },
      { slug: 'text-fonts', title: 'Text & fonts', description: 'Style text, wrap it, and load font files or web fonts.', keywords: 'Text style font Font webfont woff woff2 Google Fonts css fontFamily fontWeight stroke outline lineHeight maxWidth wrap lineBreak phrase BudouX Japanese align baseline emoji' },
      { slug: 'media', title: 'Images, video & sound', description: 'Image, Video, and Audio layers from local or remote files.', keywords: 'Image Video Audio media src url remote download startFrom playbackRate volume muted keyframes fade music preloadMedia mediaDurationInFrames png jpeg webp mp4 wav' },
      { slug: 'dialogue', title: 'Character dialogue', description: 'Portraits, subtitles, voices, and lip sync.', keywords: 'dialogue character CharacterView portrait expressions subtitles voice lip sync lipsync mouth psd pfv PSDTool voiceroid talk conversation' },
      { slug: 'data', title: 'Data, properties & components', description: 'Load data once, expose settings, and reuse components from JSON.', keywords: 'prepare async fetch data defineProjectProperties useProjectProperty ProjectProvider loadProject registerComponent component inspector schema' },
      { slug: 'export', title: 'Export a video', description: 'Take your composition from the preview to an MP4.', keywords: 'mp4 H264 AAC render cli command line from to overwrite export Linux headless GPU software Vulkan lavapipe Mesa Docker container CI png frame' },
    ],
  },
  {
    title: 'Packages',
    pages: [
      { slug: 'reference', title: '@celesta/react', description: 'A compact reference for everything in @celesta/react.', keywords: 'api props Composition Rect Text Group Image Video Audio Font Sequence Series Stagger Transition Camera Line Polyline Path Circle Ellipse Arrow TextReveal useBeat useCue Character CharacterView Dialogue hooks reference' },
      { slug: 'math', title: '@celesta/math', description: 'Seeded randomness that renders the same way every time.', keywords: 'math random randomRange randomInt randomBool randomSign randomPick shuffle randomGaussian randomInCircle seed deterministic Math.random' },
      { slug: 'math-noise', title: 'Math: noise & fbm', description: 'Smooth, repeatable drift, wobble, and organic fields.', keywords: 'noise noise2D noise3D fbm fbm2D fbm3D octaves lacunarity gain perlin wobble drift shake clouds smoke terrain' },
      { slug: 'math-shaping', title: 'Math: shaping, waves & geometry', description: 'Number shaping, waves, angles, and 2D points.', keywords: 'clamp lerp inverseLerp remap smoothstep smootherstep wrap mod pingPong snap roundTo fract sineWave triangleWave squareWave sawtoothWave degToRad lerpAngle rotatePoint polarToCartesian bezier distance Vec2 TAU' },
      { slug: 'code', title: '@celesta/code', description: 'Syntax-highlighted code, with themes and line highlights.', keywords: 'code syntax highlight highlighting twinkleplop tokens theme codeThemes language tsx ts json bash monospace font highlightLines tabSize' },
      { slug: 'code-typing', title: 'Code: typing, carets & tokens', description: 'Reveal code as it is typed, place carets, and read tokens.', keywords: 'visibleCharacters useTypewriter useCodePoint codeCharacterCount tokenizeCode caret annotation typing reveal limits performance ligatures' },
    ],
  },
  {
    title: 'More',
    pages: [
      { slug: 'examples', title: 'Examples to build on', description: 'Complete files you can copy, run, and change.', keywords: 'examples samples inspiration title layout animation dialogue project properties' },
      { slug: 'timelines', title: 'JSON project timelines', description: 'Deprecated. Kept for existing projects.', keywords: 'json schema project assets tracks settings range time timescale properties keyframes easing ProjectTimeline' },
      { slug: 'build-from-source', title: 'Build from source', description: 'For contributors, custom builds, and Linux users.', keywords: 'developer source code clone cargo Rust FFmpeg Node pnpm codegen macOS Windows Linux Ubuntu apt pkg-config PKG_CONFIG_PATH' },
      { slug: 'troubleshooting', title: 'Troubleshooting', description: 'Common errors and how to fix them.', keywords: 'errors missing media build FFmpeg npm typescript black blank reload limitations help lip sync font subtitle' },
    ],
  },
];

export const docPages: DocPage[] = docGroups.flatMap(group => group.pages);

/** The path of a documentation page. The overview is `/docs/`. */
export function docPath(slug: string): string {
  return slug === 'overview' ? '/docs/' : `/docs/${slug}/`;
}
