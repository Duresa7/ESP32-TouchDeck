import { describe, expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { demoLayout, sampleDevice } from '../companion/samples.ts';
import { createUiStore } from '../state/ui.ts';
import { DevicePreview } from './DevicePreview.tsx';
import { deviceWidth, firmwareUpdate, lastSeen, previewOf, statusOf } from './home-model.ts';

describe('Home', () => {
  it.each([
    ['connected', 'Connected'], ['connecting', 'Connecting…'], ['busy', 'In use by another program'],
    ['updating', 'Updating…'], ['restarting', 'Restarting…'], ['offline', 'Not connected'],
    ['lost', 'Not connected'], ['disconnected', 'Not connected'], ['unresponsive', 'Not connected'], ['future', 'Not connected'],
  ])('describes %s with words and a light', (state, words) => {
    expect(statusOf(state)).toEqual({ words, live: state === 'connected' });
  });
  it.each([0, 1, 2, 3, 4, 5])('sizes %i Devices', (count) => {
    expect(deviceWidth(count)).toBe(count === 1 ? 560 : 312);
  });
  it('shows a Firmware badge only when an update is available', () => {
    const device = sampleDevice('one', 'Desk');
    expect(firmwareUpdate(device)).toBe(false);
    device.firmware!.update_available = '1.5.0';
    expect(firmwareUpdate(device)).toBe(true);
    device.firmware = null;
    expect(firmwareUpdate(device)).toBe(false);
  });
  it('renders the current Page from a validated Layout, including while offline', () => {
    const preview = previewOf(demoLayout());
    expect(preview).not.toBeNull();
    const before = renderToStaticMarkup(<DevicePreview preview={preview} live={false} />);
    expect(before).toContain('Media');
    expect(before).toContain('Play/Pause');
    expect(before).toContain('is-off');
    const after = renderToStaticMarkup(<DevicePreview preview={previewOf(demoLayout('work'))} live />);
    expect(after).toContain('Work');
    expect(after).toContain('Copy');
    expect(after).not.toContain('Play/Pause');
  });
  it('does not invent a Page or a last-seen time', () => {
    expect(previewOf(null)).toBeNull();
    expect(previewOf({ document: {}, current_page: 'unknown' })).toBeNull();
    expect(previewOf(demoLayout('unknown'))).toBeNull();
    expect(lastSeen(sampleDevice('one', 'Desk'))).toBeNull();
    expect(lastSeen(sampleDevice('one', 'Desk', { state: 'lost', reason: '', detail: '', at_ms: 1793137200000 }))).toMatch(/^Last seen/);
  });
  it('starts at Home and carries the chosen Device into Layout', () => {
    const store = createUiStore();
    expect(store.getState().section).toBe('home');
    store.getState().openDevice('two');
    expect(store.getState()).toMatchObject({ section: 'layout', deviceId: 'two' });
    store.getState().openTroubleshooting();
    expect(store.getState()).toMatchObject({ settingsOpen: true, settingsSection: 'troubleshooting' });
  });
});
