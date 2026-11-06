import { expect, test } from 'vitest';
import { readGif } from '../editor/gif.ts';
import { fakeGif } from './fake-gif.ts';

test('the fake GIF is a looping GIF of the size asked for, at 10 fps', () => {
  const read = readGif(fakeGif({ seed: 7, width: 128, height: 92 }));
  expect(read?.info).toEqual({ width: 128, height: 92, frames: 8, durationMs: 800 });
});

test('a different seed gives different bytes', () => {
  expect(fakeGif({ seed: 1, width: 16, height: 16 })).not.toEqual(fakeGif({ seed: 2, width: 16, height: 16 }));
});
