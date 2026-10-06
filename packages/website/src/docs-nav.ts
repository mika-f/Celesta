import { translate, type Message } from './catalog.ts';
import { localeFromPath, type Locale } from './locales.ts';

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

const groupMessages = [
  {
    title: 'nav.getting-started',
    pages: [
      { slug: 'installation', title: 'nav.install-get-started', description: 'nav.packages-for-macos-and-windows-with-the', keywords: 'setup download installer binary macOS Windows Linux dmg portable zip updates' },
      { slug: 'create-project', title: 'nav.create-a-project', description: 'nav.initialize-a-react-project-from-the-app', keywords: 'new project init --init directory folder film.tsx package.json pnpm npm dependencies ag-psd node assets typescript workspace' },
      { slug: 'preview', title: 'nav.preview-your-work', description: 'nav.open-reload-play-and-scrub-in-the', keywords: 'keyboard shortcuts open reload play pause scrub inspector audio mute solo' },
      { slug: 'react-compositions', title: 'nav.your-first-react-composition', description: 'nav.a-five-second-title-card-with-composition', keywords: 'tsx jsx components typescript setup codegen build title' },
    ],
  },
  {
    title: 'nav.guides',
    pages: [
      { slug: 'animation', title: 'nav.frames-timing-animation', description: 'nav.make-motion-a-function-of-the-frame', keywords: 'interpolate interpolateColor color easing spring transition sequence hooks useCurrentFrame duration fps opacity animation' },
      { slug: 'motion-toolkit', title: 'nav.scenes-cues-beats', description: 'nav.sequencing-staggering-and-beat-sync', keywords: 'Series Stagger computeSeries progress useBeat beatAt bpm music useCue cueAt scenes cascade frameToTimecode timecode' },
      { slug: 'text-camera-lines', title: 'nav.text-effects-camera-lines', description: 'nav.reveals-typewriters-counters-camera-moves-and-drawn', keywords: 'TextReveal useTypewriter useCountUp Camera shake zoom Line Polyline Path Circle Ellipse Arrow pointOnPolyline chart counter typewriter draw on' },
      { slug: 'layout', title: 'nav.shapes-layout', description: 'nav.position-layers-group-them-clip-them-and', keywords: 'Rect Group blendMode blend mode multiply screen overlay difference anchor rotation scale Center SafeArea Stack Grid Fit clip mask layout coordinates' },
      { slug: 'text-fonts', title: 'nav.text-fonts', description: 'nav.style-text-wrap-it-and-load-font', keywords: 'TextBox fitText useFitText measureText useTextMetrics Text style font Font webfont woff woff2 Google Fonts css fontFamily fontWeight stroke outline lineHeight maxWidth wrap lineBreak phrase BudouX Japanese align baseline emoji' },
      { slug: 'media', title: 'nav.images-video-sound', description: 'nav.image-video-and-audio-layers-from-local', keywords: 'Image Video Audio media src url remote download startFrom playbackRate volume muted keyframes fade music preloadMedia mediaDurationInFrames png jpeg webp mp4 wav' },
      { slug: 'dialogue', title: 'nav.character-dialogue', description: 'nav.portraits-subtitles-voices-and-lip-sync', keywords: 'DialogueSeries planDialogue holdSubtitle subtitle render dialogue character CharacterView portrait expressions subtitles voice lip sync lipsync mouth psd pfv PSDTool voiceroid talk conversation' },
      { slug: 'data', title: 'nav.data-properties-components', description: 'nav.load-data-once-expose-settings-and-reuse', keywords: 'prepare async fetch data defineProjectProperties useProjectProperty ProjectProvider loadProject registerComponent component inspector schema' },
      { slug: 'export', title: 'nav.export-a-video', description: 'nav.take-your-composition-from-the-preview-to', keywords: 'driver auto vulkan dx12 metal gl frames every contact-sheet columns tile-width preset crf json mp4 H264 AAC render cli command line from to overwrite export Linux headless GPU software Vulkan lavapipe Mesa Docker container CI png frame' },
    ],
  },
  {
    title: 'nav.packages',
    pages: [
      { slug: 'reference', title: 'nav.celesta-react', description: 'nav.a-compact-reference-for-everything-in-celesta', keywords: 'api props Composition Rect Text Group Image Video Audio Font Sequence Series Stagger Transition Camera Line Polyline Path Circle Ellipse Arrow TextReveal useBeat useCue Character CharacterView Dialogue hooks reference' },
      { slug: 'voicevox', title: 'nav.celesta-voicevox', description: 'nav.voicevox-description', keywords: 'VOICEVOX AudioQuery lipSyncFromVoicevox voicevoxVowelShape voice lip sync phoneme interrogativeUpspeak frameRate' },
      { slug: 'math', title: 'nav.celesta-math', description: 'nav.seeded-randomness-that-renders-the-same-way', keywords: 'math random randomRange randomInt randomBool randomSign randomPick shuffle randomGaussian randomInCircle seed deterministic Math.random' },
      { slug: 'math-noise', title: 'nav.math-noise-fbm', description: 'nav.smooth-repeatable-drift-wobble-and-organic-fields', keywords: 'noise noise2D noise3D fbm fbm2D fbm3D octaves lacunarity gain perlin wobble drift shake clouds smoke terrain' },
      { slug: 'math-shaping', title: 'nav.math-shaping-waves-geometry', description: 'nav.number-shaping-waves-angles-and-2d-points', keywords: 'clamp lerp inverseLerp remap smoothstep smootherstep wrap mod pingPong snap roundTo fract sineWave triangleWave squareWave sawtoothWave degToRad lerpAngle rotatePoint polarToCartesian bezier distance Vec2 TAU' },
      { slug: 'code', title: 'nav.celesta-code', description: 'nav.syntax-highlighted-code-with-themes-and-line', keywords: 'code syntax highlight highlighting twinkleplop tokens theme codeThemes language tsx ts json bash monospace font highlightLines tabSize' },
      { slug: 'code-typing', title: 'nav.code-typing-carets-tokens', description: 'nav.reveal-code-as-it-is-typed-place', keywords: 'visibleCharacters useTypewriter useCodePoint codeCharacterCount tokenizeCode caret annotation typing reveal limits performance ligatures' },
    ],
  },
  {
    title: 'nav.more',
    pages: [
      { slug: 'examples', title: 'nav.examples-to-build-on', description: 'nav.complete-files-you-can-copy-run-and', keywords: 'examples samples inspiration title layout animation dialogue project properties' },
      { slug: 'timelines', title: 'nav.json-project-timelines', description: 'nav.deprecated-kept-for-existing-projects', keywords: 'json schema project assets tracks settings range time timescale properties keyframes easing ProjectTimeline' },
      { slug: 'build-from-source', title: 'nav.build-from-source', description: 'nav.for-contributors-custom-builds-and-linux-users', keywords: 'developer source code clone cargo Rust FFmpeg Node pnpm codegen macOS Windows Linux Ubuntu apt pkg-config PKG_CONFIG_PATH' },
      { slug: 'troubleshooting', title: 'nav.troubleshooting', description: 'nav.common-errors-and-how-to-fix-them', keywords: 'errors missing media build FFmpeg npm typescript black blank reload limitations help lip sync font subtitle' },
    ],
  },
] satisfies { title: Message; pages: (Omit<DocPage, 'title' | 'description'> & { title: Message; description: Message })[] }[];

export function getDocGroups(locale: Locale): DocGroup[] {
  return groupMessages.map(group => ({
    title: translate(locale, group.title),
    pages: group.pages.map(page => ({ ...page, title: translate(locale, page.title), description: translate(locale, page.description) })),
  }));
}

export const docGroups = getDocGroups(localeFromPath(typeof window === 'undefined' ? '/en/' : window.location.pathname));
export const docPages: DocPage[] = docGroups.flatMap(group => group.pages);

/** The path of a documentation page. The overview is `/<locale>/docs/`. */
export function docPath(slug: string, locale = localeFromPath(typeof window === 'undefined' ? '/en/' : window.location.pathname)): string {
  return slug === 'overview' ? `/${locale}/docs/` : `/${locale}/docs/${slug}/`;
}
