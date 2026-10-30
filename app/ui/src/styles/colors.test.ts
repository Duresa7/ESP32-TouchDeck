import { readdirSync, readFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { expect, test } from 'vitest';

const src = join(import.meta.dirname, '..');
const RAW_COLOR = /#[0-9a-f]{3,8}\b|\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch)\(|:\s*(?:white|black)\b/i;

const sheets = readdirSync(src, { recursive: true, encoding: 'utf8' })
  .filter((file) => file.endsWith('.css') && !file.endsWith('tokens.css'))
  .map((file) => join(src, file));

test.each(sheets.map((path) => [relative(src, path).replaceAll('\\', '/'), path]))('%s takes every color from tokens.css', (_, path) => {
  const raw = readFileSync(path, 'utf8')
    .split('\n')
    .map((line, i) => [i + 1, line] as const)
    .filter(([, line]) => RAW_COLOR.test(line));
  expect(raw).toEqual([]);
});
