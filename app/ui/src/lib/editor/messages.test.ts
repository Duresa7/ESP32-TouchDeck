import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'vitest';
import { friendlyFileError, friendlyRefusal, plural } from './messages.ts';
import { parseLayout } from './validate.ts';

const parsed = parseLayout(readFileSync(join(import.meta.dirname, '..', '..', '..', '..', '..', 'schema', 'examples', 'dev.json')));
assert.ok(parsed.ok);
const dev = parsed.layout;

test('a refusal names the Tile in the way and the size that fits', () => {
  assert.equal(friendlyRefusal('overlaps quiet', dev), 'That spot is taken by “Mute”. Pick an empty one.');
  assert.equal(friendlyRefusal('overlaps nobody', dev), 'That spot is taken. Pick an empty one.');
  assert.equal(friendlyRefusal('outside the 3x4 grid'), "That doesn't fit. This Page has 3 rows and 4 columns.");
  assert.equal(friendlyRefusal('outside the 1x1 grid'), "That doesn't fit. This Page has 1 row and 1 column.");
  assert.equal(friendlyRefusal('grid.too-small: quiet', dev), "The grid can't shrink past “Mute”. Move or remove it first.");
});

test('rule keys read as what to do', () => {
  assert.equal(friendlyRefusal('label.too-long: /pages/0/tiles/0/label must NOT have more than 16 characters (maxLength)'), 'Tile labels can be up to 16 characters.');
  assert.equal(friendlyRefusal('text.empty: /pages/1/tiles/0/action/path must NOT have fewer than 1 characters (minLength)'), 'Choose an app.');
  assert.equal(friendlyRefusal('something.new: whatever'), "That change breaks a Layout rule. See Details.");
  assert.equal(friendlyFileError('notes.txt', 'file.json'), '“notes.txt” isn\'t a Layout file. Nothing changed.');
});

test('counts read naturally', () => {
  assert.equal(plural(1, 'button'), '1 button');
  assert.equal(plural(4, 'page'), '4 pages');
});
