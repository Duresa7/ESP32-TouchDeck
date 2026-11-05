import { join } from 'node:path';
import { expect, test, type Page } from '@playwright/test';

const FULL = 'This Page is full. Make its grid bigger or add a Page.';
const ADDED = "Added to Desk's Layout";

async function presetOnFullPage(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('list', { name: 'Devices' })).toBeVisible();
  await page.getByRole('button', { name: 'Integrations', exact: true }).click();
  await page.locator('[data-testid="integration"][data-id="cliproxyapi"]').getByTestId('preset').first().click();
  return page.locator('[data-sonner-toast]').filter({ hasText: FULL });
}

async function presetOnEmptyLayout(page: Page) {
  await page.getByRole('button', { name: 'Layout', exact: true }).click();
  await page.getByTestId('layout-menu').click();
  await page.getByTestId('menu-new').click();
  await expect(page.getByTestId('tile')).toHaveCount(0);
  await page.getByRole('button', { name: 'Integrations', exact: true }).click();
  await page.locator('[data-testid="integration"][data-id="cliproxyapi"]').getByTestId('preset').first().click();
  return page.locator('[data-sonner-toast]').filter({ hasText: ADDED });
}

const tokens = (page: Page) =>
  page.evaluate(() => {
    const probe = document.createElement('span');
    document.body.append(probe);
    const read = (name: string, property: 'color' | 'backgroundColor') => {
      probe.style.setProperty(property === 'color' ? 'color' : 'background-color', `var(${name})`);
      return getComputedStyle(probe)[property];
    };
    const out = { surface: read('--surface-raised', 'backgroundColor'), ink: read('--ink', 'color'), danger: read('--danger-ink', 'color'), live: read('--live-ink', 'color') };
    probe.remove();
    return out;
  });

for (const theme of ['light', 'dark'] as const) {
  test(`toasts use the app's surface, type and status colors in ${theme}`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: theme });
    const error = await presetOnFullPage(page);
    await expect(error).toBeVisible();
    const want = await tokens(page);
    const look = (toast: typeof error) =>
      toast.evaluate((node) => {
        const style = getComputedStyle(node);
        const icon = node.querySelector('[data-icon]');
        return {
          background: style.backgroundColor,
          color: style.color,
          radius: style.borderRadius,
          font: style.fontFamily,
          size: style.fontSize,
          styled: node.getAttribute('data-styled'),
          icon: icon ? getComputedStyle(icon).color : null,
        };
      });
    const errorLook = await look(error);
    expect(errorLook).toMatchObject({ background: want.surface, color: want.ink, radius: '12px', size: '14px', styled: 'false', icon: want.danger });
    expect(errorLook.font).toContain('Archivo');

    const success = await presetOnEmptyLayout(page);
    await expect(success).toBeVisible();
    expect(await look(success)).toMatchObject({ background: want.surface, color: want.ink, radius: '12px', icon: want.live });
  });

  test(`proof toasts ${theme}`, async ({ page }) => {
    test.skip(!process.env.TD_SHOTS, 'set TD_SHOTS for visual evidence');
    await page.emulateMedia({ colorScheme: theme });
    const error = await presetOnFullPage(page);
    await expect(error).toBeVisible();
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(600);
    await page.screenshot({ path: join(process.env.TD_SHOTS!, `toast-error-${theme}.png`) });
    const success = await presetOnEmptyLayout(page);
    await expect(success).toBeVisible();
    await page.waitForTimeout(600);
    await page.screenshot({ path: join(process.env.TD_SHOTS!, `toast-success-${theme}.png`) });
  });
}

test('toasts drop their slide when the system asks for reduced motion', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  const error = await presetOnFullPage(page);
  await expect(error).toBeVisible();
  expect(await error.evaluate((node) => getComputedStyle(node).transitionDuration.split(',').every((d) => parseFloat(d) === 0))).toBe(true);
});
