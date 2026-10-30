import { expect, type Page } from '@playwright/test';
import type { AssetSource } from '../src/companion/api.ts';
import type { FakeSearch } from '../src/companion/fake-assets.ts';
import { openStarter } from './inspector-helpers.ts';

export async function openCopyTile(page: Page, path = '/') {
  await openStarter(page, path);
  await page.locator('[data-testid="tile"][data-id="copy"]').click();
  await expect(page.getByTestId('tile-icon')).toBeVisible();
}

export const picker = (page: Page) => page.getByTestId('icon-picker');

export async function openPicker(page: Page) {
  await page.getByTestId('icon-change').click();
  await expect(picker(page).getByTestId('icon-grid')).toBeVisible();
}

export const option = (page: Page, key: string) => picker(page).locator(`[data-testid="icon-option"][data-key="${key}"]`);

export const searches = (page: Page): Promise<FakeSearch[]> => page.evaluate(() => window.touchdeckDemo?.assets.searches() ?? []);

export const created = (page: Page): Promise<AssetSource[]> => page.evaluate(() => window.touchdeckDemo?.assets.created() ?? []);

export async function storedTile(page: Page, tile: string) {
  const text = await page.evaluate(() => localStorage.getItem('touchdeck.layout-draft.mock0001.landscape'));
  const draft: { layout: string } = JSON.parse(text ?? '{"layout":"null"}');
  const layout: { schema: number; pages: { kind: string; tiles?: { id: string; icon: string | null; asset?: string | null }[] }[] } | null = JSON.parse(draft.layout);
  return { schema: layout?.schema, tile: layout?.pages.flatMap((p) => p.tiles ?? []).find((t) => t.id === tile) };
}
