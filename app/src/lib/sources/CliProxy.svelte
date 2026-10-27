<script lang="ts">
  import { ago, clearManagementKey, refreshSource, setManagementKey, type Config, type ProxyInfo, type SourcesView } from './api.ts';

  let { view, draft, info }: { view: SourcesView; draft: Config; info: ProxyInfo | undefined } = $props();

  let key = $state('');
  let hint = $state<string | null>(null);
  let problem = $state<string | null>(null);
  let busy = $state(false);
  let copied = $state<string | null>(null);

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

  async function copyAccount(account: string) {
    try {
      await navigator.clipboard.writeText(account);
      copied = account;
    } catch {
      problem = 'Could not copy the account key. Select and copy the text in the table.';
    }
  }
</script>

<section class="card" data-testid="proxy-card">
  <header>
    <h3>CLIProxyAPI usage limits</h3>
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
    <span>Hub URL</span>
    <input bind:value={draft.cliproxy_url} spellcheck="false" data-testid="proxy-url" />
  </label>
  <label class="row">
    <span>Poll every (s)</span>
    <input type="number" min="60" bind:value={draft.cliproxy_interval_s} data-testid="proxy-interval" />
  </label>
  {#if info?.insecure_http}
    <p class="warning" role="status" data-testid="proxy-http-warning">This hub uses plain HTTP outside this PC. The management key travels unencrypted. Use HTTPS to protect it.</p>
  {/if}
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
    <span class="note">Each poll asks the providers through your hub. Refreshes are limited to once every 15 s.</span>
  </div>

  {#if info && info.accounts.length}
    <div class="accounts-scroll">
    <table data-testid="proxy-accounts">
      <thead><tr><th>Account key</th><th>Provider</th><th>Plan</th><th>5-hour</th><th>7-day</th><th>Status</th></tr></thead>
      <tbody>
        {#each info.accounts as account, index (`${account.key}:${index}`)}
          <tr data-testid="proxy-account" data-email={account.email} data-account-key={account.key} data-status={account.status} data-stale={info.stale}>
            <td class="account-key">
              <code data-testid="proxy-account-key">{account.key}</code>
              <button class="copy" onclick={() => copyAccount(account.key)} aria-label={`Copy ${account.key}`} data-testid="proxy-account-copy">{copied === account.key ? 'Copied' : 'Copy'}</button>
            </td>
            <td data-testid="proxy-account-provider">{account.provider}</td>
            <td data-testid="proxy-account-plan">{account.plan ?? '—'}</td>
            {#each [{ name: 'five-hour', window: account.five_hour }, { name: 'seven-day', window: account.seven_day }] as { name, window } (name)}
              <td class="window" data-testid={`proxy-account-${name}`}>
                {#if window}
                  <strong>{Math.round(window.pct)}%</strong>
                  <div class="meter" aria-hidden="true"><span style:width={`${window.pct}%`}></span></div>
                  <small>{window.reset_in === 'now' ? 'resets now' : window.reset_in ? `resets in ${window.reset_in}` : 'reset time unavailable'}</small>
                {:else}
                  <span class="muted">—</span>
                {/if}
              </td>
            {/each}
            <td class="status" data-testid="proxy-account-status">
              <span class="badge">{account.status}</span>
              {#if account.usage_error}<small class="bad" data-testid="proxy-account-usage-error">{account.usage_error}</small>{/if}
              <small>{account.success} OK · {account.failed} failed · {account.recent_10m} / 10m</small>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    </div>
    <p class="note">Usage is the percentage used. Gemini and other providers show request counts only for now.</p>
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
    align-items: center;
    flex-wrap: wrap;
  }
  .switch {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  table {
    width: 100%;
    min-width: 730px;
    border-collapse: collapse;
    font-size: 13px;
  }
  th,
  td {
    padding: 10px 8px;
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
  .accounts-scroll { overflow-x: auto; margin-top: 12px; }
  .account-key { max-width: 230px; }
  .account-key code { display: block; overflow-wrap: anywhere; user-select: text; }
  .copy { padding: 2px 7px; margin-top: 5px; font-size: 11px; }
  .window { min-width: 100px; }
  .window strong { font-variant-numeric: tabular-nums; font-weight: 600; }
  small { display: block; margin-top: 4px; font-size: 11px; color: var(--muted); }
  .status { max-width: 190px; }
  .badge { font-size: 12px; }
  .muted { color: var(--muted); }
  .meter { height: 4px; margin-top: 5px; border-radius: 4px; background: var(--line); overflow: hidden; }
  .meter span { display: block; height: 100%; background: var(--accent); border-radius: inherit; }
  .warning { padding: 8px 10px; border: 1px solid var(--bad); border-radius: 8px; font-size: 13px; color: var(--bad); }
</style>
