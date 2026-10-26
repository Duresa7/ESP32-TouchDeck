export const MOCK_ELF_SHA256 = '6d6f636b6465636b6d6f636b6465636b6d6f636b6465636b6d6f636b6465636b';
export const HELLO_FIELDS = ['product', 'proto', 'elf_sha256', 'version', 'running', 'sinks', 'recovery', 'layout'];

export const row = (port: string) => $(`[data-testid="deck-row"][data-port="${port}"]`);
export const dot = () => $('[data-testid="status-dot"]');

export async function waitForDot(state: 'online' | 'offline', timeout: number) {
  await browser.waitUntil(async () => (await dot().getAttribute('data-state')) === state, {
    timeout,
    interval: 50,
    timeoutMsg: `status dot did not turn ${state} within ${timeout} ms`,
  });
}

/** Connects to the first port that answered hello and returns it. */
export async function connectFirstDeck() {
  const deck = $('[data-testid="deck-row"][data-status="deck"]');
  await deck.waitForExist({ timeout: 15_000 });
  const port = await deck.getAttribute('data-port');
  await deck.$('[data-testid="connect"]').click();
  await waitForDot('online', 5_000);
  return port;
}

export async function mockAction(action: 'stop' | 'hang-up') {
  await browser.execute(async (name) => {
    const tauri = (window as unknown as { __TAURI__: { core: { invoke: (cmd: string, args: object) => Promise<unknown> } } }).__TAURI__;
    await tauri.core.invoke('e2e_mock', { action: name });
  }, action);
}
