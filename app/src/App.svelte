<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onMount, tick } from 'svelte';
  import { append, classify, type ConsoleEntry } from './lib/console.ts';
  import Sources from './routes/Sources.svelte';
  import {
    HELLO_FIELDS,
    connect,
    consoleSend,
    deckInfo,
    describe,
    disconnect,
    listDecks,
    onConsoleLine,
    onLinkClosed,
    type Candidate,
    type DeckInfo,
  } from './lib/deck.ts';

  let title = $state('TouchDeck');
  let tab = $state<'console' | 'sources'>('console');
  let decks = $state<Candidate[]>([]);
  let listed = $state(false);
  let scanning = $state(false);
  let deck = $state<DeckInfo | null>(null);
  let connecting = $state<string | null>(null);
  let problem = $state<string | null>(null);
  let entries = $state<ConsoleEntry[]>([]);
  let line = $state('');
  let log = $state<HTMLOListElement>();

  const online = $derived(deck !== null && deck.online);
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
      await tick();
      performance.mark('connected');
    } catch (error) {
      problem = String(error);
    } finally {
      connecting = null;
    }
  }

  async function send(event: SubmitEvent) {
    event.preventDefault();
    const text = line.trim();
    if (!text || !online) return;
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
    return candidate.status === 'deck' || candidate.status === 'silent';
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
  <nav class="tabs" aria-label="Pages">
    <button class:active={tab === 'console'} aria-pressed={tab === 'console'} onclick={() => (tab = 'console')} data-testid="tab-console">Console</button>
    <button class:active={tab === 'sources'} aria-pressed={tab === 'sources'} onclick={() => (tab = 'sources')} data-testid="tab-sources">Sources</button>
  </nav>
  <div class="status" data-testid="link-status">
    <span class="dot" data-testid="status-dot" data-state={online ? 'online' : 'offline'}></span>
    <span>{status}</span>
  </div>
</header>

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
          {#if online && deck?.port === candidate.port}
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

  {#if tab === 'sources'}
    <Sources />
  {/if}

  <section class="panel console" class:gone={tab !== 'console'} data-testid="console">
    <div class="panel-head">
      <h2>Console</h2>
      <span class="hint">{online ? 'Lines go to the deck console' : 'Connect a deck to use the console'}</span>
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
        placeholder={online ? 'version' : ''}
        disabled={!online}
        autocomplete="off"
        spellcheck="false"
        aria-label="Console line"
        data-testid="console-input"
      />
      <button class="primary" disabled={!online || !line.trim()} data-testid="console-send">Send</button>
    </form>
  </section>
</main>

<style>
  .tabs {
    display: flex;
    gap: 6px;
  }

  .tabs button.active {
    border-color: var(--accent);
    background: var(--raised);
  }

  .gone {
    display: none;
  }
</style>
