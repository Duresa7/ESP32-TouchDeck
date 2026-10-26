import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { decodeIcon, fileId, searchIcons, type BuiltinIcon } from './icons.ts';

const golden = new Uint8Array(readFileSync(join(import.meta.dirname, '..', '..', '..', 'crates', 'td-host', 'tests', 'fixtures', 'icon-64.tdi')));

test('a TDI1 file decodes to the pixels it packs', () => {
  const rgba = decodeIcon(golden)!;
  assert.equal(rgba.length, 64 * 64 * 4);
  // (63, 63) packs r8 255, g8 255, b8 0, alpha 255 inside no square, (x + y) even, so alpha 160.
  const at = (x: number, y: number) => Array.from(rgba.subarray((y * 64 + x) * 4, (y * 64 + x) * 4 + 4));
  assert.deepEqual(at(63, 63), [255, 255, 0, 160]);
  assert.deepEqual(at(32, 32).slice(3), [255]);
  assert.deepEqual(at(1, 0).slice(3), [0]);
  assert.equal(decodeIcon(golden.subarray(1)), null);
});

test('only file: icons name a cached file', () => {
  assert.equal(fileId('file:0123456789abcdef'), '0123456789abcdef');
  assert.equal(fileId('lv:mute'), null);
  assert.equal(fileId(null), null);
});

test('search matches names, labels, and tags', () => {
  const icons: BuiltinIcon[] = [
    { name: 'terminal', label: 'Terminal', tags: ['shell'], id: 'a' },
    { name: 'browser', label: 'Browser', tags: ['web', 'world'], id: 'b' },
  ];
  assert.deepEqual(searchIcons(icons, 'term').map((i) => i.name), ['terminal']);
  assert.deepEqual(searchIcons(icons, 'WEB').map((i) => i.name), ['browser']);
  assert.equal(searchIcons(icons, '').length, 2);
  assert.equal(searchIcons(icons, 'shell web').length, 0);
});
