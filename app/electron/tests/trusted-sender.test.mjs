import { test } from 'node:test';
import assert from 'node:assert/strict';
import { trustedSender } from '../dist/trusted-sender.js';

const frame = {};
const window = { webContents: { mainFrame: frame } };
const valid = { sender: window.webContents, frame, origin: 'https://companion.invalid', window,
  closing: false, expectedOrigin: 'https://companion.invalid' };

test('owning top frame at Companion origin is authorized', () => assert.equal(trustedSender(valid), true));
for (const [name, changed] of Object.entries({
  'wrong webContents': { sender: {} },
  'subframe': { frame: {} },
  'foreign origin on actual top frame': { origin: 'https://foreign.invalid' },
  'closing window': { closing: true },
  'missing window': { window: undefined },
  'missing session': { expectedOrigin: undefined },
})) test(`rejects ${name} independently`, () => assert.equal(trustedSender({ ...valid, ...changed }), false));
