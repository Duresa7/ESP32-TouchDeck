import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { note, shot } from '../report.ts';

const examples = join(import.meta.dirname, '..', '..', '..', 'schema', 'examples');
const text = (name: string) => readFileSync(join(examples, name), 'utf8');

async function importText(content: string, name: string) {
  await browser.execute(
    (body, file) => {
      const input = document.querySelector<HTMLInputElement>('[data-testid="import-file"]')!;
      const data = new DataTransfer();
      data.items.add(new File([body], file, { type: 'application/json' }));
      input.files = data.files;
      input.dispatchEvent(new Event('change', { bubbles: true }));
    },
    content,
    name,
  );
  await $('[data-testid="deck-screen"]').waitForExist({ timeout: 5_000 });
  await browser.pause(300);
}

describe('deck preview draws pages as the deck does', () => {
  before(async () => {
    await $('[data-testid="nav-editor"]').click();
    await $('[data-testid="editor"]').waitForExist({ timeout: 5_000 });
  });

  it('draws the built-in pages', async () => {
    await importText(text('builtin.json'), 'builtin.json');
    note('scale', await $('[data-testid="deck-screen"]').getAttribute('data-scale'));
    await shot('builtin-fkeys');
    await $('[data-testid="page-tab"][data-id="media"]').click();
    await browser.pause(200);
    await shot('builtin-media');
  });

  it('draws the dev pages', async () => {
    await importText(text('dev.json'), 'dev.json');
    for (const id of ['shortcuts', 'desktop', 'usage', 'accounts']) {
      await $(`[data-testid="page-tab"][data-id="${id}"]`).click();
      await browser.pause(200);
      await shot(`dev-${id}`);
    }
  });
});
