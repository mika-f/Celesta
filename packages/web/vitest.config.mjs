import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  // Browser export loads the ducking curve from source, as vite.config.ts does.
  resolve: { alias: [
    { find: /^@celesta\.studio\/media-utils\/duck$/, replacement: fileURLToPath(new URL('../media-utils/src/duck.ts', import.meta.url)) },
  ] },
  test: {
    // `pnpm test` type-checks the tests first (test/tsconfig.json).
    include: ['test/**/*.test.mts'],
    environment: 'node',
    // The plane tests draw in software, the longest for 20 s or more: far
    // past vitest's 5 s default.
    testTimeout: 120_000,
    // Spies are put back before the next test, as node:test's `t.mock` put
    // them back after theirs.
    restoreMocks: true,
  },
});
