import { mkdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

export const out = resolve(process.env.TD_E2E_OUT ?? join(import.meta.dirname, '..', 'target', 'e2e'));

interface TestResult {
  title: string;
  passed: boolean;
  duration_ms: number;
  error?: string;
}

interface Report {
  tests: TestResult[];
  notes: Record<string, unknown>;
}

// WDIO loads the config and the specs as separate module instances, so the report lives on the global.
const report: Report = ((globalThis as { tdReport?: Report }).tdReport ??= { tests: [], notes: {} });

export function note(key: string, value: unknown) {
  report.notes[key] = value;
}

export function recordTest(result: TestResult) {
  report.tests.push(result);
}

export function writeReport(spec: string) {
  mkdirSync(out, { recursive: true });
  const { tests, notes } = report;
  const summary = { spec, ok: tests.length > 0 && tests.every((test) => test.passed), platform: process.platform, tests, ...notes };
  writeFileSync(join(out, `${spec}.json`), JSON.stringify(summary, null, 2) + '\n');
}

export async function shot(name: string) {
  mkdirSync(out, { recursive: true });
  const path = join(out, `${name}.png`);
  await browser.saveScreenshot(path);
  return path;
}
