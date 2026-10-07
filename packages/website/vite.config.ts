import { cpSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig, normalizePath, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { getDocGroups } from './src/docs-nav.ts';
import { richText } from './src/i18n.ts';
import { translate } from './src/catalog.ts';
import { languageRedirect, localeCodes, localeFromPath, type Locale } from './src/locales.ts';
import { showcase } from './src/showcase-catalog.ts';

const escapeHtml = (text: string) => text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/"/g, '&quot;');
const site = 'https://celesta.natsuneko.cat';

/** Samples are editor text, independent of their desktop .celesta/tsconfig. */
function sampleSources(): Plugin {
  const prefix = '\0celesta-sample:';
  const root = fileURLToPath(new URL('../..', import.meta.url));
  return {
    name: 'celesta-sample-sources', enforce: 'pre',
    resolveId(source, importer) {
      if (!source.includes('/examples/') || !source.endsWith('?raw')) return;
      return `${prefix}${encodeURIComponent(relative(root, resolve(importer ? dirname(importer) : '.', source.slice(0, -4))))}.js`;
    },
    load(id) {
      if (!id.startsWith(prefix)) return;
      const path = join(root, decodeURIComponent(id.slice(prefix.length, -3)));
      this.addWatchFile(path);
      return `export default ${JSON.stringify(readFileSync(path, 'utf8'))};`;
    },
  };
}

function localizedHtml(html: string, locale: Locale, slug?: string, showcaseSlug?: string): string {
  const isShowcase = showcaseSlug !== undefined;
  const sample = showcase.find(item => item.slug === showcaseSlug);
  const docs = slug !== undefined;
  const page = getDocGroups(locale).flatMap(group => group.pages).find(page => page.slug === slug);
  const title = isShowcase ? sample ? translate(locale, 'showcase.metadata-sample-title', { title: sample.title }) : translate(locale, 'showcase.metadata-title') : page ? translate(locale, 'docs.metadata.title', { title: page.title }) : translate(locale, docs ? 'docs.metadata.overview-title' : 'home.metadata.title');
  const description = isShowcase ? sample ? translate(locale, `showcase.samples.${sample.slug}.description`) : translate(locale, 'showcase.metadata-description') : page?.description ?? translate(locale, docs ? 'docs.metadata.description' : 'home.metadata.description');
  const suffix = isShowcase ? `showcase/${showcaseSlug === 'overview' ? '' : `${showcaseSlug}/`}` : docs ? `docs/${slug === 'overview' ? '' : `${slug}/`}` : '';
  const path = `/${locale}/${suffix}`;
  const alternatives = localeCodes.map(code => `<link rel="alternate" hreflang="${code}" href="${site}/${code}/${suffix}" />`).join('\n    ');
  return html
    .replace(/<html lang="[^"]*">/, `<html lang="${locale}">`)
    .replace(/<title>.*?<\/title>/, `<title>${escapeHtml(title)}</title>`)
    .replace(/(<meta name="description" content=")[^"]*/, `$1${escapeHtml(description)}`)
    .replace(/(<meta property="og:title" content=")[^"]*/, `$1${escapeHtml(title)}`)
    .replace(/(<meta property="og:description" content=")[^"]*/, `$1${escapeHtml(isShowcase ? description : page?.description ?? translate(locale, docs ? 'docs.metadata.og-description' : 'home.metadata.og-description'))}`)
    .replace(/<noscript>[\s\S]*?<\/noscript>/, renderToStaticMarkup(createElement('noscript', null,
      richText(translate(locale, isShowcase ? 'showcase.metadata-noscript' : docs ? 'docs.metadata.noscript' : 'home.metadata.noscript'), [createElement('a', { href: isShowcase ? 'https://github.com/mika-f/Celesta/tree/main/examples' : 'https://github.com/mika-f/Celesta#readme' })]))))
    .replace(/\s*<link rel="(?:canonical|alternate)"[^>]*>/g, '')
    .replace('</head>', `    <link rel="canonical" href="${site}${path}" />\n    ${alternatives}\n    <link rel="alternate" hreflang="x-default" href="${docs || isShowcase ? `${site}/en/${suffix}` : `${site}/`}" />\n  </head>`);
}

/** Build real HTML entries for every locale and documentation chapter. */
function sitePages(): Plugin {
  let outDir = 'dist';
  const slugs = getDocGroups('en').flatMap(group => group.pages).map(page => page.slug);
  const route = (path: string) => {
    const locale = localeFromPath(path);
    const prefix = `/${locale}/`;
    const localPath = path.startsWith(prefix) ? path.slice(prefix.length - 1) : path;
    const match = /^\/docs\/(?:([a-z0-9-]+)\/?)?$/.exec(localPath);
    const showcaseMatch = /^\/showcase\/(?:([a-z0-9-]+)\/?)?$/.exec(localPath);
    return { locale, slug: localPath === '/docs/index.html' ? 'overview' : match ? match[1] ?? 'overview' : undefined, showcaseSlug: localPath === '/showcase/index.html' ? 'overview' : showcaseMatch ? showcaseMatch[1] ?? 'overview' : undefined, home: localPath === '/' || localPath === '/index.html' };
  };
  return {
    name: 'celesta-site-pages',
    configResolved(config) { outDir = config.build.outDir; },
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        const url = new URL(request.url ?? '/', 'http://localhost');
        const target = languageRedirect(url, request.headers['accept-language'] ?? null);
        if (target) {
          response.writeHead(302, { Location: target.pathname + target.search, Vary: 'Accept-Language', 'Cache-Control': 'no-store' });
          response.end();
          return;
        }
        const { slug, home, showcaseSlug } = route(url.pathname);
        if (home) request.url = '/index.html' + url.search;
        else if (slug && (slug === 'overview' || slugs.includes(slug))) request.url = '/docs/index.html' + url.search;
        else if (showcaseSlug && (showcaseSlug === 'overview' || showcase.some(item => item.slug === showcaseSlug))) request.url = '/showcase/index.html' + url.search;
        else if (/^\/showcase-assets\/examples\/(?:afterimage\/assets\/fonts|versus\/bench\/assets\/(?:fonts|licenses)|assets\/dialogue-demo\/portraits)\/[^/]+$/.test(url.pathname)) request.url = '/@fs/' + normalizePath(fileURLToPath(new URL(`../../${url.pathname.slice('/showcase-assets/'.length)}`, import.meta.url)));
        next();
      });
    },
    configurePreviewServer(server) {
      server.middlewares.use((request, response, next) => {
        const url = new URL(request.url ?? '/', 'http://localhost');
        const target = languageRedirect(url, request.headers['accept-language'] ?? null);
        if (!target) { next(); return; }
        response.writeHead(302, { Location: target.pathname + target.search, Vary: 'Accept-Language', 'Cache-Control': 'no-store' });
        response.end();
      });
    },
    transformIndexHtml(html, context) {
      const { locale, slug, showcaseSlug } = route((context.originalUrl ?? context.path).split(/[?#]/)[0]);
      return localizedHtml(html, locale, slug, showcaseSlug);
    },
    writeBundle() {
      const home = readFileSync(join(outDir, 'index.html'), 'utf8');
      const docs = readFileSync(join(outDir, 'docs', 'index.html'), 'utf8');
      const gallery = readFileSync(join(outDir, 'showcase', 'index.html'), 'utf8');
      function write(path: string, html: string) {
        const target = join(outDir, path, 'index.html');
        mkdirSync(dirname(target), { recursive: true });
        writeFileSync(target, html);
      }
      for (const locale of localeCodes) {
        write(locale, localizedHtml(home, locale));
        for (const slug of ['overview', ...slugs]) write(`${locale}/docs/${slug === 'overview' ? '' : slug}`, localizedHtml(docs, locale, slug));
        for (const slug of ['overview', ...showcase.map(item => item.slug)]) write(`${locale}/showcase/${slug === 'overview' ? '' : slug}`, localizedHtml(gallery, locale, undefined, slug));
        const notFound = createElement('html', { lang: locale },
          createElement('head', null,
            createElement('meta', { charSet: 'utf-8' }),
            createElement('meta', { name: 'viewport', content: 'width=device-width, initial-scale=1' }),
            createElement('title', null, translate(locale, 'not-found.title'))),
          createElement('body', { style: { background: '#0a0a0a', color: '#ededeb', fontFamily: 'Helvetica, Arial, sans-serif', padding: '12vw' } },
            createElement('p', null, '404'),
            createElement('h1', null, translate(locale, 'not-found.heading')),
            createElement('p', null, translate(locale, 'not-found.description')),
            createElement('a', { href: `/${locale}/`, style: { color: 'inherit' } }, translate(locale, 'not-found.back'))));
        writeFileSync(join(outDir, locale, '404.html'), '<!doctype html>\n' + renderToStaticMarkup(notFound));
      }
      // Preserve existing English documentation URLs and hash links.
      for (const slug of slugs) write(`docs/${slug}`, localizedHtml(docs, 'en', slug));
      for (const sample of showcase) write(`showcase/${sample.slug}`, localizedHtml(gallery, 'en', undefined, sample.slug));
      for (const path of ['examples/afterimage/assets/fonts', 'examples/versus/bench/assets/fonts', 'examples/versus/bench/assets/licenses', 'examples/assets/dialogue-demo/portraits']) {
        cpSync(fileURLToPath(new URL(`../../${path}`, import.meta.url)), join(outDir, 'showcase-assets', path), { recursive: true });
      }
    },
  };
}

export default defineConfig({
  plugins: [sampleSources(), react(), tailwindcss(), sitePages()],
  build: { rolldownOptions: { input: ['index.html', 'docs/index.html', 'showcase/index.html'] } },
});
