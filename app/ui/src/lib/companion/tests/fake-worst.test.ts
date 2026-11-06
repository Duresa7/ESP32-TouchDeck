import assert from 'node:assert/strict';
import { test } from 'vitest';
import { nameBreak } from '../../device/settings.ts';
import { parseLayout } from '../../editor/validate.ts';
import { LONG_WORD, WORST_DEVICE_NAMES, WORST_LABELS, worstDevices, worstLayout, worstLayouts } from '../fake.ts';

test('every worst Device name is one the Device accepts', () => {
  for (const name of WORST_DEVICE_NAMES) assert.equal(nameBreak(name), null, name);
  assert.ok(WORST_DEVICE_NAMES.some((name) => new TextEncoder().encode(name).length >= 46));
});

test('the worst Layouts pass the Layout rules the Companion and Firmware apply', () => {
  const parsed = parseLayout(JSON.stringify(worstLayout()));
  assert.ok(parsed.ok, JSON.stringify(!parsed.ok && parsed.errors));
  assert.equal(parsed.layout.pages.length, 8);
  const first = parsed.layout.pages[0];
  assert.equal(first.kind === 'grid' && first.tiles.length, 64);
  for (const [device, layout] of Object.entries(worstLayouts(worstDevices(10)))) {
    const each = parseLayout(JSON.stringify(layout));
    assert.ok(each.ok, `${device}: ${JSON.stringify(!each.ok && each.errors)}`);
  }
});

test('the worst labels sit at the 16-character limit and the long word has 80', () => {
  assert.ok(WORST_LABELS.every((label) => [...label].length <= 16));
  assert.ok(WORST_LABELS.some((label) => [...label].length === 16));
  assert.equal(LONG_WORD.length, 80);
  assert.ok(!/\s/.test(LONG_WORD));
});

test('ten worst Devices, and all of them offline on request', () => {
  assert.equal(worstDevices(10).length, 10);
  assert.ok(worstDevices(10, { offline: true }).every((d) => d.link.state === 'lost'));
  assert.equal(worstDevices(0).length, 0);
});
