import { expect, test, type Page } from '@playwright/test';

const ID = 'mock00ff';
const card = (page: Page) => page.getByTestId('new-board');
const status = (page: Page) => card(page).getByTestId('board-status');
const dialog = (page: Page) => page.getByTestId('onboarding');

const demo = (page: Page) => ({
  plug: () => page.evaluate((id) => window.touchdeckDemo?.newBoards.plug({ id, port: 'COM7' }), ID),
  unplug: () => page.evaluate((id) => window.touchdeckDemo?.newBoards.unplug(id), ID),
  answer: (answer: 'accept' | 'decline') => page.evaluate(({ id, answer }) => window.touchdeckDemo?.newBoards.answer(id, answer), { id: ID, answer }),
  confirming: () => page.evaluate((id) => window.touchdeckDemo?.newBoards.confirming(id) ?? false, ID),
  fail: (fail: boolean) => page.evaluate(({ id, fail }) => window.touchdeckDemo?.newBoards.fail(id, fail), { id: ID, fail }),
  hold: (pct: number) => page.evaluate(({ id, pct }) => window.touchdeckDemo?.newBoards.hold(id, pct), { id: ID, pct }),
  release: () => page.evaluate((id) => window.touchdeckDemo?.newBoards.release(id), ID),
});

async function found(page: Page) {
  await page.goto('/?devices=1');
  await expect(page.getByRole('list', { name: 'Devices' })).toBeVisible();
  await demo(page).plug();
  await expect(card(page)).toHaveAttribute('data-step', 'detected');
}

test('a New board installs only after the click, with progress, then sets up as a Device', async ({ page }) => {
  await found(page);
  await expect(card(page)).toContainText('New board found');
  await expect(card(page)).toContainText("The board on COM7 doesn't run Open TouchDeck Firmware yet.");
  await page.waitForTimeout(300);
  expect(await demo(page).confirming()).toBe(false);
  await demo(page).hold(60);
  await card(page).getByTestId('board-install').click();
  await expect(card(page)).toHaveAttribute('data-step', 'waiting');
  await expect(status(page)).toHaveText('Confirm on this PC to continue.');
  expect(await demo(page).confirming()).toBe(true);
  await expect(card(page).getByRole('button')).toHaveCount(0);
  await demo(page).answer('accept');
  await expect(status(page)).toHaveText('Installing Firmware… 60%');
  await expect(card(page).getByRole('progressbar')).toHaveAttribute('aria-valuenow', '60');
  await expect(card(page)).toContainText('Keep the board plugged in. This takes about a minute.');
  await demo(page).release();
  await expect(dialog(page)).toHaveAttribute('data-device-id', ID, { timeout: 10_000 });
  await expect(dialog(page)).toHaveAttribute('data-step', 'name');
  await expect(card(page)).toHaveCount(0);
  await page.getByTestId('onboarding-name').fill('Bench');
  await page.getByTestId('onboarding-next').click();
  await expect(dialog(page)).toHaveAttribute('data-step', 'orientation');
  await expect(page.locator('.device-row > .device-figure').nth(1)).toContainText('Bench');
});

test('the installed card says what comes next while the board restarts', async ({ page }) => {
  await found(page);
  await card(page).getByTestId('board-install').click();
  await demo(page).answer('accept');
  await expect(card(page)).toHaveAttribute('data-step', 'done', { timeout: 10_000 });
  await expect(card(page)).toContainText('Firmware installed');
  await expect(status(page)).toHaveText('Next, name your Device.');
});

test('a declined install is explained and leaves the board as it was', async ({ page }) => {
  await found(page);
  await card(page).getByTestId('board-install').click();
  await demo(page).answer('decline');
  await expect(card(page)).toHaveAttribute('data-step', 'declined');
  await expect(status(page)).toHaveText('Declined on this PC. Nothing changed on the board.');
  await card(page).getByTestId('board-cancel').click();
  await expect(card(page)).toHaveAttribute('data-step', 'detected');
  await expect(card(page).getByTestId('board-install')).toBeVisible();
});

test('a failed install shows Details, and Retry asks again', async ({ page }) => {
  await found(page);
  await demo(page).fail(true);
  await card(page).getByTestId('board-install').click();
  await demo(page).answer('accept');
  await expect(card(page)).toHaveAttribute('data-step', 'failed', { timeout: 10_000 });
  await expect(status(page)).toHaveText("Couldn't install Firmware. Unplug the board, plug it back in, then retry.");
  await expect(status(page)).toHaveAttribute('data-code', 'firmware.failed');
  await card(page).getByText('Details').click();
  await expect(card(page).locator('.board-details p')).toContainText('Failed: The update failed.');
  await demo(page).fail(false);
  await card(page).getByTestId('board-retry').click();
  await expect(card(page)).toHaveAttribute('data-step', 'waiting');
  await demo(page).answer('accept');
  await expect(dialog(page)).toHaveAttribute('data-device-id', ID, { timeout: 10_000 });
});

test('Ignore folds the board into a chip that brings it back', async ({ page }) => {
  await found(page);
  await card(page).getByTestId('board-ignore').click();
  await expect(card(page)).toHaveCount(0);
  const chip = page.getByTestId('board-ignored');
  await expect(chip).toHaveText('New board');
  await chip.hover();
  await expect(page.locator('.tooltip')).toHaveText('Ignored. Click to install Firmware.');
  await chip.click();
  await expect(chip).toHaveCount(0);
  await expect(card(page)).toHaveAttribute('data-step', 'detected');
});

test('an unplugged board loses its card', async ({ page }) => {
  await found(page);
  await demo(page).unplug();
  await expect(card(page)).toHaveCount(0);
});

test('focus is visible on the card buttons', async ({ page }) => {
  await found(page);
  const install = card(page).getByTestId('board-install');
  await install.focus();
  await page.keyboard.press('Tab');
  await page.keyboard.press('Shift+Tab');
  await expect(install).toBeFocused();
  expect(await install.evaluate((el) => getComputedStyle(el).outlineStyle)).toBe('solid');
});
