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
      problem = "Couldn't copy. Select the account in the table and copy it instead.";
    }
  }
</script>

<section class="section" data-testid="proxy-card">
  <header class="head">
    <div>
      <h3>Connect to your CLIProxyAPI hub</h3>
      <p class="hint">CLIProxyAPI is a hub that signs in to your AI accounts. Open TouchDeck asks it, read-only, how much of each limit is used.</p>
    </div>
    <label class="switch">
      <input type="checkbox" bind:checked={draft.cliproxy} aria-label="Read usage from the hub" data-testid="proxy-enabled" />
      <span class="track"></span>
    </label>
  </header>

  <p class="state" data-testid="proxy-state" data-state={phase} data-fails={info?.fails ?? 0}>
    <span class="dot" data-phase={phase} aria-hidden="true"></span>
    {#if phase === 'off'}
      Off. Turn it on and save to start reading usage.
    {:else if phase === 'idle'}
      Checking the hub for the first time…
    {:else if phase === 'ok'}
      Reading {info?.accounts.length} account{info?.accounts.length === 1 ? '' : 's'}. Last checked {ago(info?.polled ?? null)}.
    {:else if phase === 'stale'}
      The hub hasn't answered {info?.fails} times in a row, so the deck greys out its AI usage buttons until it does.
    {:else}
      The hub didn't answer the last {info?.fails === 1 ? 'check' : `${info?.fails} checks`}. Open TouchDeck keeps trying.
    {/if}
  </p>
  {#if info?.error && phase !== 'ok'}<p class="problem" data-testid="proxy-error">{info.error}</p>{/if}

  <div class="fields">
    <label class="field">
      <span class="name">Hub address</span>
      <input bind:value={draft.cliproxy_url} spellcheck="false" placeholder="https://hub.example.com" data-testid="proxy-url" />
    </label>
    <label class="field short">
      <span class="name">Check every</span>
      <div class="with-unit">
        <input type="number" min="60" bind:value={draft.cliproxy_interval_s} data-testid="proxy-interval" />
        <span>seconds</span>
      </div>
    </label>
  </div>
  {#if info?.insecure_http}
    <p class="notice" data-tone="warn" role="status" data-testid="proxy-http-warning">This hub address starts with http:// and isn't on this computer, so the management key travels unencrypted. Use an https:// address to protect it.</p>
  {/if}
  <div class="field">
    <span class="name">Management key</span>
    <div class="key">
      <input type="password" bind:value={key} placeholder={shownHint ? `Saved, ends in ${shownHint}` : 'Not set yet'} autocomplete="off" aria-label="Management key" data-testid="proxy-key" />
      <button onclick={saveKey} disabled={busy || !key.trim()} data-testid="proxy-key-save">Save key</button>
      <button class="quiet" onclick={clearKey} disabled={busy || !shownHint} data-testid="proxy-key-clear">Forget key</button>
    </div>
    <span class="hint">Kept in your computer's secure password store, never in a file. Last 4 characters: <code data-testid="proxy-key-hint">{shownHint ?? 'none'}</code></span>
  </div>
  {#if problem}<p class="problem" data-testid="proxy-key-error">{problem}</p>{/if}

  <div class="actions">
    <button class="small" onclick={() => refreshSource('cliproxyapi')} disabled={!draft.cliproxy} data-testid="proxy-refresh">Check now</button>
    <span class="hint">Each check asks your AI services through the hub, so Open TouchDeck waits at least 15 seconds between them.</span>
  </div>

  {#if info && info.accounts.length}
    <div class="accounts-scroll">
    <table data-testid="proxy-accounts">
      <thead><tr><th>Account</th><th>Service</th><th>Plan</th><th>5-hour limit</th><th>7-day limit</th><th>Status</th></tr></thead>
      <tbody>
        {#each info.accounts as account, index (`${account.key}:${index}`)}
          <tr data-testid="proxy-account" data-email={account.email} data-account-key={account.key} data-status={account.status} data-stale={info.stale}>
            <td class="account-key">
              <code data-testid="proxy-account-key">{account.key}</code>
              <button class="copy small quiet" onclick={() => copyAccount(account.key)} aria-label={`Copy ${account.key}`} data-testid="proxy-account-copy">{copied === account.key ? 'Copied' : 'Copy'}</button>
            </td>
            <td data-testid="proxy-account-provider">{account.provider}</td>
            <td data-testid="proxy-account-plan">{account.plan ?? '—'}</td>
            {#each [{ name: 'five-hour', window: account.five_hour }, { name: 'seven-day', window: account.seven_day }] as { name, window } (name)}
              <td class="window" data-testid={`proxy-account-${name}`}>
                {#if window}
                  <strong>{Math.round(window.pct)}%</strong>
                  <div class="meter" aria-hidden="true"><span style:width={`${window.pct}%`}></span></div>
                  <small>{window.reset_in === 'now' ? 'resets now' : window.reset_in ? `resets in ${window.reset_in}` : 'reset time unknown'}</small>
                {:else}
                  <span class="muted">—</span>
                {/if}
              </td>
            {/each}
            <td class="account-status" data-testid="proxy-account-status">
              <span class="badge">{account.status}</span>
              {#if account.usage_error}<small class="problem" data-testid="proxy-account-usage-error">{account.usage_error}</small>{/if}
              <small>{account.success} worked · {account.failed} failed · {account.recent_10m} in 10 min</small>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    </div>
    <p class="hint">Limits show the share already used. Copy an account to show only that account on a button. Gemini and some other services only report request counts for now.</p>
  {/if}
</section>

<style>
  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-4);
  }

  .head > div {
    display: grid;
    gap: var(--space-1);
  }

  .state {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--muted);
  }

  .state .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--faint);
  }

  .state .dot[data-phase='ok'] {
    background: var(--good);
  }

  .state .dot[data-phase='stale'],
  .state .dot[data-phase='failing'] {
    background: var(--warn);
  }

  .fields {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 200px;
    gap: var(--space-3);
  }

  .with-unit {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--muted);
  }

  .key {
    display: flex;
    gap: var(--space-2);
  }

  .key input {
    flex: 1;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-3);
  }

  table {
    width: 100%;
    min-width: 730px;
    border-collapse: collapse;
    font-size: var(--text-s);
  }

  th,
  td {
    padding: 10px 8px;
    border-bottom: 1px solid var(--line);
    text-align: left;
    vertical-align: top;
  }

  th {
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 500;
  }

  tr[data-stale='true'] td {
    opacity: 0.6;
  }

  .accounts-scroll {
    overflow-x: auto;
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    background: var(--panel);
  }

  .account-key {
    max-width: 230px;
  }

  .account-key code {
    display: block;
    overflow-wrap: anywhere;
    user-select: text;
  }

  .copy {
    margin-top: 4px;
  }

  .window {
    min-width: 100px;
  }

  .window strong {
    font-variant-numeric: tabular-nums;
    font-weight: 600;
  }

  small {
    display: block;
    margin-top: 4px;
    color: var(--muted);
    font-size: 11px;
  }

  .account-status {
    max-width: 190px;
  }

  .badge {
    font-size: var(--text-xs);
  }

  .muted {
    color: var(--muted);
  }

  .meter {
    height: 4px;
    margin-top: 5px;
    overflow: hidden;
    border-radius: 4px;
    background: var(--line);
  }

  .meter span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
  }
</style>
