import { defineConfig } from '@playwright/test';
export default defineConfig({ testDir: './tests', testMatch: '**/*.spec.ts', workers: 1,
  globalSetup: './e2e-setup.ts', outputDir: '../../.verify/72/playwright', timeout: 30_000 });
