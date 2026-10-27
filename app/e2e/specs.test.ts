import assert from 'node:assert/strict';
import { test } from 'node:test';
import { selectSpecs, specsFor } from './specs.ts';

const files = ['a/smoke.spec.ts', 'a/notepad.win.spec.ts', 'a/input.linux.spec.ts', 'a/helpers.ts', 'a/console.spec.ts'];

test('Windows runs shared and Windows specs', () => {
  assert.deepEqual(selectSpecs(files, 'win32'), ['a/console.spec.ts', 'a/notepad.win.spec.ts', 'a/smoke.spec.ts']);
});

test('Linux runs shared and Linux specs, never a Windows one', () => {
  assert.deepEqual(selectSpecs(files, 'linux'), ['a/console.spec.ts', 'a/input.linux.spec.ts', 'a/smoke.spec.ts']);
});

test('one spec can be picked by name', () => {
  assert.deepEqual(selectSpecs(files, 'win32', 'smoke'), ['a/smoke.spec.ts']);
  assert.deepEqual(selectSpecs(files, 'linux', 'notepad'), []);
});

test('the bundled specs are all shared', () => {
  const names = (platform: string) => specsFor(platform).map((file) => file.split(/[\\/]/).pop());
  assert.deepEqual(names('win32'), names('linux'));
  assert.deepEqual(names('linux'), ['actions.spec.ts', 'connect.spec.ts', 'console.spec.ts', 'disconnect.spec.ts', 'editor.spec.ts', 'icons.spec.ts', 'keys.spec.ts', 'settings.spec.ts', 'smoke.spec.ts', 'sources-cliproxy.spec.ts', 'sources-install.spec.ts', 'sources.spec.ts', 'update.spec.ts', 'window.spec.ts']);
});
