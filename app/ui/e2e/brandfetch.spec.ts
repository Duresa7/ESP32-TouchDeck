import { join } from 'node:path';
import { expect, test, type Page, type Request } from '@playwright/test';
import { SEARCH_DELAY_MS } from '../src/lib/brandfetch/search.ts';
import { created, openCopyTile, openPicker, option, picker, storedTile } from './icon-picker-helpers.ts';

// Every Brandfetch host is intercepted here: these specs never reach Brandfetch.
const MUSIC_ICON = 'https://cdn.brandfetch.io/id_fixture_music/icon.png?c=demo-client-id';
const SEARCH_HITS = [
  { name: 'Example Music', domain: 'music.example', claimed: true, brandId: 'id_fixture_music', icon: MUSIC_ICON },
  { name: 'Example Mail', domain: 'mail.example', claimed: false, brandId: 'id_fixture_mail', icon: 'https://cdn.brandfetch.io/id_fixture_mail/icon.png?c=demo-client-id' },
  { name: null, domain: 'nameless.example', claimed: false, brandId: 'id_fixture_nameless', icon: null },
];
// A 1x1 PNG.
const PNG = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==', 'base64');
const CORS = { 'access-control-allow-origin': '*' };
const shots = process.env.TD_SHOTS ?? '';

interface Brandfetch {
  searches: URL[];
  images: Request[];
  answer: { status: number; body?: unknown };
}

async function mockBrandfetch(page: Page): Promise<Brandfetch> {
  const seen: Brandfetch = { searches: [], images: [], answer: { status: 200, body: SEARCH_HITS } };
  await page.route('https://api.brandfetch.io/**', async (route) => {
    seen.searches.push(new URL(route.request().url()));
    const { status, body } = seen.answer;
    await route.fulfill({ status, headers: CORS, contentType: 'application/json', body: body === undefined ? '' : JSON.stringify(body) });
  });
  await page.route(/^https:\/\/(cdn|asset)\.brandfetch\.io\//, async (route) => {
    seen.images.push(route.request());
    await route.fulfill({ status: 200, contentType: 'image/png', body: PNG });
  });
  return seen;
}

const brandSection = (page: Page) => picker(page).getByTestId('icon-section-brandfetch');
const settings = (page: Page) => page.getByRole('dialog', { name: 'Settings' });
const brandfetchView = (page: Page) => page.getByRole('dialog', { name: 'Brandfetch logos' });

/** Everything the page keeps in localStorage, sessionStorage and IndexedDB, as text. */
async function pageStorage(page: Page): Promise<string> {
  return page.evaluate(async () => {
    const parts = [JSON.stringify({ ...localStorage }), JSON.stringify({ ...sessionStorage })];
    for (const { name } of await indexedDB.databases()) {
      if (!name) continue;
      parts.push(name);
      const db = await new Promise<IDBDatabase>((resolve, reject) => {
        const open = indexedDB.open(name);
        open.onsuccess = () => resolve(open.result);
        open.onerror = () => reject(open.error ?? new Error(name));
      });
      for (const store of Array.from(db.objectStoreNames)) {
        const all = db.transaction(store).objectStore(store).getAll();
        parts.push(store, JSON.stringify(await new Promise((resolve) => (all.onsuccess = () => resolve(all.result)))));
      }
      db.close();
    }
    return parts.join(' ');
  });
}

async function noBrandfetchInStorage(page: Page) {
  const stored = await pageStorage(page);
  for (const word of ['Example Music', 'music.example', 'id_fixture_music', 'brandfetch.io', 'demo-client-id', 'test-client-id', 'test-api-key']) {
    expect(stored, word).not.toContain(word);
  }
}

test('the settings flow saves a client ID and a write-only key', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('list', { name: 'Devices' })).toBeVisible();
  await page.getByRole('button', { name: 'Settings' }).click();
  await expect(settings(page).getByTestId('brandfetch-entry')).toContainText('Off.');
  await settings(page).getByTestId('brandfetch-entry').click();
  const view = brandfetchView(page);
  await expect(view.getByTestId('brandfetch-notes')).toContainText('own Brandfetch account');
  await expect(view.getByRole('link', { name: /Get a client ID/ })).toHaveAttribute('href', 'https://developers.brandfetch.com/register');

  await view.getByTestId('brandfetch-client-id-input').fill('has space');
  await view.getByTestId('brandfetch-client-id-save').click();
  await expect(view.getByTestId('brandfetch-problem')).toContainText("doesn't look like a client ID");

  await view.getByTestId('brandfetch-client-id-input').fill('test-client-id');
  await view.getByTestId('brandfetch-client-id-save').click();
  await expect(view.getByTestId('brandfetch-client-id-state')).toHaveText('test-client-id');

  const keyInput = view.getByTestId('brandfetch-key-input');
  await expect(keyInput).toHaveAttribute('type', 'password');
  await expect(keyInput).toHaveAttribute('autocomplete', 'off');
  await expect(keyInput).toHaveAttribute('spellcheck', 'false');
  await expect(view.getByTestId('brandfetch-key')).toContainText('Stored in Windows Credential Manager; Open TouchDeck never shows it again.');
  await keyInput.fill('test-api-key');
  await keyInput.press('Enter');
  await expect(view.getByTestId('brandfetch-key-state')).toHaveText('Set');
  // The field is gone, and the key with it; nothing on the page holds it.
  await expect(keyInput).toHaveCount(0);
  expect(await page.content()).not.toContain('test-api-key');
  expect(await page.evaluate(() => Array.from(document.querySelectorAll('input')).map((i) => i.value))).not.toContain('test-api-key');
  await expect(view.getByTestId('brandfetch-notice')).toHaveCount(0);
  await view.getByTestId('brandfetch-key-test').click();
  await expect(view.getByTestId('brandfetch-notice')).toHaveText('Brandfetch accepted the key.');
  await expect(view.getByTestId('brandfetch-count')).toHaveText('No Brandfetch logos yet');
  if (shots) await view.screenshot({ path: join(shots, 'settings-brandfetch-saved.png') });

  await view.getByRole('button', { name: 'Back' }).click();
  await expect(settings(page).getByTestId('brandfetch-entry')).toContainText('On.');
  await expect(settings(page).getByTestId('brandfetch-entry')).toBeFocused();
  await noBrandfetchInStorage(page);
});

test('without a client ID the picker has no Brandfetch section and never calls it', async ({ page }) => {
  const brandfetch = await mockBrandfetch(page);
  await openCopyTile(page);
  await openPicker(page);
  await page.getByTestId('icon-search').fill('spotify');
  await expect(option(page, 'hit:simple-icons:spotify')).toBeVisible();
  await page.waitForTimeout(SEARCH_DELAY_MS + 200);
  await expect(brandSection(page)).toHaveCount(0);
  expect(brandfetch.searches).toEqual([]);
});

test('search runs live after the debounce, a pick becomes the Tile’s Asset, and nothing is kept', async ({ page }) => {
  const brandfetch = await mockBrandfetch(page);
  await openCopyTile(page, '/?brandfetch=on');
  await openPicker(page);
  await page.getByTestId('icon-search').pressSequentially('example', { delay: 30 });
  await expect(option(page, 'brand:id_fixture_music')).toBeVisible();
  expect(brandfetch.searches).toHaveLength(1);
  const [asked] = brandfetch.searches;
  expect(asked.pathname).toBe('/v2/search/example');
  expect(asked.searchParams.get('c')).toBe('demo-client-id');
  expect(asked.searchParams.get('limit')).toBe('5');

  await expect(brandSection(page).getByTestId('icon-option')).toHaveCount(3);
  await expect(option(page, 'brand:id_fixture_music')).toHaveAccessibleName('Example Music, Brandfetch');
  await expect(option(page, 'brand:id_fixture_nameless')).toHaveAccessibleName('nameless.example, Brandfetch');
  const thumb = option(page, 'brand:id_fixture_music').locator('img');
  await expect(thumb).toHaveAttribute('src', MUSIC_ICON);
  await expect.poll(() => thumb.evaluate((img: HTMLImageElement) => img.complete && img.naturalWidth > 0)).toBe(true);
  // The hotlinked image says where it's shown, which Brandfetch asks for.
  const referer = brandfetch.images.find((r) => r.url() === MUSIC_ICON)?.headers().referer;
  expect(referer).toBe('http://127.0.0.1:5174/');
  if (shots) await picker(page).screenshot({ path: join(shots, 'picker-brandfetch-hits.png') });

  await option(page, 'brand:id_fixture_music').click();
  await expect(picker(page)).toBeHidden();
  expect((await created(page)).at(-1)).toEqual({ kind: 'brandfetch', brand: 'music.example', logo_type: 'icon', theme: 'auto', padding: 0, background: null, tint: null });
  const stored = await storedTile(page, 'copy');
  expect(stored.tile?.asset).toMatch(/^sha256:[0-9a-f]{64}$/);
  await expect(page.getByTestId('tile-icon').locator('canvas[data-ready="true"]')).toBeVisible();
  // One search for the whole session; recents keep only the Asset.
  expect(brandfetch.searches).toHaveLength(1);
  await noBrandfetchInStorage(page);
  const recent = await page.evaluate(() => localStorage.getItem('touchdeck.recent-icons'));
  expect(JSON.parse(recent ?? '[]')[0]).toEqual({ kind: 'asset', asset: stored.tile?.asset?.slice('sha256:'.length), name: 'Brandfetch logo' });
});

for (const [status, message] of [
  [403, 'Brandfetch refused the client ID. Check it in Settings, Brandfetch logos.'],
  [429, 'Too many Brandfetch searches for now. Wait a few minutes, then retry.'],
] as const) {
  test(`a ${status} from Brandfetch search says what to do`, async ({ page }) => {
    const brandfetch = await mockBrandfetch(page);
    brandfetch.answer = { status, body: { error: 'refused' } };
    await openCopyTile(page, '/?brandfetch=on');
    await openPicker(page);
    await page.getByTestId('icon-search').fill('spotify');
    const error = picker(page).getByTestId('brandfetch-search-error');
    await expect(error).toContainText(message);
    // The other sources still answer.
    await expect(option(page, 'hit:simple-icons:spotify')).toBeVisible();
    if (shots && status === 429) await picker(page).screenshot({ path: join(shots, `picker-brandfetch-${status}.png`) });

    brandfetch.answer = { status: 200, body: SEARCH_HITS };
    await error.getByRole('button', { name: 'Retry Brandfetch' }).click();
    await expect(option(page, 'brand:id_fixture_music')).toBeVisible();
    await expect(error).toBeHidden();
  });
}

test('a pick Brandfetch has no details for explains itself', async ({ page }) => {
  const brandfetch = await mockBrandfetch(page);
  brandfetch.answer = { status: 200, body: [{ name: 'Gone', domain: 'missing.example', brandId: 'id_fixture_gone', icon: null }] };
  await openCopyTile(page, '/?brandfetch=on');
  await openPicker(page);
  await page.getByTestId('icon-search').fill('gone');
  await option(page, 'brand:id_fixture_gone').click();
  await expect(picker(page).getByTestId('icon-error')).toContainText('Brandfetch has no details for Gone. Choose another result.');
});

test('on Linux the key goes to the system keychain, and a missing one says how to start it', async ({ page }) => {
  await page.goto('/?platform=linux&keychain=off');
  await expect(page.getByRole('list', { name: 'Devices' })).toBeVisible();
  await page.getByRole('button', { name: 'Settings' }).click();
  await settings(page).getByTestId('brandfetch-entry').click();
  const view = brandfetchView(page);
  await expect(view.getByTestId('brandfetch-key')).toContainText('Stored in your system keychain (GNOME Keyring or KWallet); Open TouchDeck never shows it again.');
  await view.getByTestId('brandfetch-key-input').fill('test-api-key');
  await view.getByTestId('brandfetch-key-save').click();
  await expect(view.getByTestId('brandfetch-problem')).toHaveText('No system keychain is available. Start GNOME Keyring or KWallet and try again.');
  await expect(view.getByTestId('brandfetch-key-state')).toHaveCount(0);
  await expect(view.getByTestId('brandfetch-key-input')).toHaveValue('');
  if (shots) await view.screenshot({ path: join(shots, 'settings-brandfetch-linux-no-keychain.png') });
  await noBrandfetchInStorage(page);
});

test('Integration secrets name the same keychain', async ({ page }) => {
  await page.goto('/?platform=linux');
  await expect(page.getByRole('list', { name: 'Devices' })).toBeVisible();
  await page.getByRole('button', { name: 'Integrations', exact: true }).click();
  await page.locator('[data-testid="integration"][data-id="cliproxyapi"]').getByTestId('integration-open').click();
  await expect(page.getByTestId('integration-detail')).toContainText('Stored in your system keychain (GNOME Keyring or KWallet), never in a file.');
});
