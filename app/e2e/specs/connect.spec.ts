import { connectFirstDeck } from '../deck.ts';
import { note, shot } from '../report.ts';

describe('connect', () => {
  it('connects to the first deck and shows its elf_sha256', async () => {
    const port = await connectFirstDeck();
    const elf = await $('[data-testid="hello-elf_sha256"] dd').getText();
    note('port', port);
    note('elf_sha256', elf);
    note('title', await browser.getTitle());
    expect(elf).toMatch(/^[0-9a-f]{64}$/);
    await shot('connect');
  });
});
