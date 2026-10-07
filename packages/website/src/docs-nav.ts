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
      { slug: 'installation', title: 'nav.install-get-started', description: 'nav.packages-for-macos-and-windows-with-the', keywords: 'search.installation' },
      { slug: 'create-project', title: 'nav.create-a-project', description: 'nav.initialize-a-react-project-from-the-app', keywords: 'search.create-project' },
      { slug: 'preview', title: 'nav.preview-your-work', description: 'nav.open-reload-play-and-scrub-in-the', keywords: 'search.preview' },
      { slug: 'react-compositions', title: 'nav.your-first-react-composition', description: 'nav.a-five-second-title-card-with-composition', keywords: 'search.react-compositions' },
    ],
  },
  {
    title: 'nav.guides',
    pages: [
      { slug: 'animation', title: 'nav.frames-timing-animation', description: 'nav.make-motion-a-function-of-the-frame', keywords: 'search.animation' },
      { slug: 'motion-toolkit', title: 'nav.scenes-cues-beats', description: 'nav.sequencing-staggering-and-beat-sync', keywords: 'search.motion-toolkit' },
      { slug: 'text-camera-lines', title: 'nav.text-effects-camera-lines', description: 'nav.reveals-typewriters-counters-camera-moves-and-drawn', keywords: 'search.text-camera-lines' },
      { slug: 'layout', title: 'nav.shapes-layout', description: 'nav.position-layers-group-them-clip-them-and', keywords: 'search.layout' },
      { slug: 'text-fonts', title: 'nav.text-fonts', description: 'nav.style-text-wrap-it-and-load-font', keywords: 'search.text-fonts' },
      { slug: 'media', title: 'nav.images-video-sound', description: 'nav.image-video-and-audio-layers-from-local', keywords: 'search.media' },
      { slug: 'dialogue', title: 'nav.character-dialogue', description: 'nav.portraits-subtitles-voices-and-lip-sync', keywords: 'search.dialogue' },
      { slug: 'data', title: 'nav.data-properties-components', description: 'nav.load-data-once-expose-settings-and-reuse', keywords: 'search.data' },
      { slug: 'export', title: 'nav.export-a-video', description: 'nav.take-your-composition-from-the-preview-to', keywords: 'search.export' },
    ],
  },
  {
    title: 'nav.packages',
    pages: [
      { slug: 'reference', title: 'nav.celesta-react', description: 'nav.a-compact-reference-for-everything-in-celesta', keywords: 'search.reference' },
      { slug: 'voicevox', title: 'nav.celesta-voicevox', description: 'nav.voicevox-description', keywords: 'search.voicevox' },
      { slug: 'math', title: 'nav.celesta-math', description: 'nav.seeded-randomness-that-renders-the-same-way', keywords: 'search.math' },
      { slug: 'math-noise', title: 'nav.math-noise-fbm', description: 'nav.smooth-repeatable-drift-wobble-and-organic-fields', keywords: 'search.math-noise' },
      { slug: 'math-shaping', title: 'nav.math-shaping-waves-geometry', description: 'nav.number-shaping-waves-angles-and-2d-points', keywords: 'search.math-shaping' },
      { slug: 'code', title: 'nav.celesta-code', description: 'nav.syntax-highlighted-code-with-themes-and-line', keywords: 'search.code' },
      { slug: 'code-typing', title: 'nav.code-typing-carets-tokens', description: 'nav.reveal-code-as-it-is-typed-place', keywords: 'search.code-typing' },
    ],
  },
  {
    title: 'nav.more',
    pages: [
      { slug: 'examples', title: 'nav.examples-to-build-on', description: 'nav.complete-files-you-can-copy-run-and', keywords: 'search.examples' },
      { slug: 'timelines', title: 'nav.json-project-timelines', description: 'nav.deprecated-kept-for-existing-projects', keywords: 'search.timelines' },
      { slug: 'build-from-source', title: 'nav.build-from-source', description: 'nav.for-contributors-custom-builds-and-linux-users', keywords: 'search.build-from-source' },
      { slug: 'troubleshooting', title: 'nav.troubleshooting', description: 'nav.common-errors-and-how-to-fix-them', keywords: 'search.troubleshooting' },
    ],
  },
] satisfies { title: Message; pages: (Omit<DocPage, 'title' | 'description' | 'keywords'> & { title: Message; description: Message; keywords: Message })[] }[];

export function getDocGroups(locale: Locale): DocGroup[] {
  return groupMessages.map(group => ({
    title: translate(locale, group.title),
    pages: group.pages.map(page => ({ ...page, title: translate(locale, page.title), description: translate(locale, page.description), keywords: translate(locale, page.keywords) })),
  }));
}

export const docGroups = getDocGroups(localeFromPath(typeof window === 'undefined' ? '/en/' : window.location.pathname));
export const docPages: DocPage[] = docGroups.flatMap(group => group.pages);

/** The path of a documentation page. The overview is `/<locale>/docs/`. */
export function docPath(slug: string, locale = localeFromPath(typeof window === 'undefined' ? '/en/' : window.location.pathname)): string {
  return slug === 'overview' ? `/${locale}/docs/` : `/${locale}/docs/${slug}/`;
}
