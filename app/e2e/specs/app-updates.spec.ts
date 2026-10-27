import { connectFirstDeck, waitForDot } from '../deck.ts';
import { note, shot } from '../report.ts';

type Scenario = 'current' | 'available' | 'network' | 'signature' | 'metadata' | 'unconfigured' | 'unsupported' | 'busy' | 'installer' | 'internal';
interface Record { fixture: string; checks: number; downloads: number; installs: number; awaitingDownload: boolean }

async function invoke<T>(command: string, args: object = {}): Promise<T> {
  return browser.execute(async (command, args) => {
    const tauri = (window as unknown as { __TAURI__: { core: { invoke: (command: string, args: object) => Promise<unknown> } } }).__TAURI__;
    return tauri.core.invoke(command, args);
  }, command, args) as Promise<T>;
}

const script = (args: { scenario?: Scenario; finishDownload?: boolean } = {}) => invoke<Record>('e2e_app_update', args);
const section = () => $('[data-testid="app-updates"]');
const check = () => $('[data-testid="app-update-check"]');
const install = () => $('[data-testid="app-update-install"]');
const alert = () => section().$('[role="alert"]');

async function shows(text: string) {
  await browser.waitUntil(async () => (await section().getText()).includes(text), { timeout: 5_000, timeoutMsg: `App updates did not show: ${text}` });
}

async function capture(name: string) {
  await section().scrollIntoView();
  await shot(name);
}

async function checkScenario(scenario: Scenario) {
  await script({ scenario });
  await check().click();
}

async function startDownload() {
  await $('[data-testid="app-update-download"]').click();
  await shows('Downloading update: 16 KB of 64 KB');
  expect(await check().isEnabled()).toBe(false);
  expect(await install().isExisting()).toBe(false);
  expect(await script()).toMatchObject({ checks: 1, downloads: 1, installs: 0, awaitingDownload: true });
}

async function finishDownload() {
  await script({ finishDownload: true });
  await shows('Download verified.');
  expect(await install().getText()).toBe('Install and close TouchDeck');
}

describe('Settings > App updates with scripted updater I/O', () => {
  it('shows the up-to-date state when the check finds no update', async () => {
    await $('[data-testid="nav-settings"]').click();
    await checkScenario('current');
    await shows('You’re on the latest app version.');
    expect(await check().isEnabled()).toBe(true);
    expect(await $('[data-testid="app-update-download"]').isExisting()).toBe(false);
    note('current', await script());
    await capture('a-current');
  });

  it('shows the available version and release notes without installing', async () => {
    await checkScenario('available');
    await shows('Version 0.2.0');
    expect(await section().$('.notes').getText()).toBe('Scripted release notes\nImprove USB deck reliability.');
    expect(await $('[data-testid="app-update-download"]').isEnabled()).toBe(true);
    expect(await install().isExisting()).toBe(false);
    note('available', await script());
    await capture('b-available');
  });

  it('renders channel progress and offers install only after download verification', async () => {
    await startDownload();
    expect(await section().$('progress').getAttribute('value')).toBe('16384');
    expect(await section().$('progress').getAttribute('max')).toBe('65536');
    await capture('c-downloading');
    await finishDownload();
    expect(await install().isEnabled()).toBe(true);
    expect((await script()).installs).toBe(0);
    note('downloaded', await script());
    await capture('c-ready');
  });

  it('refuses installation through the real connected-deck guard and keeps the download retryable', async () => {
    // Simulate a deck connecting after the UI last observed offline. This reaches the native
    // guard through the real Install button instead of removing its disabled attribute.
    const deck = await invoke<{ port: string; online: boolean }>('connect', { port: 'mock' });
    expect(deck.online).toBe(true);
    expect(await invoke('deck_info')).toMatchObject({ port: 'mock', online: true });
    await browser.waitUntil(async () => (await invoke('action_layout')) !== null, { timeout: 5_000 });
    await install().click();
    await shows('Disconnect the deck before installing the app update');
    expect(await alert().getText()).toBe('Disconnect the deck before installing the app update');
    expect(await install().isEnabled()).toBe(true);
    expect((await script()).installs).toBe(0);
    note('connected_refusal', { native_deck: deck.port, message: await alert().getText(), ...await script() });
    await capture('d-install-refused');

    // With the UI aware of the connection it prevents the action before IPC as well.
    await $('[data-testid="nav-deck"]').click();
    await connectFirstDeck();
    await $('[data-testid="nav-settings"]').click();
    expect(await install().isEnabled()).toBe(false);
    await shows('Disconnect the deck and wait for transfers to finish before installing.');
    await capture('d-connected');
    await $('[data-testid="nav-deck"]').click();
    await $('[data-testid="disconnect"]').click();
    await waitForDot('offline', 5_000);
    await $('[data-testid="nav-settings"]').click();
    expect(await install().isEnabled()).toBe(true);
  });

  for (const [kind, message] of [
    ['network', 'Cannot reach the app update server'],
    ['signature', 'App update signature verification failed'],
    ['metadata', 'App update metadata is invalid'],
    ['unconfigured', 'Install an official release to enable app updates'],
    ['unsupported', 'Desktop app updates are currently packaged for Windows x64 only'],
    ['busy', 'An app update operation is already running'],
    ['installer', 'Could not start the app installer'],
    ['internal', 'The app updater failed unexpectedly'],
  ] as const) {
    it(`shows the ${kind} error and allows another check`, async () => {
      await checkScenario(kind);
      if (kind === 'signature' || kind === 'installer') {
        await shows('Version 0.2.0');
        await startDownload();
        if (kind === 'installer') {
          await finishDownload();
          await install().click();
        } else {
          await script({ finishDownload: true });
        }
      }
      await shows(message);
      expect(await alert().getText()).toBe(message);
      expect(await check().isEnabled()).toBe(true);
      expect((await section().getText()).includes('Offline or a release may be unavailable.')).toBe(kind === 'network');
      if (kind === 'installer') expect(await install().isEnabled()).toBe(true);
      else expect(await install().isExisting()).toBe(false);
      note(`error_${kind}`, { message: await alert().getText(), ...await script() });
      if (kind === 'network') await capture('e-network-error');
      await checkScenario('current');
      await shows('You’re on the latest app version.');
      expect(await alert().isExisting()).toBe(false);
      expect(await check().isEnabled()).toBe(true);
    });
  }

  it('calls the fake install exactly once while disconnected and leaves the app alive', async () => {
    expect(await invoke('deck_info')).toBeNull();
    await checkScenario('available');
    await shows('Version 0.2.0');
    await startDownload();
    await finishDownload();
    await browser.execute(() => {
      const button = document.querySelector<HTMLButtonElement>('[data-testid="app-update-install"]')!;
      button.click();
      button.click();
    });
    await shows('Starting the installer. TouchDeck will close');
    await browser.waitUntil(async () => (await script()).installs === 1, { timeout: 5_000 });
    expect(await script()).toMatchObject({ checks: 1, downloads: 1, installs: 1 });
    expect(await check().isEnabled()).toBe(false);
    // Another rendered interaction and IPC query prove the fake never exited the app.
    await $('[data-testid="nav-deck"]').click();
    await $('[data-testid="nav-settings"]').click();
    await shows('Starting the installer. TouchDeck will close');
    expect((await script()).installs).toBe(1);
    note('installed', await script());
    await capture('f-install-called-once');
  });
});
