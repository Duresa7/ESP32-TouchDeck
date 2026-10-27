// Helpers for the Buttons screen: its status line, its Layout menu, and file import and export.

export const status = () => $('[data-testid="editor-status"]');
/** The status line's message alone, without its Details toggle. */
export const statusText = async () => ((await $('[data-testid="status-message"]').isExisting()) ? (await $('[data-testid="status-message"]').getText()).trim() : '');
/** The status line's technical detail, such as the rule a change broke or the deck's reply. */
export const statusDetail = async () => String((await status().getAttribute('data-detail')) ?? '');

/** Waits for the status line to report `event`, optionally with a detail matching `detail`, and returns the detail. */
export async function waitEvent(event: string, detail?: RegExp, timeout = 5_000): Promise<string> {
  await browser.waitUntil(
    async () => (await status().getAttribute('data-event')) === event && (!detail || detail.test(await statusDetail())),
    {
      timeout,
      interval: 50,
      timeoutMsg: `the status never reported ${event}${detail ? ` ${detail}` : ''}: ${await status().getAttribute('data-event')} "${await statusDetail()}" "${await statusText()}"`,
    },
  );
  return statusDetail();
}

/** Picks an item from the Layout menu: pull, import, export, menu-starter, or menu-new. */
export async function menu(item: 'pull' | 'import' | 'export' | 'menu-starter' | 'menu-new') {
  if (!(await $(`[role="menu"] [data-testid="${item}"]`).isExisting())) await $('[data-testid="layout-menu"]').click();
  await $(`[role="menu"] [data-testid="${item}"]`).click();
}

export async function importText(text: string, name: string) {
  await browser.execute(
    (content, file) => {
      const input = document.querySelector<HTMLInputElement>('[data-testid="import-file"]')!;
      const data = new DataTransfer();
      data.items.add(new File([content], file, { type: 'application/json' }));
      input.files = data.files;
      input.dispatchEvent(new Event('change', { bubbles: true }));
    },
    text,
    name,
  );
}

/** The editor's current layout as Save to a file writes it, with the native save dialog stubbed. */
export async function exportText(): Promise<string> {
  await browser.execute(() => {
    const host = window as unknown as { tdExported?: string; showSaveFilePicker?: unknown };
    host.tdExported = undefined;
    host.showSaveFilePicker = async () => ({
      createWritable: async () => ({
        write: async (data: string) => {
          host.tdExported = data;
        },
        close: async () => {},
      }),
    });
  });
  await menu('export');
  await browser.waitUntil(async () => (await browser.execute(() => (window as unknown as { tdExported?: string }).tdExported)) !== undefined, {
    timeout: 5_000,
    timeoutMsg: 'Save to a file wrote nothing',
  });
  return browser.execute(() => (window as unknown as { tdExported: string }).tdExported);
}
