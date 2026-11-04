import { chromium, type FullConfig } from '@playwright/test';

export default async function globalSetup(config: FullConfig) {
  const baseURL = config.projects[0]?.use.baseURL ?? 'http://127.0.0.1:5174/';
  const channel = process.platform === 'win32' ? 'msedge' : undefined;
  const browser = await chromium.launch({ channel });
  try {
    const page = await browser.newPage({ baseURL });
    await page.goto('/');
    await page.getByRole('list', { name: 'Devices' }).waitFor({ state: 'visible' });
    await page.getByRole('button', { name: 'Layout', exact: true }).click();
    await page.getByTestId('editor').waitFor({ state: 'visible' });
    await page.getByRole('button', { name: 'Integrations', exact: true }).click();
    await page.getByRole('heading', { level: 1, name: 'Integrations' }).waitFor({ state: 'visible' });
    const isDemo = await page.evaluate(() => {
      if (!window.touchdeckDemo) return false;
      window.touchdeckDemo.fresh('mock0099');
      return true;
    });
    if (isDemo) {
      await page.getByTestId('onboarding').waitFor({ state: 'visible' });
      await page.goto('/?toggle=0&data=worst&update=available');
      await page.getByRole('button', { name: 'Update available' }).waitFor({ state: 'visible' });
    }
    await page.goto('/pair');
    await page.getByRole('heading', { level: 1, name: 'Pair this browser' }).waitFor({ state: 'visible' });
  } finally {
    await browser.close();
  }
}
