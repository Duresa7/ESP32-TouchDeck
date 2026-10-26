<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onMount, tick } from 'svelte';
  import Approval from './lib/actions/Approval.svelte';
  import { append, classify, type ConsoleEntry } from './lib/console.ts';
  import Settings from './lib/settings/Settings.svelte';
  import Actions from './routes/Actions.svelte';
  import Editor from './routes/Editor.svelte';
  import Sources from './routes/Sources.svelte';
  import Update from './routes/Update.svelte';
  import {
    HELLO_FIELDS,
    connect,
    consoleSend,
    deckInfo,
    describe,
    disconnect,
    listDecks,
    netAddHost,
    netForgetDeck,
    netPairCancel,
    netPairCode,
    netPairStart,
    onConsoleLine,
    onLinkClosed,
    type Candidate,
    type DeckInfo,
    type NetPairOffer,
  } from './lib/deck.ts';

  let title = $state('TouchDeck');
  let decks = $state<Candidate[]>([]);
  let listed = $state(false);
  let scanning = $state(false);
  let deck = $state<DeckInfo | null>(null);
  let connecting = $state<string | null>(null);
  let problem = $state<string | null>(null);
  let entries = $state<ConsoleEntry[]>([]);
  let line = $state('');
  let log = $state<HTMLOListElement>();
  let manualIp = $state('');
  let pairOffer = $state<NetPairOffer | null>(null);
  let pairCode = $state('');
  let pairBusy = $state(false);
  let pairLocked = $state(false);
  let pairError = $state<string | null>(null);
  let view = $state<'deck' | 'editor' | 'actions' | 'settings' | 'sources' | 'update'>('deck');
  const VIEWS = [
    ['deck', 'Deck'],
    ['editor', 'Editor'],
    ['actions', 'Actions'],
    ['settings', 'Settings'],
    ['sources', 'Sources'],
    ['update', 'Update'],
  ] as const;

  const online = $derived(deck !== null && deck.online);
  const isWifiLink = $derived(deck !== null && deck.port.startsWith('net:'));
  const status = $derived(online && deck ? `Connected to ${deck.port}` : problem ? `Offline (${problem})` : 'Not connected');

  async function refresh() {
    scanning = true;
    try {
      decks = await listDecks();
    } catch (error) {
      problem = String(error);
    } finally {
      scanning = false;
    }
    if (!listed) {
      listed = true;
      await tick();
      performance.mark('list-ready');
    }
  }

  async function connectTo(port: string) {
    connecting = port;
    problem = null;
    performance.mark('connect-start');
    try {
      deck = await connect(port);
      await refresh();
      await tick();
      performance.mark('connected');
    } catch (error) {
      problem = String(error);
    } finally {
      connecting = null;
    }
  }

  async function addIp(event: SubmitEvent) {
    event.preventDefault();
    const target = manualIp.trim();
    if (!target) return;
    problem = null;
    try {
      await netAddHost(target);
      manualIp = '';
      await refresh();
    } catch (error) {
      problem = String(error);
    }
  }

  async function startPair(candidate: Candidate) {
    problem = null;
    pairError = null;
    pairCode = '';
    pairLocked = false;
    pairBusy = true;
    const name = candidate.status === 'net-unpaired' || candidate.status === 'net-paired' ? candidate.name : undefined;
    try {
      pairOffer = await netPairStart(candidate.port, name);
    } catch (error) {
      problem = String(error);
    } finally {
      pairBusy = false;
    }
  }

  async function submitPair(event: SubmitEvent) {
    event.preventDefault();
    const code = pairCode.trim();
    if (!pairOffer || pairBusy || pairLocked || !/^[0-9]{6}$/.test(code)) return;
    pairBusy = true;
    pairError = null;
    try {
      const step = await netPairCode(code);
      if (step.paired) {
        const targetPort = step.port;
        pairOffer = null;
        pairCode = '';
        await refresh();
        await connectTo(targetPort);
      } else {
        pairError = step.error ?? 'Authentication failed.';
        pairLocked = step.locked;
        pairCode = '';
        if (step.locked) {
          await refresh();
        }
      }
    } catch (error) {
      pairError = String(error);
      pairLocked = true;
    } finally {
      pairBusy = false;
    }
  }

  async function cancelPair() {
    pairOffer = null;
    pairCode = '';
    pairError = null;
    pairLocked = false;
    try {
      await netPairCancel();
    } catch {
      // Ignore cancel cleanup error
    }
  }

  async function forgetPairedDeck(candidate: Candidate) {
    const deckId = candidate.status === 'net-paired' ? candidate.deck_id : undefined;
    try {
      if (deck?.port === candidate.port) {
        deck = null;
      }
      await netForgetDeck(candidate.port, deckId);
      await refresh();
    } catch (error) {
      problem = String(error);
    }
  }

  async function send(event: SubmitEvent) {
    event.preventDefault();
    const text = line.trim();
    if (!text || !online || isWifiLink) return;
    entries = append(entries, { kind: 'sent', text });
    line = '';
    try {
      await consoleSend(text);
    } catch (error) {
      problem = String(error);
    }
  }

  function show(value: unknown) {
    return Array.isArray(value) ? value.join(', ') : String(value);
  }

  function connectable(candidate: Candidate) {
    return candidate.status === 'deck' || candidate.status === 'silent' || candidate.status === 'net-paired';
  }

  $effect(() => {
    if (entries.length && log) {
      tick().then(() => log?.scrollTo({ top: log.scrollHeight }));
    }
  });

  onMount(() => {
    const listeners = [
      onConsoleLine((text) => (entries = append(entries, classify(text)))),
      onLinkClosed((closed) => {
        if (deck?.session !== closed.session) return;
        deck = null;
        problem = closed.reason;
        refresh();
      }),
    ];
    getCurrentWindow()
      .title()
      .then((name) => {
        title = name;
        document.title = name;
      });
    deckInfo().then((info) => {
      if (info) deck = info;
    });
    refresh();
    return () => listeners.forEach((listener) => listener.then((stop) => stop()));
  });
</script>

<header class="bar">
  <div class="brand">
    <span class="mark" aria-hidden="true"></span>
    <span data-testid="title">{title}</span>
  </div>
  <nav class="views" aria-label="Views">
    {#each VIEWS as [id, name] (id)}
      <button class:active={view === id} onclick={() => (view = id)} aria-current={view === id ? 'page' : undefined} data-testid="nav-{id}"
        >{name}</button
      >
    {/each}
  </nav>
  <div class="status" data-testid="link-status">
    <span class="dot" data-testid="status-dot" data-state={online ? 'online' : 'offline'}></span>
    <span>{status}</span>
  </div>
</header>

{#if view === 'update'}
  <Update port={deck?.port ?? ''} />
{:else if view === 'sources'}
  <Sources />
{:else if view === 'editor'}
  <Editor {online} />
{:else if view === 'actions'}
  <Actions {online} />
{:else if view === 'settings'}
  <Settings {online} session={online && deck ? deck.session : null} />
{:else}
<main class="layout">
  <section class="panel side">
    <div class="panel-head">
      <h2>Decks</h2>
      <button onclick={refresh} disabled={scanning} data-testid="refresh">{scanning ? 'Scanning' : 'Refresh'}</button>
    </div>
    <ul class="decks" data-testid="deck-list">
      {#each decks as candidate (candidate.port)}
        <li class="deck" data-testid="deck-row" data-port={candidate.port} data-status={candidate.status}>
          <div class="deck-text">
            <span class="port">{candidate.port}</span>
            <span class="detail">{describe(candidate)}</span>
          </div>
          {#if candidate.status === 'net-unpaired'}
            <button
              class="primary"
              onclick={() => startPair(candidate)}
              disabled={pairBusy || connecting !== null}
              data-testid="pair">Pair</button
            >
          {:else if candidate.status === 'net-paired'}
            <div class="deck-actions">
              {#if online && deck?.port === candidate.port}
                <button onclick={() => disconnect().then(() => { deck = null; refresh(); })} data-testid="disconnect">Disconnect</button>
              {:else}
                <button
                  class="primary"
                  onclick={() => connectTo(candidate.port)}
                  disabled={!connectable(candidate) || connecting !== null}
                  data-testid="connect">{connecting === candidate.port ? 'Connecting' : 'Connect'}</button
                >
              {/if}
              <button onclick={() => forgetPairedDeck(candidate)} data-testid="net-forget-deck">Forget</button>
            </div>
          {:else if online && deck?.port === candidate.port}
            <button onclick={() => disconnect()} data-testid="disconnect">Disconnect</button>
          {:else}
            <button
              class="primary"
              onclick={() => connectTo(candidate.port)}
              disabled={!connectable(candidate) || connecting !== null}
              data-testid="connect">{connecting === candidate.port ? 'Connecting' : 'Connect'}</button
            >
          {/if}
        </li>
      {:else}
        {#if listed && !scanning}
          <li class="empty">No TouchDeck found. Plug the deck into USB and press Refresh.</li>
        {/if}
      {/each}
    </ul>

    <form class="add-ip" onsubmit={addIp} data-testid="net-ip-form">
      <input
        bind:value={manualIp}
        placeholder="Add deck by IP (e.g. 192.168.1.50)"
        autocomplete="off"
        spellcheck="false"
        aria-label="Deck IP address"
        data-testid="net-ip-input"
      />
      <button type="submit" disabled={!manualIp.trim()} data-testid="net-ip-add">Add IP</button>
    </form>

    {#if deck}
      <dl class="hello" data-testid="deck-info">
        {#each HELLO_FIELDS as field (field)}
          <div class="field" data-testid="hello-{field}">
            <dt>{field}</dt>
            <dd>{show(deck.hello[field])}</dd>
          </div>
        {/each}
      </dl>
    {/if}
  </section>

  <section class="panel console" data-testid="console">
    <div class="panel-head">
      <h2>Console</h2>
      <span class="hint">{isWifiLink ? 'Console is USB-only' : online ? 'Lines go to the deck console' : 'Connect a deck to use the console'}</span>
    </div>
    <ol class="log" bind:this={log} data-testid="console-log">
      {#each entries as entry}
        <li class={entry.kind} data-testid="console-line" data-kind={entry.kind} data-ok={entry.kind === 'reply' ? String(entry.ok) : null}>
          {entry.kind === 'sent' ? `> ${entry.text}` : entry.text}
        </li>
      {/each}
    </ol>
    <form class="prompt" onsubmit={send}>
      <input
        bind:value={line}
        placeholder={online && !isWifiLink ? 'version' : ''}
        disabled={!online || isWifiLink}
        autocomplete="off"
        spellcheck="false"
        aria-label="Console line"
        data-testid="console-input"
      />
      <button class="primary" disabled={!online || isWifiLink || !line.trim()} data-testid="console-send">Send</button>
    </form>
  </section>
</main>
{/if}

{#if pairOffer}
  <div class="pair-backdrop" role="dialog" aria-modal="true" aria-labelledby="pair-title" data-testid="pair-modal">
    <form class="pair-dialog" onsubmit={submitPair}>
      <h3 id="pair-title">Pair over Wi-Fi</h3>
      <p class="pair-sub">
        Deck <code>{pairOffer.name ?? pairOffer.host}</code> (<code>{pairOffer.deck_id}</code>) is showing a 6-digit pairing code.
      </p>
      <label class="pair-field">
        <span>6-digit code</span>
        <input
          bind:value={pairCode}
          maxlength="6"
          inputmode="numeric"
          placeholder="000000"
          autocomplete="off"
          disabled={pairBusy || pairLocked}
          data-testid="pair-code-input"
        />
      </label>
      {#if pairError}
        <p class="pair-error" data-testid="pair-error">{pairError}</p>
      {/if}
      <div class="pair-actions">
        <button type="button" onclick={cancelPair} data-testid="pair-cancel">{pairLocked ? 'Close' : 'Cancel'}</button>
        <button
          type="submit"
          class="primary"
          disabled={pairBusy || pairLocked || !/^[0-9]{6}$/.test(pairCode.trim())}
          data-testid="pair-submit">{pairBusy ? 'Pairing…' : 'Pair & Connect'}</button
        >
      </div>
    </form>
  </div>
{/if}

<Approval />

<style>
  .views {
    display: flex;
    gap: 4px;
    margin-right: auto;
    margin-left: 12px;
  }

  .views button {
    border-color: transparent;
    background: transparent;
    color: var(--muted);
  }

  .views button.active {
    border-color: var(--line);
    background: var(--raised);
    color: var(--text);
  }

  .deck-actions {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .add-ip {
    display: flex;
    gap: 6px;
    margin-top: 8px;
  }

  .add-ip input {
    flex: 1;
  }

  .pair-backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(8, 11, 16, 0.78);
  }

  .pair-dialog {
    width: min(400px, 92vw);
    padding: 18px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--panel, #141a24);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .pair-dialog h3 {
    margin: 0;
    font-size: 16px;
  }

  .pair-sub {
    margin: 0;
    color: var(--muted);
    font-size: 13px;
    line-height: 1.4;
  }

  .pair-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 13px;
  }

  .pair-field input {
    font-family: var(--mono);
    font-size: 18px;
    letter-spacing: 0.18em;
    text-align: center;
  }

  .pair-error {
    margin: 0;
    color: #ff6b6b;
    font-size: 13px;
  }

  .pair-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  :global(select),
  :global(textarea),
  :global(input:not([type='checkbox'])) {
    min-width: 0;
    padding: 6px 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: #0f141d;
    color: var(--text);
    font: inherit;
  }

  :global(textarea) {
    resize: vertical;
    font-family: var(--mono);
    font-size: 12px;
  }

  :global(select:focus),
  :global(textarea:focus),
  :global(input:focus) {
    border-color: var(--accent);
    outline: none;
  }
</style>