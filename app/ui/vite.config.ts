import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import react from '@vitejs/plugin-react';
import type { Plugin } from 'vite';
import { defineConfig } from 'vitest/config';
import { companionDev } from './dev/companion.ts';

const require = createRequire(import.meta.url);

// Archivo ships under the SIL Open Font License, which travels with the font
// files. The build writes it into assets/, where Vite puts the woff2 files.
function archivoLicense(): Plugin {
  return {
    name: 'archivo-license',
    apply: 'build',
    generateBundle() {
      const license = require.resolve('@fontsource-variable/archivo/LICENSE');
      this.emitFile({ type: 'asset', fileName: 'assets/Archivo-OFL.txt', source: readFileSync(license) });
    },
  };
}

export default defineConfig({
  plugins: [react(), archivoLicense(), companionDev()],
  clearScreen: false,
  server: { host: '127.0.0.1', port: 5174, strictPort: true },
  preview: { host: '127.0.0.1', port: 5174, strictPort: true },
  build: { target: 'es2022' },
  test: {
    include: ['src/**/*.test.ts'],
    environment: 'node',
  },
});
