import { describe, expect, test } from 'vitest';
import { durationLabel, START_AFTER_S, startAfterChoices } from './photo-frame.ts';

describe('the idle time', () => {
  test.each([
    [10, '10 s'],
    [30, '30 s'],
    [60, '1 min'],
    [90, '90 s'],
    [300, '5 min'],
    [5400, '90 min'],
    [3600, '1 hr'],
    [86400, '24 hr'],
  ])('%i seconds reads %s', (seconds, label) => {
    expect(durationLabel(seconds)).toBe(label);
  });

  test('a preset keeps the list as it is', () => {
    expect(startAfterChoices(60)).toEqual(START_AFTER_S);
  });

  test('a time set some other way joins the list in order', () => {
    expect(startAfterChoices(45)).toEqual([30, 45, 60, 120, 300, 600, 900, 1800, 3600]);
    expect(startAfterChoices(7200).at(-1)).toBe(7200);
  });
});
