import { defineConfig } from '@playwright/test';
export default defineConfig({ testDir: './tests', testMatch: '**/*.spec.ts', workers: 1,
  outputDir: '../../.verify/72/playwright', timeout: 30_000 });
