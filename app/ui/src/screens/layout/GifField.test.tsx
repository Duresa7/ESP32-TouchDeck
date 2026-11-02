import { describe, expect, test } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { GifStatus } from './GifField.tsx';

const noop = () => {};
const render = (props: Partial<Parameters<typeof GifStatus>[0]>) => renderToStaticMarkup(<GifStatus work={undefined} fitted={null} onCancel={noop} onRetry={noop} {...props} />);

describe('GIF status', () => {
  test('preparing says so and offers Cancel', () => {
    const html = render({ work: { state: 'preparing', file: 'cat.gif' } });
    expect(html).toContain('data-state="preparing"');
    expect(html).toContain('Preparing GIF…');
    expect(html).toContain('data-testid="gif-cancel"');
  });

  test('queued waits for the GIF ahead of it and can be cancelled', () => {
    const html = render({ work: { state: 'queued', file: 'cat.gif' } });
    expect(html).toContain('data-state="queued"');
    expect(html).toContain('Waiting for another GIF…');
    expect(html).toContain('data-testid="gif-cancel"');
  });

  test('ready shows how the GIF was fitted, with nothing to cancel', () => {
    const html = render({ fitted: 'Fitted to Desk: 128 × 92, 10 fps.' });
    expect(html).toContain('data-state="ready"');
    expect(html).toContain('Fitted to Desk: 128 × 92, 10 fps.');
    expect(html).not.toContain('gif-cancel');
  });

  test('failed says why, offers Retry and keeps the raw reason under Details', () => {
    const html = render({ work: { state: 'failed', file: 'cat.gif', text: 'This GIF is too long for Desk. Choose a shorter GIF.', detail: 'asset.too_large: raw', retry: true } });
    expect(html).toContain('data-state="failed"');
    expect(html).toContain('This GIF is too long for Desk. Choose a shorter GIF.');
    expect(html).toContain('data-testid="gif-retry"');
    expect(html).toContain('<summary>Details</summary>');
    expect(html).toContain('asset.too_large: raw');
  });

  test('a failure Retry can not fix asks for the file instead', () => {
    const html = render({ work: { state: 'failed', file: '', text: 'Choose this GIF again to refit it.', detail: '', retry: false } });
    expect(html).toContain('Choose this GIF again to refit it.');
    expect(html).not.toContain('gif-retry');
    expect(html).not.toContain('Details');
  });

  test('nothing to say shows nothing', () => {
    expect(render({})).toContain('data-state="idle"');
  });
});
