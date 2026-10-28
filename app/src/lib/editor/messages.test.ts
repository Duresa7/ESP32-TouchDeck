import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { friendlyFileError, friendlyRefusal, plural } from './messages.ts';
import type { Layout } from './validate.ts';

const dev = JSON.parse(readFileSync(join(import.meta.dirname, '..', '..', '..', '..', 'schema', 'examples', 'dev.json'), 'utf8')) as Layout;

test('a refusal names the button in the way and the size that fits', () => {
  assert.equal(friendlyRefusal('overlaps quiet', dev), 'That spot is taken by “Mute”. Pick an empty spot.');
  assert.equal(friendlyRefusal('overlaps nobody', dev), 'That spot is taken. Pick an empty spot.');
  assert.equal(friendlyRefusal('outside the 3x4 grid'), "That doesn't fit. This page has 3 rows and 4 columns.");
  assert.equal(friendlyRefusal('outside the 1x1 grid'), "That doesn't fit. This page has 1 row and 1 column.");
  assert.equal(friendlyRefusal('grid.too-small: quiet', dev), "The grid can't shrink past “Mute”. Move or remove it first.");
});

test('rule keys read as what to do', () => {
  assert.equal(friendlyRefusal('label.too-long: /pages/0/tiles/0/label must NOT have more than 16 characters (maxLength)'), 'Button names can be up to 16 characters.');
  assert.equal(friendlyRefusal('text.empty: /pages/1/tiles/0/action/path must NOT have fewer than 1 characters (minLength)'), 'Choose the app to open.');
  assert.equal(friendlyRefusal('something.new: whatever'), "The deck can't take that change.");
  assert.equal(friendlyFileError('notes.txt', 'file.json'), '“notes.txt” isn\'t a TouchDeck layout file, so nothing changed.');
});

test('counts read naturally', () => {
  assert.equal(plural(1, 'button'), '1 button');
  assert.equal(plural(4, 'page'), '4 pages');
});
