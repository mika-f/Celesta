import { fileURLToPath } from 'node:url';
import { defineConfig } from '@natsuneko-laboratory/makit';
import { remarkArrowIcons } from './plugins/remark-arrow-icons.ts';
import { remarkDownloadLinks } from './plugins/remark-download-links.ts';
import { remarkInclude } from './plugins/remark-include.ts';
import { repository } from './site/links.ts';

export default defineConfig({
  title: 'Celesta',
  description:
    'Celesta renders React components to video. Scrub to any frame in the browser or the desktop app, then export an MP4. Free and open source.',
  siteUrl: 'https://celesta.natsuneko.cat',
  sourceDir: 'content',
  outDir: 'dist',
  i18n: {
    defaultLocale: 'en',
    locales: [
      { locale: 'en', label: 'English' },
      { locale: 'ja', label: '日本語' },
    ],
    root: { behavior: 'detect' },
  },
  header: {
    links: [{ label: 'GitHub', href: repository, external: true }],
  },
  theme: { extends: './theme', colorScheme: 'system', accentColor: '#3d7bff', radius: 'none' },
  navigation: {
    auto: { numericPrefixes: true },
    pagination: { enabled: true, crossSection: true },
  },
  markdown: {
    code: { copyButton: true },
    remarkPlugins: [[remarkInclude, { root: fileURLToPath(new URL('.', import.meta.url)) }], remarkDownloadLinks, remarkArrowIcons],
    // Every token color reaches 4.5:1 on the Noir code panels (one light override in theme/docs.css).
    shiki: { themes: { light: 'github-light-high-contrast', dark: 'github-dark-high-contrast' } },
  },
  llms: { enabled: true },
  github: { repository: 'mika-f/Celesta', directory: 'packages/website' },
});
