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
    title: 'docs.navigation.groups.getting-started',
    pages: [
      { slug: 'installation', title: 'docs.chapters.installation.metadata.title', description: 'docs.chapters.installation.metadata.description', keywords: 'docs.chapters.installation.metadata.keywords' },
      { slug: 'create-project', title: 'docs.chapters.create-project.metadata.title', description: 'docs.chapters.create-project.metadata.description', keywords: 'docs.chapters.create-project.metadata.keywords' },
      { slug: 'preview', title: 'docs.chapters.preview.metadata.title', description: 'docs.chapters.preview.metadata.description', keywords: 'docs.chapters.preview.metadata.keywords' },
      { slug: 'react-compositions', title: 'docs.chapters.react-compositions.metadata.title', description: 'docs.chapters.react-compositions.metadata.description', keywords: 'docs.chapters.react-compositions.metadata.keywords' },
    ],
  },
  {
    title: 'docs.navigation.groups.guides',
    pages: [
      { slug: 'animation', title: 'docs.chapters.animation.metadata.title', description: 'docs.chapters.animation.metadata.description', keywords: 'docs.chapters.animation.metadata.keywords' },
      { slug: 'motion-toolkit', title: 'docs.chapters.motion-toolkit.metadata.title', description: 'docs.chapters.motion-toolkit.metadata.description', keywords: 'docs.chapters.motion-toolkit.metadata.keywords' },
      { slug: 'text-camera-lines', title: 'docs.chapters.text-camera-lines.metadata.title', description: 'docs.chapters.text-camera-lines.metadata.description', keywords: 'docs.chapters.text-camera-lines.metadata.keywords' },
      { slug: 'layout', title: 'docs.chapters.layout.metadata.title', description: 'docs.chapters.layout.metadata.description', keywords: 'docs.chapters.layout.metadata.keywords' },
      { slug: 'text-fonts', title: 'docs.chapters.text-fonts.metadata.title', description: 'docs.chapters.text-fonts.metadata.description', keywords: 'docs.chapters.text-fonts.metadata.keywords' },
      { slug: 'media', title: 'docs.chapters.media.metadata.title', description: 'docs.chapters.media.metadata.description', keywords: 'docs.chapters.media.metadata.keywords' },
      { slug: 'dialogue', title: 'docs.chapters.dialogue.metadata.title', description: 'docs.chapters.dialogue.metadata.description', keywords: 'docs.chapters.dialogue.metadata.keywords' },
      { slug: 'data', title: 'docs.chapters.data.metadata.title', description: 'docs.chapters.data.metadata.description', keywords: 'docs.chapters.data.metadata.keywords' },
      { slug: 'export', title: 'docs.chapters.export.metadata.title', description: 'docs.chapters.export.metadata.description', keywords: 'docs.chapters.export.metadata.keywords' },
    ],
  },
  {
    title: 'docs.navigation.groups.packages',
    pages: [
      { slug: 'reference', title: 'docs.chapters.reference.metadata.title', description: 'docs.chapters.reference.metadata.description', keywords: 'docs.chapters.reference.metadata.keywords' },
      { slug: 'voicevox', title: 'docs.chapters.voicevox.metadata.title', description: 'docs.chapters.voicevox.metadata.description', keywords: 'docs.chapters.voicevox.metadata.keywords' },
      { slug: 'math', title: 'docs.chapters.math.metadata.title', description: 'docs.chapters.math.metadata.description', keywords: 'docs.chapters.math.metadata.keywords' },
      { slug: 'math-noise', title: 'docs.chapters.math-noise.metadata.title', description: 'docs.chapters.math-noise.metadata.description', keywords: 'docs.chapters.math-noise.metadata.keywords' },
      { slug: 'math-shaping', title: 'docs.chapters.math-shaping.metadata.title', description: 'docs.chapters.math-shaping.metadata.description', keywords: 'docs.chapters.math-shaping.metadata.keywords' },
      { slug: 'code', title: 'docs.chapters.code.metadata.title', description: 'docs.chapters.code.metadata.description', keywords: 'docs.chapters.code.metadata.keywords' },
      { slug: 'code-typing', title: 'docs.chapters.code-typing.metadata.title', description: 'docs.chapters.code-typing.metadata.description', keywords: 'docs.chapters.code-typing.metadata.keywords' },
    ],
  },
  {
    title: 'docs.navigation.groups.more',
    pages: [
      { slug: 'examples', title: 'docs.chapters.examples.metadata.title', description: 'docs.chapters.examples.metadata.description', keywords: 'docs.chapters.examples.metadata.keywords' },
      { slug: 'timelines', title: 'docs.chapters.timelines.metadata.title', description: 'docs.chapters.timelines.metadata.description', keywords: 'docs.chapters.timelines.metadata.keywords' },
      { slug: 'build-from-source', title: 'docs.chapters.build-from-source.metadata.title', description: 'docs.chapters.build-from-source.metadata.description', keywords: 'docs.chapters.build-from-source.metadata.keywords' },
      { slug: 'troubleshooting', title: 'docs.chapters.troubleshooting.metadata.title', description: 'docs.chapters.troubleshooting.metadata.description', keywords: 'docs.chapters.troubleshooting.metadata.keywords' },
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
