import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    // `pnpm test` type-checks the tests and the helpers first (test/tsconfig.json).
    include: ['test/**/*.test.mts'],
    environment: 'node',
  },
});
