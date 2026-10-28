import { expect, test } from '@playwright/test';

// Runs with playwright.companion.config.ts, against `td-companion --mock-devices 1`.
test('Home lists the mock Device from the real Companion', async ({ page }) => {
  const session = page.waitForResponse((r) => r.url().endsWith('/__companion/session'));
  const events = page.waitForEvent('websocket', (ws) => new URL(ws.url()).pathname === '/api/v1/events');
  await page.goto('/');

  expect((await session).status()).toBe(200);
  await events;

  const cards = page.getByRole('list', { name: 'Devices' }).getByRole('listitem');
  await expect(cards).toHaveCount(1);
  await expect(cards.first()).toContainText('Unnamed Device');
  await expect(cards.first()).toContainText('Connected');
});
