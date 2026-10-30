import { expect, test, type Page } from '@playwright/test';

const KEY = 'e2e-secret-5c1d70';

type FakeRecord = { settings: { enabled: boolean; url: string; interval_s: number }; keySet: boolean; refreshes: number; configPuts: number };

async function record(page: Page): Promise<FakeRecord> {
  return page.evaluate(async () => (await fetch('/api/v1/test/integrations/record')).json()) as Promise<FakeRecord>;
}

async function openIntegrations(page: Page, query = '') {
  await page.goto(`/${query}`);
  await expect(page.getByRole('list', { name: 'Devices' })).toBeVisible();
  await page.getByRole('button', { name: 'Integrations', exact: true }).click();
  await expect(page.getByTestId('integration-list')).toBeVisible();
}

async function openCliProxy(page: Page, query = '') {
  await openIntegrations(page, query);
  await page.getByTestId('integration-open').click();
  await expect(page.getByTestId('integration-detail')).toHaveAttribute('data-id', 'cliproxyapi');
}

const field = (page: Page, name: string) => page.locator(`[data-testid="config-field"][data-field="${name}"]`);
const outcome = (page: Page) => page.getByTestId('outcome');

async function reach(page: Page, testId: string, nth = 0) {
  const el = page.getByTestId(testId).nth(nth);
  await el.focus();
  await page.keyboard.press('Tab');
  await page.keyboard.press('Shift+Tab');
  await expect(el).toBeFocused();
  return el.evaluate((node) => {
    const style = getComputedStyle(node);
    return { visible: node.matches(':focus-visible'), ring: style.outlineStyle !== 'none' || style.boxShadow.includes('inset') };
  });
}

test('lists CLIProxyAPI with its Preset tiles and a live status light', async ({ page }) => {
  await openIntegrations(page);
  const row = page.getByTestId('integration');
  await expect(row).toHaveCount(1);
  await expect(row).toHaveAttribute('data-state', 'ok');
  await expect(row).toContainText('CLIProxyAPI');
  await expect(row.getByTestId('preset')).toHaveCount(3);
  await expect(row.getByTestId('integration-more')).toHaveText('+6');
  await expect(page.getByTestId('integration-status')).toHaveText('Connected');
  await expect(page.getByTestId('integration-status')).toHaveAttribute('data-live', 'true');
  await expect(page.getByTestId('integration-setup')).toHaveCount(0);
});

test('a fresh install offers Set up, which opens the form with focus on its first control', async ({ page }) => {
  await openIntegrations(page, '?integrations=new');
  await expect(page.getByTestId('integration')).toHaveAttribute('data-state', 'not_configured');
  await expect(page.getByTestId('integration-status')).toHaveCount(0);
  await page.getByTestId('integration-setup').click();
  await expect(page.getByTestId('config-switch')).toBeFocused();
  await expect(page.getByTestId('detail-status')).toHaveText('Not set up');
  await expect(page.getByTestId('secret-field')).toHaveAttribute('data-set', 'false');
  await expect(page.getByTestId('secret-input')).toHaveAttribute('placeholder', 'Not set');
  await expect(page.getByTestId('refresh')).toHaveAttribute('aria-disabled', 'true');
});

test('setting up with a key shows it only as Set, and never keeps it in the page', async ({ page }) => {
  await openCliProxy(page, '?integrations=new');
  await page.getByTestId('config-switch').click();
  await field(page, 'url').fill('https://hub.example.com/');
  await field(page, 'interval_s').fill('120');
  await expect(page.getByTestId('config-dirty')).toHaveText('Unsaved changes');
  await page.getByTestId('config-save').click();
  await expect(outcome(page)).toHaveAttribute('data-event', 'saved');
  await expect(outcome(page)).toHaveText('Saved');
  await expect(field(page, 'url')).toHaveValue('https://hub.example.com');
  await expect(page.getByTestId('config-save')).toHaveAttribute('aria-disabled', 'true');

  const input = page.getByTestId('secret-input');
  await expect(input).toHaveAttribute('type', 'password');
  await input.fill(KEY);
  await page.getByTestId('secret-save').click();
  await expect(page.getByTestId('secret-state')).toHaveText('Set');
  await expect(page.getByTestId('secret-field')).toHaveAttribute('data-set', 'true');
  await expect(input).toHaveCount(0);
  await expect(page.getByTestId('detail-status')).toHaveAttribute('data-live', 'true');

  expect(await page.content()).not.toContain(KEY);
  const kept = await page.evaluate(() => JSON.stringify({ local: { ...localStorage }, session: { ...sessionStorage }, url: location.href }));
  expect(kept).not.toContain(KEY);
  expect(await record(page)).toMatchObject({ keySet: true, settings: { enabled: true, url: 'https://hub.example.com', interval_s: 120 } });
  expect(JSON.stringify(await record(page))).not.toContain(KEY);

  await page.getByTestId('secret-replace').click();
  await expect(page.getByTestId('secret-input')).toHaveValue('');
  await page.getByTestId('secret-cancel').click();
  await expect(page.getByTestId('secret-state')).toHaveText('Set');

  await page.getByTestId('detail-back').click();
  await expect(page.getByTestId('integration-open')).toBeFocused();
  await expect(page.getByTestId('integration-status')).toHaveText('Connected');
});

test('checks fields inline and warns before a key travels unencrypted', async ({ page }) => {
  await openCliProxy(page);
  const before = (await record(page)).configPuts;
  await field(page, 'interval_s').fill('soon');
  await page.getByTestId('config-save').click();
  await expect(page.getByTestId('field-error')).toHaveText('Enter a whole number.');
  await expect(field(page, 'interval_s')).toHaveAttribute('aria-invalid', 'true');
  expect((await record(page)).configPuts).toBe(before);
  await field(page, 'interval_s').fill('300');
  await expect(page.getByTestId('field-error')).toHaveCount(0);

  await field(page, 'url').fill('http://192.168.1.20:8317');
  await expect(page.getByTestId('transport-warning')).toHaveText("This address isn't on this PC and uses http://, so the key is sent unencrypted. Use https://.");
  await field(page, 'url').fill('http://hub.example.com');
  await expect(page.getByTestId('transport-warning')).toHaveText("This address uses http:// and isn't on your network, so the key isn't sent. Use https://.");
  await field(page, 'url').fill('http://127.0.0.1:8317');
  await expect(page.getByTestId('transport-warning')).toHaveCount(0);
});

test('the status light follows the hub and the switch', async ({ page }) => {
  await openCliProxy(page);
  const status = page.getByTestId('detail-status');
  await expect(status).toHaveText(/^Connected\. 2 accounts, checked 1[2-9] s ago\.$/);
  await expect(status).toHaveAttribute('data-live', 'true');

  await page.evaluate(() => window.touchdeckDemo?.setHub(false));
  await expect(status).toHaveText("Can't connect");
  await expect(status).toHaveAttribute('data-live', 'false');
  await page.getByText('Details', { exact: true }).first().click();
  await expect(page.getByTestId('detail-status-detail')).toHaveText('connection refused');

  await page.evaluate(() => window.touchdeckDemo?.deliver({ device: 'mock0002', sent: 0, error: 'Device is busy' }));
  await expect(page.getByTestId('delivery-error')).toHaveAttribute('data-device', 'mock0002');
  await expect(page.getByTestId('delivery-error')).toContainText("Couldn't send values to");

  await page.evaluate(() => window.touchdeckDemo?.setHub(true));
  await expect(status).toHaveAttribute('data-live', 'true');
  await page.getByTestId('config-switch').click();
  await page.getByTestId('config-save').click();
  await expect(outcome(page)).toHaveAttribute('data-event', 'saved');
  await expect(status).toHaveText('Off');
  await page.getByTestId('detail-back').click();
  await expect(page.getByTestId('integration-status')).toHaveText('Off');
  await expect(page.getByTestId('integration-status')).toHaveAttribute('data-live', 'false');
});

test('Check now asks the hub once, then waits out the floor politely', async ({ page }) => {
  await openCliProxy(page);
  const refresh = page.getByTestId('refresh');
  await expect(refresh).toHaveText('Check now');
  await refresh.click();
  await expect(outcome(page)).toHaveAttribute('data-event', 'refreshed');
  await expect(outcome(page)).toHaveText('Checked');
  await expect(refresh).toHaveAttribute('data-waiting', 'true');
  await expect(refresh).toHaveAttribute('aria-disabled', 'true');
  await refresh.click({ force: true });
  expect((await record(page)).refreshes).toBe(1);
  await expect(page.getByText('Checks are at least 15 s apart, because each one queries your AI services.')).toBeVisible();
});

test('moving the hub to another origin forgets the key and says so', async ({ page }) => {
  await openCliProxy(page);
  await expect(page.getByTestId('secret-field')).toHaveAttribute('data-set', 'true');
  await field(page, 'url').fill('https://hub.example.com/cliproxy');
  await page.getByTestId('config-save').click();
  await expect(outcome(page)).toHaveAttribute('data-event', 'saved');
  await expect(page.getByTestId('secret-field')).toHaveAttribute('data-set', 'true');

  await field(page, 'url').fill('https://other-hub.example.com');
  await page.getByTestId('config-save').click();
  await expect(outcome(page)).toHaveAttribute('data-event', 'secret-forgotten');
  await expect(outcome(page)).toHaveText('Saved. The address changed, so the management key was forgotten. Enter it again.');
  await expect(page.getByTestId('secret-field')).toHaveAttribute('data-set', 'false');
  await expect(page.getByTestId('secret-input')).toHaveAttribute('placeholder', 'Not set');
  await expect(page.getByTestId('detail-status')).toHaveText('Not set up');
  expect((await record(page)).keySet).toBe(false);
});

test('Forget asks first, then removes the key', async ({ page }) => {
  await openCliProxy(page);
  await page.getByTestId('secret-forget').click();
  const dialog = page.getByTestId('forget-dialog');
  await expect(dialog).toContainText('Forget the management key?');
  await expect(dialog).toContainText('CLIProxyAPI stops until you add it again.');
  await page.getByTestId('forget-confirm').click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByTestId('secret-field')).toHaveAttribute('data-set', 'false');
  expect((await record(page)).keySet).toBe(false);
});

test('shows every account with its limits, as the AI usage page did', async ({ browserName, context, page }) => {
  if (browserName === 'chromium') await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await openCliProxy(page);
  const accounts = page.getByTestId('usage-account');
  await expect(accounts).toHaveCount(2);
  const codex = page.locator('[data-testid="usage-account"][data-account="codex:Work"]');
  await expect(codex.getByTestId('usage-plan')).toHaveText('pro');
  await expect(codex.getByTestId('usage-five-hour')).toContainText('42%');
  await expect(codex.getByTestId('usage-five-hour')).toContainText('Resets in 2h 10m');
  await expect(codex.getByTestId('usage-seven-day')).toContainText('10%');
  await expect(codex.getByTestId('usage-status')).toContainText('Active');
  await expect(codex.getByTestId('usage-status')).toContainText('128 OK · 2 failed · 6 in 10 min');
  await expect(page.locator('[data-testid="usage-account"][data-account="claude:Personal"]').getByTestId('usage-seven-day')).toContainText('—');

  await codex.getByTestId('usage-copy').click();
  await expect(codex.getByTestId('usage-copy')).toHaveText('Copied');
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('codex:Work');
});

test('Add to Layout refuses a full Page, then places a Preset tile in the first free cell', async ({ page }) => {
  await openIntegrations(page);
  await page.getByTestId('integration').getByTestId('preset').first().click();
  await expect(page.getByText('This Page is full. Make its grid bigger or add a Page.')).toBeVisible();

  await page.getByRole('button', { name: 'Layout', exact: true }).click();
  await page.getByTestId('layout-menu').click();
  await page.getByTestId('menu-new').click();
  await expect(page.getByTestId('tile')).toHaveCount(0);

  await page.getByRole('button', { name: 'Integrations', exact: true }).click();
  const preset = page.getByTestId('integration').getByTestId('preset').first();
  await expect(preset).toHaveAttribute('aria-label', 'Add Account status to Layout');
  await preset.click();
  await expect(page.getByText("Added to Desk's Layout")).toBeVisible();

  await page.getByRole('button', { name: 'Layout', exact: true }).click();
  const placed = page.getByTestId('tile');
  await expect(placed).toHaveCount(1);
  await expect(placed).toHaveAttribute('aria-label', 'Account status');
  await expect(placed).toHaveAttribute('data-row', '0');
  await expect(placed).toHaveAttribute('data-col', '0');
  await expect(placed).toHaveAttribute('aria-pressed', 'true');
});

test('Add Tile lists Preset tiles by Integration and places one where it points', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('list', { name: 'Devices' })).toBeVisible();
  await page.getByRole('button', { name: 'Layout', exact: true }).click();
  await page.getByTestId('start-starter').click();
  await page.getByTestId('page-tab').filter({ hasText: 'Shortcuts' }).click();
  await page.locator('[data-testid="empty-cell"][data-row="2"][data-col="2"]').click();
  const section = page.getByTestId('add-presets');
  await expect(section).toBeVisible();
  const group = section.locator('[data-id="cliproxyapi"]');
  await expect(group).toContainText('CLIProxyAPI');
  await expect(group.getByTestId('preset')).toHaveCount(9);
  await group.locator('[data-preset="five_hour_pct"]').click();
  const placed = page.locator('[data-testid="tile"][data-row="2"][data-col="2"]');
  await expect(placed).toHaveAttribute('aria-label', '5-hour limit use');
  await expect(placed).toHaveAttribute('aria-pressed', 'true');
  await expect(section).toHaveCount(0);
});

test('Add Tile points to Integrations when none offers Preset tiles', async ({ page }) => {
  await page.goto('/?integrations=new');
  await expect(page.getByRole('list', { name: 'Devices' })).toBeVisible();
  await page.getByRole('button', { name: 'Layout', exact: true }).click();
  await page.getByTestId('start-starter').click();
  await page.getByTestId('page-tab').filter({ hasText: 'Shortcuts' }).click();
  await page.locator('[data-testid="empty-cell"][data-row="2"][data-col="2"]').click();
  const section = page.getByTestId('add-presets');
  await expect(section).toContainText('Set up an Integration to add its Preset tiles.');
  await section.getByTestId('add-presets-setup').click();
  await expect(page.getByTestId('integration-setup')).toBeVisible();
});

test('focus is visible on the list and on every control of the CLIProxyAPI view', async ({ page }) => {
  await openIntegrations(page);
  for (const id of ['integration-open', 'preset', 'integration-more']) {
    expect({ id, ...(await reach(page, id)) }).toEqual({ id, visible: true, ring: true });
  }
  await page.getByTestId('integration-open').click();
  await expect(page.getByTestId('usage-account')).toHaveCount(2);
  for (const id of ['detail-back', 'config-switch', 'config-field', 'secret-replace', 'secret-forget', 'refresh', 'preset', 'usage-copy']) {
    expect({ id, ...(await reach(page, id)) }).toEqual({ id, visible: true, ring: true });
  }
});
