import { describe, expect, test } from 'vitest';
import { applyTheme } from './theme.ts';
import { createUiStore } from './ui.ts';

const html = () => ({ dataset: {} as DOMStringMap });

describe('theme selection', () => {
  test('Match Windows leaves no data-theme, so prefers-color-scheme decides', () => {
    const root = html();
    applyTheme(root, 'system');
    expect(root.dataset.theme).toBeUndefined();
  });

  test('Light and Dark force their theme whatever Windows says', () => {
    const root = html();
    applyTheme(root, 'dark');
    expect(root.dataset.theme).toBe('dark');
    applyTheme(root, 'light');
    expect(root.dataset.theme).toBe('light');
  });

  test('going back to Match Windows removes the override', () => {
    const root = html();
    applyTheme(root, 'dark');
    applyTheme(root, 'system');
    expect('theme' in root.dataset).toBe(false);
  });

  test('the app starts on Home, following Windows', () => {
    const ui = createUiStore().getState();
    expect(ui.section).toBe('home');
    expect(ui.appearance).toBe('system');
  });
});
