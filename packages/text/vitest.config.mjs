import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    // `pnpm test` type-checks the tests first (test/tsconfig.json).
    include: ['test/**/*.test.mts'],
    environment: 'node',
    server: {
      deps: {
        // dist/ is CommonJS; let Node load it so the tests and src/ share one
        // instance of @celesta.studio/react (the text measurer the tests install).
        external: [/\/dist\//],
      },
    },
  },
});
