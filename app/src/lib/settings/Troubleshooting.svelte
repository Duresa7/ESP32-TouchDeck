<script lang="ts">
  import { tick } from 'svelte';
  import ButtonActivity from '../actions/ButtonActivity.svelte';
  import KeyMonitor from '../actions/KeyMonitor.svelte';
  import { deck, entries, friendlyConnection, lost, online, problem, sendConsole } from '../connection.ts';
  import { HELLO_FIELDS } from '../deck.ts';

  let { buildSha }: { buildSha: string } = $props();

  let line = $state('');
  let log = $state<HTMLOListElement>();

  /** The deck's hello fields, named for a person. */
  const NAMES: Record<(typeof HELLO_FIELDS)[number], string> = {
    product: 'Device',
    proto: 'Link version',
    elf_sha256: 'Software build ID',
    version: 'Software version',
    running: 'Running from',
    sinks: 'Sends keys through',
    recovery: 'Safe mode',
    layout: 'Layout on screen',
  };

  function show(value: unknown) {
    if (value === true) return 'yes';
    if (value === false) return 'no';
    return Array.isArray(value) ? value.join(', ') : String(value);
  }

  async function send(event: SubmitEvent) {
    event.preventDefault();
    const text = line.trim();
    if (!text || !$online) return;
    line = '';
    await sendConsole(text);
  }

  $effect(() => {
    if ($entries.length && log) {
      tick().then(() => log?.scrollTo({ top: log.scrollHeight }));
    }
  });
</script>

<div class="help">
  <header class="intro">
    <h1>Troubleshooting</h1>
    <p class="hint">What Open TouchDeck knows about your deck, and tools to check that buttons reach this computer. You won't need these day to day.</p>
  </header>

  <section class="section" aria-labelledby="help-connection">
    <header>
      <h3 id="help-connection">Connection</h3>
      {#if $online && $deck}
        <p class="hint">Connected through {$deck.port === 'mock' ? 'the practice deck built into Open TouchDeck' : `USB port ${$deck.port}`}.</p>
      {:else if $lost && $problem}
        <p class="hint">{friendlyConnection($problem)}</p>
      {:else}
        <p class="hint">No deck is connected. Plug it in with a USB cable, then choose Connect at the top right.</p>
      {/if}
    </header>
    {#if $problem}
      <details class="more"><summary>Last connection message</summary><div class="body" data-testid="connection-problem">{$problem}</div></details>
    {/if}
    {#if $deck}
      <dl class="facts" data-testid="deck-info">
        {#each HELLO_FIELDS as field (field)}
          <div class="fact" data-testid="hello-{field}">
            <dt>{NAMES[field]}</dt>
            <dd>{show($deck.hello[field])}</dd>
          </div>
        {/each}
      </dl>
    {/if}
    {#if buildSha}
      <p class="hint">App build <code data-testid="app-build">{buildSha}</code></p>
    {/if}
  </section>

  <section class="section" aria-labelledby="help-keys">
    <header>
      <h3 id="help-keys">Test your keys</h3>
      <p class="hint">Click in the box, then press buttons on your deck. Each key the deck sends to this computer shows up here.</p>
    </header>
    <KeyMonitor />
  </section>

  <section class="section" aria-labelledby="help-activity">
    <header>
      <h3 id="help-activity">Button activity</h3>
      <p class="hint">Buttons that open apps or run commands, and what happened each time one was pressed.</p>
    </header>
    <ButtonActivity />
  </section>

  <section class="section console" data-testid="console" aria-labelledby="help-console">
    <header>
      <h3 id="help-console">Deck console</h3>
      <p class="hint">Send a text command straight to the deck's built-in console. For example, <code>version</code> or <code>layout state</code>.</p>
    </header>
    <ol class="log" bind:this={log} data-testid="console-log" aria-live="polite">
      {#each $entries as entry, i (i)}
        <li class={entry.kind} data-testid="console-line" data-kind={entry.kind} data-ok={entry.kind === 'reply' ? String(entry.ok) : null}>
          {entry.kind === 'sent' ? `> ${entry.text}` : entry.text}
        </li>
      {:else}
        <li class="none">{$online ? 'Nothing yet. Send a command below.' : 'Connect your deck to use the console.'}</li>
      {/each}
    </ol>
    <form class="prompt" onsubmit={send}>
      <input
        bind:value={line}
        placeholder={$online ? 'version' : ''}
        disabled={!$online}
        autocomplete="off"
        spellcheck="false"
        aria-label="Console command"
        data-testid="console-input"
      />
      <button class="primary" disabled={!$online || !line.trim()} data-testid="console-send">Send</button>
    </form>
  </section>
</div>

<style>
  .help {
    display: grid;
    gap: var(--space-5);
  }

  .intro {
    display: grid;
    gap: var(--space-2);
  }

  .facts {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-3) var(--space-5);
    margin: 0;
  }

  .fact {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .fact dt {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .fact dd {
    margin: 0;
    font-family: var(--mono);
    font-size: var(--text-s);
    overflow-wrap: anywhere;
  }

  .log {
    height: 200px;
    margin: 0;
    padding: var(--space-3);
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    background: var(--sunken);
    font-family: var(--mono);
    font-size: var(--text-s);
    list-style: none;
  }

  .log li {
    padding: 1px 0;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }

  .log .sent {
    color: #9cc8ff;
  }

  .log .log,
  .log .none {
    color: var(--muted);
  }

  .log .reply[data-ok='true'] {
    color: #8fe0b0;
  }

  .log .reply[data-ok='false'] {
    color: var(--bad);
  }

  .prompt {
    display: flex;
    gap: var(--space-2);
  }

  .prompt input {
    flex: 1;
    font-family: var(--mono);
  }
</style>
