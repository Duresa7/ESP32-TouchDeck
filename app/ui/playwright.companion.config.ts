import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from '@playwright/test';
import demo from './playwright.config.ts';

// `pnpm e2e:companion` runs companion.spec.ts against the real Companion with three mock
// Devices, through `pnpm dev` and its /api proxy. The Companion gets its own data folder
// and a fixed port, so it runs beside the one installed on this PC.
const COMPANION_PORT = 50734;
const UI_PORT = 5175;
const DATA_DIR = fileURLToPath(new URL('../target/ui-e2e-companion', import.meta.url));

mkdirSync(DATA_DIR, { recursive: true });
writeFileSync(join(DATA_DIR, 'companion-settings.json'), JSON.stringify({ port: COMPANION_PORT }));

const env = { TOUCHDECK_APP_DATA_DIR: DATA_DIR };

export default defineConfig({
  ...demo,
  testIgnore: [],
  testMatch: ['companion.spec.ts', 'pairing.companion.spec.ts', 'integrations.companion.spec.ts'],
  use: { ...demo.use, baseURL: `http://127.0.0.1:${UI_PORT}/` },
  // In order: Vite reads the Companion's port and token once it's up.
  webServer: [
    {
      command: 'cargo run -p td-companion --features test-hooks -- --mock-devices 3',
      cwd: '..',
      env,
      url: `http://127.0.0.1:${COMPANION_PORT}/api/v1/system/ping`,
      reuseExistingServer: !process.env.CI,
      // A cold cargo build takes minutes.
      timeout: 600_000,
    },
    {
      command: `pnpm dev --port ${UI_PORT}`,
      env,
      url: `http://127.0.0.1:${UI_PORT}/`,
      reuseExistingServer: !process.env.CI,
      timeout: 60_000,
    },
  ],
});
