export const MOCK_ELF_SHA256 = '6d6f636b6465636b6d6f636b6465636b6d6f636b6465636b6d6f636b6465636b';
export const HELLO_FIELDS = ['product', 'proto', 'elf_sha256', 'version', 'running', 'sinks', 'recovery', 'layout'];

/** A deck row of the connection panel. The panel must be open, see openConnection. */
export const row = (port: string) => $(`[data-testid="deck-row"][data-port="${port}"]`);
export const dot = () => $('[data-testid="status-dot"]');

export async function waitForDot(state: 'online' | 'offline', timeout: number) {
  await browser.waitUntil(async () => (await dot().getAttribute('data-state')) === state, {
    timeout,
    interval: 50,
    timeoutMsg: `status dot did not turn ${state} within ${timeout} ms`,
  });
}

/** Opens the connection panel under the top bar's deck indicator, where the found decks are listed. */
export async function openConnection() {
  const panel = $('[data-testid="connection-panel"]');
  if (!(await panel.isExisting())) await $('[data-testid="connection"]').click();
  await panel.waitForExist({ timeout: 5_000 });
}

export async function closeConnection() {
  if (await $('[data-testid="connection-panel"]').isExisting()) await $('[data-testid="connection"]').click();
  await $('[data-testid="connection-panel"]').waitForExist({ reverse: true, timeout: 2_000 });
}

/** Connects to the first port that answered hello and returns it, leaving the panel closed. */
export async function connectFirstDeck() {
  await openConnection();
  const deck = $('[data-testid="deck-row"][data-status="deck"]');
  await deck.waitForExist({ timeout: 15_000 });
  const port = await deck.getAttribute('data-port');
  await deck.$('[data-testid="connect"]').click();
  await waitForDot('online', 5_000);
  await closeConnection();
  return port;
}

/** Opens a Settings section: deck, software, app, or help (Troubleshooting). */
export async function openSettings(section: 'deck' | 'software' | 'app' | 'help') {
  await $('[data-testid="nav-settings"]').click();
  await $(`[data-testid="settings-tab-${section}"]`).click();
  await $('[data-testid="settings-content"]').waitForExist({ timeout: 5_000 });
}

export async function mockAction(action: 'stop' | 'hang-up') {
  await browser.execute(async (name) => {
    const tauri = (window as unknown as { __TAURI__: { core: { invoke: (cmd: string, args: object) => Promise<unknown> } } }).__TAURI__;
    await tauri.core.invoke('e2e_mock', { action: name });
  }, action);
}
