import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['test/**/*.test.mjs'],
    environment: 'node',
    server: {
      deps: {
        // dist/ is CommonJS; let Node load it so index.js and the modules the
        // tests import directly (text-measure.js, ...) share one instance.
        external: [/\/dist\//],
      },
    },
  },
});
