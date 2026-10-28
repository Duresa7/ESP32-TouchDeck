import { mockAction, openConnection, row, waitForDot } from '../deck.ts';
import { note, shot } from '../report.ts';

describe('disconnect', () => {
  it('turns the dot grey on Disconnect without reporting a lost connection', async () => {
    await openConnection();
    await row('mock').waitForExist({ timeout: 10_000 });
    await row('mock').$('[data-testid="connect"]').click();
    await waitForDot('online', 5_000);
    await row('mock').$('[data-testid="disconnect"]').click();
    await waitForDot('offline', 5_000);
    // The session's close event arrives after the click; it must not turn a chosen Disconnect into a lost link.
    await browser.pause(500);
    expect(await $('[data-testid="link-status"]').getText()).toBe('Deck not connected');
    note('left_status_text', await $('[data-testid="link-status"]').getText());
  });

  it('turns the dot grey within 6 s when the mock stops mid-session and offers Connect again', async () => {
    await openConnection();
    await row('mock').waitForExist({ timeout: 10_000 });
    await row('mock').$('[data-testid="connect"]').click();
    await waitForDot('online', 5_000);
    const stopped = Date.now();
    await mockAction('stop');
    await waitForDot('offline', 6_000);
    note('grey_after_ms', Date.now() - stopped);
    await openConnection();
    const connect = row('mock').$('[data-testid="connect"]');
    await connect.waitForEnabled({ timeout: 5_000 });
    expect(await $('[data-testid="link-status"]').getText()).toBe('Connection lost');
    note('status_text', await $('[data-testid="link-status"]').getText());
    await shot('disconnect');
  });
});
