import { mockAction, row, waitForDot } from '../deck.ts';
import { note, shot } from '../report.ts';

describe('disconnect', () => {
  it('turns the dot grey within 6 s when the mock stops mid-session and offers Connect again', async () => {
    await row('mock').waitForExist({ timeout: 10_000 });
    await row('mock').$('[data-testid="connect"]').click();
    await waitForDot('online', 5_000);
    const stopped = Date.now();
    await mockAction('stop');
    await waitForDot('offline', 6_000);
    note('grey_after_ms', Date.now() - stopped);
    const connect = row('mock').$('[data-testid="connect"]');
    await connect.waitForEnabled({ timeout: 5_000 });
    note('status_text', await $('[data-testid="link-status"]').getText());
    await shot('disconnect');
  });
});
