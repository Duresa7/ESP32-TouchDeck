import { expect, test } from '@playwright/test';

test('This PC shows all Preset tiles and can be switched off and on', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Integrations', exact: true }).click();
  const row = page.locator('[data-testid="integration"][data-id="this_pc"]');
  await expect(row).toContainText('This PC');
  await expect(row.getByTestId('preset')).toHaveCount(3);
  await expect(row.getByTestId('integration-more')).toHaveText('+7');
  await row.getByTestId('integration-open').click();
  await expect(page.getByTestId('integration-detail')).toHaveAttribute('data-id', 'this_pc');
  await expect(page.getByTestId('preset')).toHaveCount(10);
  await expect(page.getByTestId('secret-field')).toHaveCount(0);
  await page.getByTestId('config-switch').click();
  await page.getByTestId('config-save').click();
  await expect(page.getByTestId('detail-status')).toHaveText('Off');
  await page.getByTestId('config-switch').click();
  await page.getByTestId('config-save').click();
  await expect(page.getByTestId('detail-status')).toHaveText('Connected');
});
