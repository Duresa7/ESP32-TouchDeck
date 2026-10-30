import { defineConfig, devices } from '@playwright/test';

// Component and e2e specs run against `pnpm dev:demo`, the UI on the fake Companion.
// On Windows they drive the installed Microsoft Edge, so no browser download is needed.
export default defineConfig({
  testDir: './e2e',
  testIgnore: '**/*companion.spec.ts',
  fullyParallel: true,
  workers: process.env.CI ? 1 : undefined,
  forbidOnly: !!process.env.CI,
  reporter: [['list']],
  use: {
    baseURL: 'http://127.0.0.1:5174/',
    viewport: { width: 1180, height: 760 },
    trace: 'retain-on-failure',
  },
  projects: [
    {
      name: process.platform === 'win32' ? 'edge' : 'chromium',
      use: { ...devices['Desktop Chrome'], viewport: { width: 1180, height: 760 }, channel: process.platform === 'win32' ? 'msedge' : undefined },
    },
  ],
  webServer: {
    command: 'pnpm dev:demo',
    url: 'http://127.0.0.1:5174/',
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
