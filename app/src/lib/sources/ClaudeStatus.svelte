<script lang="ts">
  import { ago, statuslineInstall, statuslinePreview, type ClaudeInfo, type Config, type InstallPreview, type SourcesView } from './api.ts';

  let { view, draft, info }: { view: SourcesView; draft: Config; info: ClaudeInfo | undefined } = $props();

  let preview = $state<InstallPreview | null>(null);
  let problem = $state<string | null>(null);
  let busy = $state(false);

  const phase = $derived(!info || info.session === null ? 'waiting' : info.stale ? 'stale' : 'live');
  const entry = $derived(JSON.stringify({ statusLine: { type: 'command', command: view.shim.command } }, null, 2));

  async function run(action: typeof statuslinePreview) {
    busy = true;
    problem = null;
    try {
      preview = await action(draft.claude_settings);
    } catch (error) {
      problem = String(error);
    } finally {
      busy = false;
    }
  }
</script>

<section class="card" data-testid="claude-card">
  <header>
    <h3>Claude Code status line</h3>
    <label class="switch">
      <input type="checkbox" bind:checked={draft.claude_status} data-testid="claude-enabled" />
      <span>On</span>
    </label>
  </header>

  <p class="note" data-testid="claude-state" data-state={phase}>
    {#if phase === 'waiting'}
      Waiting for a status line update. Install the shim below, then use Claude Code.
    {:else if phase === 'stale'}
      Stale. The newest session, {info?.session}, last updated {ago(info?.updated ?? null)}.
    {:else}
      Session {info?.session}, updated {ago(info?.updated ?? null)}.
    {/if}
  </p>
  {#if info?.error}<p class="bad" data-testid="claude-error">{info.error}</p>{/if}

  <dl class="facts">
    <dt>Watching</dt>
    <dd><code data-testid="claude-dir">{view.claude_dir}</code></dd>
  </dl>

  {#if info && info.readings.length}
    <table data-testid="claude-readings">
      <thead><tr><th>Field</th><th>Value</th><th>Bar</th></tr></thead>
      <tbody>
        {#each info.readings as reading (reading.field)}
          <tr data-testid="claude-reading-{reading.field}" data-stale={reading.stale}>
            <td>{reading.field}</td>
            <td data-testid="claude-value-{reading.field}">{reading.value}</td>
            <td>{reading.bar < 0 ? '' : `${Math.round(reading.bar * 100)}%`}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}

  <h4>Status line entry</h4>
  <p class="note">
    Put this in <code>~/.claude/settings.json</code>. Install wraps the command already there with <code>--then</code>, so it keeps running.
    {#if !view.shim.found}The <code>td-statusline</code> binary is not next to the app, so the entry names it bare and expects it on PATH.{/if}
  </p>
  <pre data-testid="statusline-entry">{entry}</pre>

  <label class="row">
    <span>Settings file</span>
    <input
      bind:value={() => draft.claude_settings ?? '', (value) => (draft.claude_settings = value || null)}
      placeholder={view.settings_path}
      spellcheck="false"
      data-testid="settings-path"
    />
  </label>
  <div class="actions">
    <button onclick={() => run(statuslinePreview)} disabled={busy} data-testid="statusline-preview">Show diff</button>
    <button class="primary" onclick={() => run(statuslineInstall)} disabled={busy || !preview?.changed || preview.installed} data-testid="statusline-install">
      Install
    </button>
  </div>
  {#if problem}<p class="bad" data-testid="statusline-error">{problem}</p>{/if}
  {#if preview}
    <div data-testid="statusline-result" data-changed={preview.changed} data-installed={preview.installed}>
      <p class="note">
        {#if preview.installed}
          Installed in <code>{preview.path}</code>. The command is <code data-testid="statusline-command">{preview.command}</code>.
        {:else if !preview.changed}
          <code>{preview.path}</code> already runs the shim. Nothing to change.
        {:else}
          {preview.exists ? 'Changes to' : 'Creates'} <code>{preview.path}</code>:
        {/if}
      </p>
      <pre class="diff" data-testid="statusline-diff">{#each preview.diff as line}<span data-kind={line.kind}>{line.kind === 'add' ? '+' : line.kind === 'del' ? '-' : ' '} {line.text}
</span>{/each}</pre>
    </div>
  {/if}
</section>

<style>
  .card {
    padding: 14px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--raised);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h3 {
    margin: 0;
    font-size: 15px;
  }
  h4 {
    margin: 14px 0 4px;
    font-size: 13px;
    color: var(--muted);
  }
  .note {
    margin: 6px 0;
    color: var(--muted);
    font-size: 13px;
  }
  .bad {
    color: var(--bad);
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  .facts {
    display: grid;
    grid-template-columns: 80px minmax(0, 1fr);
    gap: 4px 8px;
    margin: 8px 0;
  }
  .facts dt {
    color: var(--muted);
    font-size: 12px;
  }
  .facts dd {
    margin: 0;
  }
  code,
  pre {
    font-family: var(--mono);
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  pre {
    margin: 6px 0;
    padding: 10px;
    border-radius: 8px;
    background: var(--bg);
    white-space: pre-wrap;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  th,
  td {
    padding: 4px 8px;
    border-bottom: 1px solid var(--line);
    text-align: left;
  }
  th {
    color: var(--muted);
    font-size: 12px;
    font-weight: 500;
  }
  tr[data-stale='true'] td {
    opacity: 0.6;
  }
  .row {
    display: grid;
    grid-template-columns: 100px minmax(0, 1fr);
    align-items: center;
    gap: 8px;
    margin: 8px 0;
  }
  .row input {
    min-width: 0;
    padding: 6px 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--bg);
    font-family: var(--mono);
    font-size: 12px;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  .switch {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .diff span {
    display: block;
  }
  .diff [data-kind='add'] {
    color: #86efac;
  }
  .diff [data-kind='del'] {
    color: var(--bad);
  }
</style>
