import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { addTile, emitKeys, openStarter, press, script, send, setCommand } from './inspector-helpers.ts';

const folder = process.env.TD_SHOTS;

for (const theme of ['light', 'dark'] as const) {
  test.describe(theme, () => {
    test.skip(!folder, 'set TD_SHOTS to a folder to take screenshots');
    test.use({ colorScheme: theme });

    test('a command Tile selected', async ({ page }) => {
      await openStarter(page);
      const id = await addTile(page, 'command');
      await page.getByTestId('tile-label').fill('Backup');
      await setCommand(page, 'robocopy', ['C:\\Projects', 'D:\\Backup', '/MIR']);
      await send(page);
      await script(page, ['always']);
      await press(page, 'shortcuts', id, 'command');
      await expect(page.getByTestId('allowed-action')).toHaveCount(1);
      await setCommand(page, 'robocopy', ['C:\\Projects', 'E:\\Backup', '/MIR']);
      await send(page);
      await press(page, 'shortcuts', id, 'command');
      await expect(page.getByTestId('approval-pending')).toBeVisible();
      await page.mouse.move(0, 0);
      await page.screenshot({ path: join(folder!, `inspector-command-${theme}.png`), animations: 'disabled' });
    });

    test('a key Tile selected', async ({ page }) => {
      await openStarter(page);
      await page.locator('[data-testid="tile"][data-id="copy"]').click();
      await page.getByTestId('key-test-panel').locator('summary').click();
      await emitKeys(page, [{ type: 'kbd', mods: 1, keys: [6] }, { type: 'kbd', mods: 0, keys: [] }]);
      await expect(page.getByTestId('key-report')).toHaveCount(2);
      await page.getByTestId('inspector').evaluate((el) => el.scrollTo(0, el.scrollHeight));
      await page.mouse.move(0, 0);
      await page.screenshot({ path: join(folder!, `inspector-key-${theme}.png`), animations: 'disabled' });
    });
  });
}
