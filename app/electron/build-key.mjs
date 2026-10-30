import { writeFile } from 'node:fs/promises';

await writeFile(new URL('./src/updates/key.gen.ts', import.meta.url),
  `export const UPDATER_PUBLIC_KEY = ${JSON.stringify(process.env.TOUCHDECK_UPDATER_PUBKEY ?? '')};\n`);
