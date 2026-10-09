import { defineConfig } from 'vite';
import { fileURLToPath } from 'node:url';

const packages = fileURLToPath(new URL('..', import.meta.url));

export default defineConfig({
  base: './',
  define: { 'process.env.NODE_ENV': JSON.stringify('production') },
  // Every Celesta package from source, so the worker and the packages share
  // one copy of the core runtime and its contexts.
  resolve: { alias: [
    { find: /^\.\/entry-dir$/, replacement: `${packages}/react/src/entry-dir.browser.ts` },
    { find: /^@celesta\/react$/, replacement: `${packages}/react/src/browser.ts` },
    { find: /^@celesta\/react\/internal$/, replacement: `${packages}/react/src/internal.ts` },
    { find: /^@celesta\/([a-z-]+)$/, replacement: `${packages}/$1/src/index.ts` },
  ] },
  build: {
    lib: { entry: 'src/index.ts', formats: ['es'], fileName: 'index' },
    emptyOutDir: true,
    assetsInlineLimit: 0,
    target: 'es2022',
  },
});
