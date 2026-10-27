import { connectFirstDeck, openSettings } from '../deck.ts';
import { note, shot } from '../report.ts';

describe('console pane', () => {
  it('shows one @td reply with ok true for version', async () => {
    note('port', await connectFirstDeck());
    await openSettings('help');
    const replies = () => $$('[data-testid="console-line"][data-kind="reply"]');
    const before = await replies().length;
    await $('[data-testid="console-input"]').setValue('version');
    await $('[data-testid="console-send"]').click();
    await browser.waitUntil(async () => (await replies().length) > before, { timeout: 5_000, timeoutMsg: 'no @td reply within 5 s' });
    await browser.pause(500);
    const lines = await replies().map(async (line) => ({ ok: await line.getAttribute('data-ok'), text: await line.getText() }));
    const fresh = lines.slice(before);
    note('replies', fresh);
    expect(fresh).toHaveLength(1);
    expect(fresh[0].ok).toBe('true');
    expect(JSON.parse(fresh[0].text).cmd).toBe('version');
    await $('[data-testid="console"]').scrollIntoView();
    await shot('console');
  });
});
