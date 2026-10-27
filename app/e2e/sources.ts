import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { out } from './report.ts';

export const DEV_JSON = join(import.meta.dirname, '..', '..', 'schema', 'examples', 'dev.json');

export interface Set {
  m: 'widget.set' | 'dashboard.set';
  at_ms: number;
  tile?: string;
  value?: string;
  caption?: string;
  bar?: number;
  stale?: boolean;
  page?: string;
  rows?: { label: string; value: string; bar: number }[];
}

type Tauri = { __TAURI__: { core: { invoke: (cmd: string, args?: object) => Promise<unknown> } } };

export async function invoke<T>(command: string, args: object = {}): Promise<T> {
  return (await browser.execute(
    async (name, payload) => (window as unknown as Tauri).__TAURI__.core.invoke(name, payload),
    command,
    args,
  )) as T;
}

/** Every `widget.set` and `dashboard.set` the mock deck received, oldest first. */
export const mockSets = () => invoke<Set[]>('e2e_mock_sets');

export async function until<T>(read: () => Promise<T | undefined | false>, timeout: number, what: string): Promise<T> {
  const deadline = Date.now() + timeout;
  for (;;) {
    const found = await read();
    if (found !== undefined && found !== false) return found;
    if (Date.now() > deadline) throw new Error(`${what} did not happen within ${timeout} ms`);
    await new Promise((done) => setTimeout(done, 50));
  }
}

export async function openSources() {
  await $('[data-testid="nav-sources"]').click();
  await $('[data-testid="sources-page"]').waitForExist({ timeout: 5_000 });
  await $('[data-testid="layout-path"]').waitForExist({ timeout: 5_000 });
}

export async function type(testId: string, value: string) {
  const input = $(`[data-testid="${testId}"]`);
  await input.waitForExist({ timeout: 5_000 });
  await input.setValue(value);
}

export async function setChecked(testId: string, on: boolean) {
  const box = $(`[data-testid="${testId}"]`);
  if ((await box.isSelected()) !== on) await box.click();
}

/** Clicks Save and waits for the page to say it saved. */
export async function save() {
  await $('[data-testid="sources-save"]').click();
  await browser.waitUntil(async () => (await $('[data-testid="sources-saved"]').getText()) === 'Saved', {
    timeout: 5_000,
    timeoutMsg: 'the sources page did not report Saved',
  });
}

export async function useLayout(path: string) {
  await type('layout-path', path);
  await save();
  await browser.waitUntil(async () => (await $('[data-testid="layout-state"]').getAttribute('data-error')) === '' && Number(await $('[data-testid="layout-state"]').getAttribute('data-widgets')) > 0, {
    timeout: 5_000,
    timeoutMsg: 'the layout did not load',
  });
}

export function saveJson(name: string, value: unknown) {
  mkdirSync(out, { recursive: true });
  const path = join(out, name);
  writeFileSync(path, JSON.stringify(value, null, 2) + '\n');
  return path;
}

export function saveText(name: string, text: string) {
  mkdirSync(out, { recursive: true });
  const path = join(out, name);
  writeFileSync(path, text);
  return path;
}
