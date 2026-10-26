import { readdirSync } from 'node:fs';
import { basename, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const TAGS: Record<string, string> = { win32: 'win', linux: 'linux' };

/**
 * Picks the specs for one operating system. `name.spec.ts` runs everywhere,
 * `name.win.spec.ts` only on Windows, and `name.linux.spec.ts` only on Linux.
 */
export function selectSpecs(files: string[], platform: string, only?: string): string[] {
  const own = TAGS[platform];
  return files
    .filter((file) => {
      const parts = basename(file).split('.');
      if (parts.slice(-2).join('.') !== 'spec.ts' || parts.length > 4) return false;
      const tag = parts.length === 4 ? parts[1] : undefined;
      return (tag === undefined || tag === own) && (!only || parts[0] === only);
    })
    .sort();
}

export function specsFor(platform: string, only?: string): string[] {
  const directory = join(dirname(fileURLToPath(import.meta.url)), 'specs');
  return selectSpecs(
    readdirSync(directory).map((file) => join(directory, file)),
    platform,
    only,
  );
}
