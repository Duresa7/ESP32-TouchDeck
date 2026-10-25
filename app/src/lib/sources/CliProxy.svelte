<script lang="ts">
  import { ago, clearManagementKey, refreshSource, setManagementKey, type Config, type ProxyInfo, type SourcesView } from './api.ts';

  let { view, draft, info }: { view: SourcesView; draft: Config; info: ProxyInfo | undefined } = $props();

  let key = $state('');
  let hint = $state<string | null>(null);
  let problem = $state<string | null>(null);
  let busy = $state(false);

  const shownHint = $derived(hint ?? info?.key_hint ?? view.key_hint);
  const phase = $derived(!draft.cliproxy ? 'off' : info?.ok == null ? 'idle' : info.ok ? 'ok' : info.stale ? 'stale' : 'failing');

  async function saveKey() {
    busy = true;
    problem = null;
    try {
      hint = await setManagementKey(key);
      key = '';
    } catch (error) {
      problem = String(error);
    } finally {
      busy = false;
    }
  }

  async function clearKey() {
    busy = true;
    problem = null;
    try {
      await clearManagementKey();
      hint = '';
    } catch (error) {
      problem = String(error);
    } finally {
      busy = false;
    }
  }
</script>

<section class="card" data-testid="proxy-card">
  <header>
    <h3>CLIProxyAPI</h3>
    <label class="switch">
      <input type="checkbox" bind:checked={draft.cliproxy} data-testid="proxy-enabled" />
      <span>On</span>
    </label>
  </header>

  <p class="note" data-testid="proxy-state" data-state={phase} data-fails={info?.fails ?? 0}>
    {#if phase === 'off'}
      Off. Turn it on and save to poll the management API.
    {:else if phase === 'idle'}
      Waiting for the first poll.
    {:else if phase === 'ok'}
      {info?.accounts.length} account{info?.accounts.length === 1 ? '' : 's'}, polled {ago(info?.polled ?? null)}.
    {:else if phase === 'stale'}
      Stale after {info?.fails} failed polls. The deck dims its CLIProxyAPI tiles.
    {:else}
      Failing, {info?.fails} in a row.
    {/if}
  </p>
  {#if info?.error && phase !== 'ok'}<p class="bad" data-testid="proxy-error">{info.error}</p>{/if}

  <label class="row">
    <span>Base URL</span>
    <input bind:value={draft.cliproxy_url} spellcheck="false" data-testid="proxy-url" />
  </label>
  <label class="row">
    <span>Poll every (s)</span>
    <input type="number" min="1" bind:value={draft.cliproxy_interval_s} data-testid="proxy-interval" />
  </label>
  <div class="row">
    <span>Management key</span>
    <div class="key">
      <input type="password" bind:value={key} placeholder={shownHint ? `saved, ends in ${shownHint}` : 'not set'} autocomplete="off" data-testid="proxy-key" />
      <button onclick={saveKey} disabled={busy || !key.trim()} data-testid="proxy-key-save">Save key</button>
      <button onclick={clearKey} disabled={busy || !shownHint} data-testid="proxy-key-clear">Clear</button>
    </div>
  </div>
  <p class="note">
    The key lives in the operating system's credential store, never in the config file. Last 4 characters: <code data-testid="proxy-key-hint">{shownHint ?? 'none'}</code>
  </p>
  {#if problem}<p class="bad" data-testid="proxy-key-error">{problem}</p>{/if}

  <div class="actions">
    <button onclick={() => refreshSource('cliproxyapi')} disabled={!draft.cliproxy} data-testid="proxy-refresh">Refresh now</button>
  </div>

  {#if info && info.accounts.length}
    <table data-testid="proxy-accounts">
      <thead><tr><th>Account</th><th>Provider</th><th>Status</th><th>OK</th><th>Failed</th><th>Last 10 min</th></tr></thead>
      <tbody>
        {#each info.accounts as account (account.email)}
          <tr data-testid="proxy-account" data-email={account.email} data-status={account.status} data-stale={info.stale}>
            <td>{account.email}</td>
            <td>{account.provider}</td>
            <td>{account.status}</td>
            <td>{account.success}</td>
            <td>{account.failed}</td>
            <td>{account.recent_10m}</td>
          </tr>
        {/each}
      </tbody>
    </table>
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
  code {
    font-family: var(--mono);
    font-size: 12px;
  }
  .row {
    display: grid;
    grid-template-columns: 110px minmax(0, 1fr);
    align-items: center;
    gap: 8px;
    margin: 8px 0;
  }
  input:not([type='checkbox']) {
    min-width: 0;
    padding: 6px 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--bg);
    font-size: 13px;
  }
  .key {
    display: flex;
    gap: 8px;
  }
  .key input {
    flex: 1;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin: 8px 0;
  }
  .switch {
    display: flex;
    align-items: center;
    gap: 6px;
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
</style>
