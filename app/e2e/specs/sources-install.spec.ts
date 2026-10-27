import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';
import { note, out, shot } from '../report.ts';
import { openSources, saveJson, type } from '../sources.ts';

// The install lane runs against a settings file in a temporary home. It never touches the real one.
const OLD_COMMAND = 'echo old status';
const BEFORE = `{
  "model": "opus",
  "env": {
    "KEEP": "me"
  },
  "statusLine": {
    "type": "command",
    "command": "${OLD_COMMAND}",
    "padding": 0
  },
  "permissions": {
    "allow": ["Bash(ls:*)"]
  }
}
`;

describe('statusline install', () => {
  const home = join(tmpdir(), `td-install-spec-${process.pid}`);
  const settings = join(home, '.claude', 'settings.json');

  before(async () => {
    expect(settings).not.toBe(join(homedir(), '.claude', 'settings.json'));
    mkdirSync(join(home, '.claude'), { recursive: true });
    writeFileSync(settings, BEFORE);
    await openSources();
  });

  it('shows the diff, then wraps the old command with --then and changes nothing else', async () => {
    await type('settings-path', settings);
    await $('[data-testid="statusline-preview"]').click();
    const result = $('[data-testid="statusline-result"]');
    await result.waitForExist({ timeout: 5_000 });
    expect(await result.getAttribute('data-changed')).toBe('true');
    expect(await result.getAttribute('data-installed')).toBe('false');
    expect(readFileSync(settings, 'utf8')).toBe(BEFORE);
    const diff = await $('[data-testid="statusline-diff"]').getText();
    note('diff', diff);
    expect(diff).toContain(`"command": "${OLD_COMMAND}"`);
    expect(diff).toContain('--then');

    await $('[data-testid="statusline-install"]').click();
    await browser.waitUntil(async () => (await result.getAttribute('data-installed')) === 'true', { timeout: 5_000, timeoutMsg: 'the install did not finish' });
    await shot('install');

    const after = readFileSync(settings, 'utf8');
    copyFileSync(settings, join(out, 'settings-after.json'));
    const before = JSON.parse(BEFORE);
    const parsed = JSON.parse(after);
    const command: string = parsed.statusLine.command;
    note('command', command);
    expect(command).toMatch(new RegExp(`^(?:"[^"]*td-statusline(?:\\.exe)?"|\\S*td-statusline(?:\\.exe)?) --then "${OLD_COMMAND}"$`));
    expect(parsed.statusLine.padding).toBe(0);
    expect(parsed.statusLine.type).toBe('command');
    delete before.statusLine;
    delete parsed.statusLine;
    expect(parsed).toEqual(before);
    const head = BEFORE.slice(0, BEFORE.indexOf('  "statusLine"'));
    const tail = BEFORE.slice(BEFORE.indexOf('  "permissions"'));
    expect(after.startsWith(head)).toBe(true);
    expect(after.endsWith(tail)).toBe(true);
    expect(existsSync(join(home, '.claude', 'settings.json.touchdeck.bak'))).toBe(true);
    expect(readFileSync(join(home, '.claude', 'settings.json.touchdeck.bak'), 'utf8')).toBe(BEFORE);
    saveJson('install-result.json', { command, settings_path: settings });
  });

  it('leaves a settings file that already runs the shim alone', async () => {
    const installed = readFileSync(settings, 'utf8');
    await $('[data-testid="statusline-preview"]').click();
    await browser.waitUntil(async () => (await $('[data-testid="statusline-result"]').getAttribute('data-changed')) === 'false', {
      timeout: 5_000,
      timeoutMsg: 'a second preview still wanted a change',
    });
    expect(readFileSync(settings, 'utf8')).toBe(installed);
    expect(await $('[data-testid="statusline-install"]').isEnabled()).toBe(false);
  });

  it('removes the shim, puts the old command back byte for byte, then has nothing to do', async () => {
    await $('[data-testid="statusline-remove"]').click();
    const removal = $('[data-testid="statusline-removal"]');
    await removal.waitForExist({ timeout: 5_000 });
    expect(await removal.getAttribute('data-changed')).toBe('true');
    expect(await $('[data-testid="statusline-restored"]').getText()).toBe(OLD_COMMAND);
    expect(readFileSync(settings, 'utf8')).toBe(BEFORE);
    copyFileSync(settings, join(out, 'settings-removed.json'));
    await shot('remove');

    await $('[data-testid="statusline-remove"]').click();
    await browser.waitUntil(async () => (await $('[data-testid="statusline-removal"]').getAttribute('data-changed')) === 'false', {
      timeout: 5_000,
      timeoutMsg: 'a second remove still changed the file',
    });
    expect(readFileSync(settings, 'utf8')).toBe(BEFORE);
  });
});
