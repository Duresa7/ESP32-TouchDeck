import { expect, type Page } from '@playwright/test';
import type { Action, Layout } from '../src/lib/editor/validate.ts';
import { openLayout, waitEvent } from './layout-helpers.ts';

export const DESK = 'mock0001';

/** Opens the Layout screen on the Starter Layout with the Shortcuts Page showing. */
export async function openStarter(page: Page) {
  await openLayout(page);
  await page.getByTestId('start-starter').click();
  await page.getByTestId('page-tab').filter({ hasText: 'Shortcuts' }).click();
}

/** Places a Tile of `kind` on an empty cell of the current Page and returns its id. */
export async function addTile(page: Page, kind: string, row = 2, col = 2): Promise<string> {
  await page.locator(`[data-testid="empty-cell"][data-row="${row}"][data-col="${col}"]`).click();
  await page.locator(`[data-testid="add-kind"][data-kind="${kind}"]`).click();
  const id = await page.getByTestId('tile-fields').getAttribute('data-id');
  if (id === null) throw new Error(`Add Tile placed no ${kind} Tile`);
  return id;
}

export async function send(page: Page) {
  await page.getByTestId('push').click();
  await waitEvent(page, 'pushed');
}

/** A Tile's action as the demo's Desk stores it, read through the fake's test route. */
export async function storedAction(page: Page, pageId: string, tile: string, device = DESK): Promise<Action | undefined> {
  const record: { layouts: { landscape: Layout | null } } = await page.evaluate(async (id) => (await fetch(`/api/v1/test/devices/${id}/record`)).json(), device);
  const grid = record.layouts.landscape?.pages.find((p) => p.id === pageId);
  return grid?.kind === 'grid' ? grid.tiles.find((t) => t.id === tile)?.action : undefined;
}

async function post(page: Page, path: string, body: unknown) {
  const status = await page.evaluate(async ([to, text]) => (await fetch(to, { method: 'POST', body: text })).status, [path, JSON.stringify(body)] as const);
  expect(status).toBe(204);
}

/** A tap on a Tile of the Device, as the Companion's `test.press` makes one. */
export const press = (page: Page, pageId: string, tile: string, kind: string, device = DESK) =>
  post(page, `/api/v1/test/devices/${device}/press`, { page: pageId, tile, kind });

/** What the dialog on the PC answers next, as `test.approvals_script` sets it. */
export const script = (page: Page, answers: string[]) => post(page, '/api/v1/test/approvals/script', { answers });

/** Key reports from the Device, as `test.emit` sends them. */
export const emitKeys = (page: Page, events: Record<string, unknown>[], device = DESK) =>
  post(page, `/api/v1/test/devices/${device}/emit`, { events: events.map((event) => ({ m: 'key.report', ...event })) });

/** Keeps the status of each fetch the page makes to a path matching `route` in `data-in-page-fetch` on <body>. */
export async function watchInPageFetch(page: Page, route: RegExp) {
  await page.evaluate((source) => {
    const through = window.fetch;
    window.fetch = async (input, init) => {
      const response = await through(input, init);
      if (new RegExp(source).test(String(input))) document.body.dataset.inPageFetch = `${document.body.dataset.inPageFetch ?? ''} ${response.status}`.trim();
      return response;
    };
  }, route.source);
}

export async function setCommand(page: Page, program: string, args: string[]) {
  await page.getByTestId('action-program').fill(program);
  await page.getByTestId('action-args').fill(args.join('\n'));
}
