import { readFileSync } from 'node:fs';
import { expect, type Locator, type Page } from '@playwright/test';
import { geometry, screenSize, type Orientation } from '../src/lib/deck/geometry.ts';
import { parseLayout, type GridPage, type Layout, type Page as LayoutPage } from '../src/lib/editor/validate.ts';

export async function openLayout(page: Page, path = '/') {
  await page.addInitScript(() => {
    delete window.showSaveFilePicker;
  });
  await page.goto(path);
  await expect(page.getByRole('list', { name: 'Devices' })).toBeVisible();
  await page.getByRole('button', { name: 'Layout', exact: true }).click();
  await expect(page.getByTestId('editor')).toBeVisible();
  await page.evaluate(() => document.fonts.ready);
}

export const status = (page: Page) => page.getByTestId('status');

export async function waitEvent(page: Page, event: string, timeout = 10_000): Promise<Locator> {
  await expect(status(page)).toHaveAttribute('data-event', event, { timeout });
  return status(page);
}

export async function importText(page: Page, text: string, name: string) {
  await page.getByTestId('import-file').setInputFiles({ name, mimeType: 'application/json', buffer: Buffer.from(text) });
}

export async function menu(page: Page, item: 'pull' | 'import' | 'export' | 'menu-starter' | 'menu-new' | 'prune') {
  await page.getByTestId('layout-menu').click();
  await page.getByTestId(item).click();
}

export async function exportText(page: Page): Promise<string> {
  const download = page.waitForEvent('download');
  await menu(page, 'export');
  return readFileSync(await (await download).path(), 'utf8');
}

export const tile = (page: Page, id: string) => page.locator(`[data-testid="tile"][data-id="${id}"]`);

export async function placeOf(page: Page, id: string) {
  const el = tile(page, id);
  const read = async (name: string) => Number(await el.getAttribute(name));
  return { row: await read('data-row'), col: await read('data-col'), rowSpan: await read('data-row-span'), colSpan: await read('data-col-span') };
}

export async function cellPoint(page: Page, row: number, col: number) {
  const preview = page.getByTestId('deck-preview');
  const orientation: Orientation = (await preview.getAttribute('data-orientation')) === 'portrait' ? 'portrait' : 'landscape';
  const g = geometry(screenSize(orientation), (await preview.getAttribute('data-full-screen')) === 'true');
  const grid = page.getByTestId('grid');
  const rows = Number(await grid.getAttribute('data-rows'));
  const cols = Number(await grid.getAttribute('data-cols'));
  const box = (await page.getByTestId('deck-screen').boundingBox())!;
  const s = box.width / g.screen.w;
  const cw = (g.grid.w + g.gap) / cols;
  const ch = (g.grid.h + g.gap) / rows;
  return { x: box.x + (g.grid.x + col * cw + (cw - g.gap) / 2) * s, y: box.y + (g.grid.y + row * ch + (ch - g.gap) / 2) * s };
}

export async function drag(page: Page, id: string, row: number, col: number, how: 'move' | 'resize') {
  const at = await placeOf(page, id);
  let from: { x: number; y: number };
  if (how === 'resize') {
    await tile(page, id).click();
    const box = (await page.getByTestId('resize-handle').boundingBox())!;
    from = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  } else {
    from = await cellPoint(page, at.row, at.col);
  }
  const to = await cellPoint(page, row, col);
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + (to.x - from.x) / 4, from.y + (to.y - from.y) / 4, { steps: 3 });
  await page.mouse.move(to.x, to.y, { steps: 6 });
  await page.mouse.up();
}

export async function swipe(page: Page, dx: number) {
  const box = (await page.getByTestId('deck-screen').boundingBox())!;
  const s = Number(await page.getByTestId('deck-screen').getAttribute('data-scale'));
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + dx * s, y, { steps: 8 });
  await page.mouse.up();
}

export function sorted(layout: Layout) {
  return layout.pages.map((page) => (page.kind === 'grid' ? { ...page, tiles: [...page.tiles].sort((a, b) => a.id.localeCompare(b.id)) } : page));
}

export function layoutOf(text: string): Layout {
  const parsed = parseLayout(text);
  if (!parsed.ok) throw new Error(`The Layout breaks ${parsed.errors.map((e) => e.key).join(', ')}`);
  return parsed.layout;
}

export function gridPage(page: LayoutPage | undefined): GridPage {
  if (page?.kind !== 'grid') throw new Error(`${page?.id} is not a grid page`);
  return page;
}
