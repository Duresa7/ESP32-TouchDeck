import assert from 'node:assert/strict';
import { test } from 'node:test';
import { append, classify, type ConsoleEntry } from './console.ts';

test('an @td reply after a prompt is a reply with its ok flag', () => {
  assert.deepEqual(classify('td> @td {"cmd":"version","ok":true,"project_name":"touchdeck"}\r'), {
    kind: 'reply',
    text: '{"cmd":"version","ok":true,"project_name":"touchdeck"}',
    ok: true,
  });
  assert.deepEqual(classify('@td {"cmd":"nope","ok":false,"error":"unknown command"}'), {
    kind: 'reply',
    text: '{"cmd":"nope","ok":false,"error":"unknown command"}',
    ok: false,
  });
});

test('log lines keep their text without the prompt, and a bare prompt is nothing', () => {
  assert.deepEqual(classify('td> I (2000) touchdeck: alive 1, free heap 4194304 bytes'), {
    kind: 'log',
    text: 'I (2000) touchdeck: alive 1, free heap 4194304 bytes',
  });
  assert.equal(classify('td> '), null);
  assert.deepEqual(classify('@td {not json}'), { kind: 'log', text: '@td {not json}' });
});

test('the pane keeps the newest entries up to the limit', () => {
  let entries: ConsoleEntry[] = [];
  for (let i = 0; i < 5; i++) entries = append(entries, { kind: 'log', text: String(i) }, 3);
  assert.deepEqual(entries.map((entry) => entry.text), ['2', '3', '4']);
  assert.equal(append(entries, null, 3), entries);
});
