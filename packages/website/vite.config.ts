import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { defineConfig, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { docPages } from './src/docs-nav.ts';

const escapeHtml = (text: string) => text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/"/g, '&quot;');

/**
 * Every documentation page is its own address, `/docs/<slug>/`, backed by the
 * same React entry. In development the entry answers for each of them; in a
 * build, `dist/docs/index.html` is copied to each page's folder with that
 * page's title and description, so direct visits work without an SPA fallback.
 */
function docsPages(): Plugin {
  let outDir = 'dist';
  return {
    name: 'celesta-docs-pages',
    configResolved(config) {
      outDir = config.build.outDir;
    },
    configureServer(server) {
      server.middlewares.use((request, _response, next) => {
        const path = (request.url ?? '').split(/[?#]/)[0];
        if (/^\/docs\/[a-z0-9-]+\/?$/.test(path)) request.url = '/docs/index.html';
        next();
      });
    },
    writeBundle() {
      const entry = join(outDir, 'docs', 'index.html');
      const html = readFileSync(entry, 'utf8');
      for (const { slug, title, description } of docPages) {
        const heading = escapeHtml(`${title} — Celesta documentation`);
        const summary = escapeHtml(description);
        const page = html
          .replace(/<title>.*?<\/title>/, `<title>${heading}</title>`)
          .replace(/(<meta name="description" content=")[^"]*/, `$1${summary}`)
          .replace(/(<meta property="og:title" content=")[^"]*/, `$1${heading}`)
          .replace(/(<meta property="og:description" content=")[^"]*/, `$1${summary}`);
        const target = join(dirname(entry), slug, 'index.html');
        mkdirSync(dirname(target), { recursive: true });
        writeFileSync(target, page);
      }
    },
  };
}

export default defineConfig({
  plugins: [react(), tailwindcss(), docsPages()],
  build: {
    rolldownOptions: { input: ['index.html', 'docs/index.html'] },
  },
});
