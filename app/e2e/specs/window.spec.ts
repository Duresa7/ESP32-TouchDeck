import { connectFirstDeck } from '../deck.ts';
import { note, shot } from '../report.ts';

const SIZES = [
  { name: 'small', width: 1024, height: 640 },
  { name: 'large', width: 1600, height: 1000 },
];

describe('window sizes', () => {
  before(async () => {
    await connectFirstDeck();
  });

  for (const size of SIZES) {
    it(`shows the deck list, status, and console at ${size.width} by ${size.height} with no horizontal scroll bar`, async () => {
      await browser.setWindowSize(size.width, size.height);
      await browser.pause(300);
      for (const id of ['deck-list', 'link-status', 'console', 'console-input']) {
        expect(await $(`[data-testid="${id}"]`).isDisplayed({ withinViewport: true })).toBe(true);
      }
      const layout = await browser.execute(() => ({
        inner: [window.innerWidth, window.innerHeight],
        scrollWidth: document.documentElement.scrollWidth,
        clientWidth: document.documentElement.clientWidth,
        bodyScrollWidth: document.body.scrollWidth,
      }));
      note(size.name, { requested: [size.width, size.height], window: await browser.getWindowSize(), ...layout });
      expect(layout.scrollWidth).toBeLessThanOrEqual(layout.clientWidth);
      expect(layout.bodyScrollWidth).toBeLessThanOrEqual(layout.clientWidth);
      await shot(size.name);
    });
  }
});
