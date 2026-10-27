import { closeConnection, connectFirstDeck, openConnection, openSettings } from '../deck.ts';
import { note, shot } from '../report.ts';

const SIZES = [
  { name: 'small', width: 1024, height: 640 },
  { name: 'large', width: 1600, height: 1000 },
];

const measure = () =>
  browser.execute(() => ({
    inner: [window.innerWidth, window.innerHeight],
    scrollWidth: document.documentElement.scrollWidth,
    clientWidth: document.documentElement.clientWidth,
    bodyScrollWidth: document.body.scrollWidth,
  }));

/** No horizontal scroll bar on the current screen. */
async function fits(where: string) {
  const layout = await measure();
  if (layout.scrollWidth > layout.clientWidth || layout.bodyScrollWidth > layout.clientWidth) {
    throw new Error(`${where} scrolls sideways: ${JSON.stringify(layout)}`);
  }
  return layout;
}

async function visible(id: string) {
  const shown = await $(`[data-testid="${id}"]`).isDisplayed({ withinViewport: true });
  if (!shown) throw new Error(`${id} is not in view`);
}

describe('window sizes', () => {
  before(async () => {
    await connectFirstDeck();
    await $('[data-testid="empty-starter"]').click();
  });

  for (const size of SIZES) {
    it(`shows the deck indicator, the deck list, the preview, and the console at ${size.width} by ${size.height} with no horizontal scroll bar`, async () => {
      await browser.setWindowSize(size.width, size.height);
      await $('[data-testid="nav-buttons"]').click();
      await browser.pause(300);
      for (const id of ['link-status', 'connection', 'deck-preview', 'push']) await visible(id);
      const layout = await fits('Buttons');
      await openConnection();
      await visible('deck-list');
      await shot(size.name);
      await closeConnection();
      await $('[data-testid="nav-usage"]').click();
      await fits('AI usage');
      await openSettings('help');
      await $('[data-testid="console-input"]').scrollIntoView();
      for (const id of ['console', 'console-input']) await visible(id);
      await fits('Troubleshooting');
      note(size.name, { requested: [size.width, size.height], window: await browser.getWindowSize(), ...layout });
      await shot(`${size.name}-troubleshooting`);
    });
  }
});
