import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    // `pnpm test` type-checks the tests first (test/tsconfig.json).
    include: ['test/**/*.test.mts'],
    environment: 'node',
    server: {
      deps: {
        // The workspace packages' dist/ is CommonJS; let Node load it so the
        // module and the tests share one `@celesta/react`.
        external: [/\/dist\//],
      },
    },
  },
});
