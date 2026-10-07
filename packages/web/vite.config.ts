import { defineConfig } from 'vite';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  base: './',
  define: { 'process.env.NODE_ENV': JSON.stringify('production') },
  resolve: { alias: [
    { find: /^\.\/entry-dir$/, replacement: fileURLToPath(new URL('../react/src/entry-dir.browser.ts', import.meta.url)) },
    { find: '@celesta/react', replacement: fileURLToPath(new URL('../react/src/browser.ts', import.meta.url)) },
  ] },
  build: {
    lib: { entry: 'src/index.ts', formats: ['es'], fileName: 'index' },
    emptyOutDir: true,
    assetsInlineLimit: 0,
    target: 'es2022',
  },
});
