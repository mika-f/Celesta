import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { defineConfig, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { getDocGroups } from './src/docs-nav.ts';
import { richText } from './src/i18n.ts';
import { translate } from './src/catalog.ts';
import { languageRedirect, localeCodes, localeFromPath, type Locale } from './src/locales.ts';

const escapeHtml = (text: string) => text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/"/g, '&quot;');
const site = 'https://celesta.natsuneko.cat';

function localizedHtml(html: string, locale: Locale, slug?: string): string {
  const docs = slug !== undefined;
  const page = getDocGroups(locale).flatMap(group => group.pages).find(page => page.slug === slug);
  const title = page ? translate(locale, 'docs-title', { title: page.title }) : translate(locale, docs ? 'docs-overview-title' : 'home-title');
  const description = page?.description ?? translate(locale, docs ? 'docs-description' : 'home-description');
  const suffix = docs ? `docs/${slug === 'overview' ? '' : `${slug}/`}` : '';
  const path = `/${locale}/${suffix}`;
  const alternatives = localeCodes.map(code => `<link rel="alternate" hreflang="${code}" href="${site}/${code}/${suffix}" />`).join('\n    ');
  return html
    .replace(/<html lang="[^"]*">/, `<html lang="${locale}">`)
    .replace(/<title>.*?<\/title>/, `<title>${escapeHtml(title)}</title>`)
    .replace(/(<meta name="description" content=")[^"]*/, `$1${escapeHtml(description)}`)
    .replace(/(<meta property="og:title" content=")[^"]*/, `$1${escapeHtml(title)}`)
    .replace(/(<meta property="og:description" content=")[^"]*/, `$1${escapeHtml(page?.description ?? translate(locale, docs ? 'docs-og-description' : 'home-og-description'))}`)
    .replace(/<noscript>[\s\S]*?<\/noscript>/, renderToStaticMarkup(createElement('noscript', null,
      richText(translate(locale, docs ? 'docs-noscript' : 'home-noscript'), [createElement('a', { href: 'https://github.com/mika-f/Celesta#readme' })]))))
    .replace(/\s*<link rel="(?:canonical|alternate)"[^>]*>/g, '')
    .replace('</head>', `    <link rel="canonical" href="${site}${path}" />\n    ${alternatives}\n    <link rel="alternate" hreflang="x-default" href="${docs ? `${site}/en/${suffix}` : `${site}/`}" />\n  </head>`);
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
    return { locale, slug: localPath === '/docs/index.html' ? 'overview' : match ? match[1] ?? 'overview' : undefined, home: localPath === '/' || localPath === '/index.html' };
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
        const { slug, home } = route(url.pathname);
        if (home) request.url = '/index.html' + url.search;
        else if (slug && (slug === 'overview' || slugs.includes(slug))) request.url = '/docs/index.html' + url.search;
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
      const { locale, slug } = route((context.originalUrl ?? context.path).split(/[?#]/)[0]);
      return localizedHtml(html, locale, slug);
    },
    writeBundle() {
      const home = readFileSync(join(outDir, 'index.html'), 'utf8');
      const docs = readFileSync(join(outDir, 'docs', 'index.html'), 'utf8');
      function write(path: string, html: string) {
        const target = join(outDir, path, 'index.html');
        mkdirSync(dirname(target), { recursive: true });
        writeFileSync(target, html);
      }
      for (const locale of localeCodes) {
        write(locale, localizedHtml(home, locale));
        for (const slug of ['overview', ...slugs]) write(`${locale}/docs/${slug === 'overview' ? '' : slug}`, localizedHtml(docs, locale, slug));
        const notFound = createElement('html', { lang: locale },
          createElement('head', null,
            createElement('meta', { charSet: 'utf-8' }),
            createElement('meta', { name: 'viewport', content: 'width=device-width, initial-scale=1' }),
            createElement('title', null, translate(locale, 'not-found-title'))),
          createElement('body', { style: { background: '#0a0a0a', color: '#ededeb', fontFamily: 'Helvetica, Arial, sans-serif', padding: '12vw' } },
            createElement('p', null, '404'),
            createElement('h1', null, translate(locale, 'not-found-heading')),
            createElement('p', null, translate(locale, 'not-found-description')),
            createElement('a', { href: `/${locale}/`, style: { color: 'inherit' } }, translate(locale, 'not-found-back'))));
        writeFileSync(join(outDir, locale, '404.html'), '<!doctype html>\n' + renderToStaticMarkup(notFound));
      }
      // Preserve existing English documentation URLs and hash links.
      for (const slug of slugs) write(`docs/${slug}`, localizedHtml(docs, 'en', slug));
    },
  };
}

export default defineConfig({
  plugins: [react(), tailwindcss(), sitePages()],
  build: { rolldownOptions: { input: ['index.html', 'docs/index.html'] } },
});
