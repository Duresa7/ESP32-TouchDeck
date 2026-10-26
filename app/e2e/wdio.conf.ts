import { spawn, type ChildProcess } from 'node:child_process';
import { basename, join } from 'node:path';
import { recordTest, writeReport } from './report.ts';
import { specsFor } from './specs.ts';

const attach = process.env.TD_E2E_ATTACH_PORT;
const port = Number(attach ?? process.env.TAURI_WEBDRIVER_PORT ?? 4445);
const binary =
  process.env.TD_APP_BINARY ?? join(import.meta.dirname, '..', 'target', 'release', process.platform === 'win32' ? 'touchdeck.exe' : 'touchdeck');
let app: ChildProcess | undefined;

async function serverReady(timeoutMs: number) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (app?.exitCode != null) throw new Error(`the app exited with code ${app.exitCode} before WebDriver came up`);
    try {
      const status = await fetch(`http://127.0.0.1:${port}/status`);
      if (status.ok && (await status.json()).value?.ready) return;
    } catch {
      await new Promise((done) => setTimeout(done, 100));
    }
  }
  throw new Error(`no WebDriver server on port ${port} within ${timeoutMs} ms`);
}

export const config: WebdriverIO.Config = {
  runner: 'local',
  hostname: '127.0.0.1',
  port,
  specs: specsFor(process.platform, process.env.TD_E2E_SPEC),
  maxInstances: 1,
  capabilities: [{}],
  logLevel: 'warn',
  framework: 'mocha',
  mochaOpts: { timeout: 60_000 },
  reporters: ['spec'],
  waitforTimeout: 10_000,
  connectionRetryCount: 1,

  beforeSession: async (_config, _capabilities, specs) => {
    if (attach) return;
    process.env.TD_E2E_SPAWNED_AT = String(Date.now());
    const env: NodeJS.ProcessEnv = { ...process.env, TAURI_WEBDRIVER_PORT: String(port) };
    if (specs.some((spec) => basename(spec) === 'wireless.spec.ts')) env.TOUCHDECK_INPUT = 'fake';
    app = spawn(binary, [], { env, stdio: 'ignore' });
    await serverReady(30_000);
  },

  afterTest: (test, _context, result) => {
    recordTest({ title: test.title, passed: result.passed, duration_ms: result.duration, error: result.error?.message });
  },

  after: (_result, _capabilities, specs) => {
    writeReport(basename(specs[0]).replace(/\.spec\.ts$/, ''));
  },

  afterSession: async () => {
    if (!app || app.exitCode != null) return;
    const exited = new Promise((done) => app?.once('exit', done));
    app.kill();
    await exited;
  },
};
