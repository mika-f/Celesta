import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    // Tests are TypeScript; `pnpm test` type-checks them first (test/tsconfig.json).
    include: ['test/**/*.test.ts'],
    environment: 'node',
    // Rendering a score takes a second or so; a busy runner can take longer.
    testTimeout: 60_000,
    server: {
      deps: {
        external: [/\/dist\//],
      },
    },
  },
});
