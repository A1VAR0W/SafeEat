import { defineConfig } from 'vitest/config';
export default defineConfig({
  test: {
    include: ['apps/api/**/*.integration.test.ts'],
    testTimeout: 20000,
    fileParallelism: false,
  },
});
