import type { LaunchOptions, Page } from '@playwright/test';

// On Windows 11 22H2 and later, Windows Server 2025 (CI's windows-latest) included, Chromium sets
// SO_RANDOMIZE_PORT on every TCP connect (its TcpPortRandomizationWin feature, on by default). About one
// connect in a few thousand then fails at once with WSAENOBUFS, which the page sees as
// net::ERR_NO_BUFFER_SPACE. The demo Home loads about 140 modules from Vite, and a module that fails to
// load stops the whole graph, so the page stays blank and a spec times out on its first locator.
// With the feature off, Windows picks the local port the ordinary way.
//
// Chromium keeps only the last --disable-features switch, and Playwright puts its own list before
// ours. So we drop Playwright's switch and pass one with its list plus ours. `checkLaunchOptions`
// fails the run if a Playwright upgrade changes that list.

/** Playwright 1.64.1's own list (`chromiumSwitches.ts`), in its order. */
const PLAYWRIGHT_DISABLED_FEATURES = [
  'AvoidUnnecessaryBeforeUnloadCheckSync',
  'DestroyProfileOnBrowserClose',
  'DialMediaRouteProvider',
  'GlobalMediaControls',
  'HttpsUpgrades',
  'LensOverlay',
  'MediaRouter',
  'PaintHolding',
  'ThirdPartyStoragePartitioning',
  'BlockOriginHeaderModificationOnRedirect',
  'AvoidCorsURLLoaderRestartOnRedirect',
  'Translate',
  'AutoDeElevate',
  'OptimizationHints',
  'NetworkTimeServiceQuerying',
  'AimEnabled',
  'msForceBrowserSignIn',
  'msEdgeUpdateLaunchServicesPreferredVersion',
];

const DISABLED_FEATURES = [...PLAYWRIGHT_DISABLED_FEATURES, 'TcpPortRandomizationWin'];

/** Edge and Chromium launch options for the e2e specs and their global setup. */
export const launchOptions: LaunchOptions =
  process.platform === 'win32'
    ? {
        ignoreDefaultArgs: [`--disable-features=${PLAYWRIGHT_DISABLED_FEATURES.join(',')}`],
        args: [`--disable-features=${DISABLED_FEATURES.join(',')}`],
      }
    : {};

/** Throws unless the browser behind `page` runs with exactly our --disable-features list. */
export async function checkLaunchOptions(page: Page, channel: string | undefined) {
  if (process.platform !== 'win32') return;
  await page.goto(`${channel === 'msedge' ? 'edge' : 'chrome'}://version`);
  const commandLine = await page.locator('#command_line').innerText();
  const switches = commandLine.match(/--disable-features=\S+/g) ?? [];
  const expected = `--disable-features=${DISABLED_FEATURES.join(',')}`;
  if (switches.length === 1 && switches[0] === expected) return;
  throw new Error(
    `The browser got ${switches.join(' ') || 'no --disable-features'}; e2e/launch-options.ts expects only ${expected}. ` +
      "If Playwright's own list changed, copy it into PLAYWRIGHT_DISABLED_FEATURES.",
  );
}
