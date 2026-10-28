<script lang="ts">
  import { connectable, connecting, connectTo, deck, decks, describeCandidate, friendlyConnection, leave, listed, lost, online, problem, refresh, scanning } from '../connection.ts';

  let { onhelp }: { onhelp: () => void } = $props();

  let open = $state(false);
  let root = $state<HTMLDivElement>();

  const phase = $derived($online ? 'online' : $connecting ? 'connecting' : $lost ? 'lost' : 'offline');
  const label = $derived(
    { online: 'Deck connected', connecting: 'Connecting…', lost: 'Connection lost', offline: 'Deck not connected' }[phase],
  );
  const found = $derived($decks.filter((candidate) => candidate.status === 'deck' || candidate.status === 'silent'));
  const others = $derived($decks.filter((candidate) => !connectable(candidate)));

  function toggle() {
    open = !open;
    if (open && !$scanning) refresh();
  }

  function outside(event: PointerEvent) {
    if (open && root && !root.contains(event.target as Node)) open = false;
  }

  function keys(event: KeyboardEvent) {
    if (open && event.key === 'Escape') {
      open = false;
      root?.querySelector<HTMLButtonElement>('.pill')?.focus();
    }
  }
</script>

<svelte:window onpointerdown={outside} onkeydown={keys} />

<div class="connection" bind:this={root}>
  <button
    class="pill"
    data-state={phase}
    onclick={toggle}
    aria-expanded={open}
    aria-controls="connection-panel"
    data-testid="connection"
  >
    <span class="dot" data-testid="status-dot" data-state={$online ? 'online' : 'offline'} aria-hidden="true"></span>
    <span data-testid="link-status">{label}</span>
    <svg class="chevron" viewBox="0 0 16 16" aria-hidden="true"><path d="M4 6l4 4 4-4" /></svg>
  </button>

  {#if open}
    <div class="panel" id="connection-panel" role="dialog" aria-label="Your deck" data-testid="connection-panel">
      {#if $online && $deck}
        <header>
          <h3>Your deck is connected</h3>
          <p class="hint">It's using this computer's {$deck.port === 'mock' ? 'practice deck' : `USB port ${$deck.port}`}.</p>
        </header>
      {:else if $lost && $problem}
        <div class="notice" data-tone="warn" role="status">
          <div>
            <p>{friendlyConnection($problem)}</p>
            <details class="more"><summary>Details</summary><div class="body">{$problem}</div></details>
          </div>
        </div>
      {:else}
        <header>
          <h3>Connect your deck</h3>
          <p class="hint">Plug your Device into this computer with a USB cable, then choose it below.</p>
        </header>
      {/if}

      <ul class="decks" data-testid="deck-list">
        {#each found as candidate (candidate.port)}
          <li class="deck" data-testid="deck-row" data-port={candidate.port} data-status={candidate.status}>
            <svg class="glyph" viewBox="0 0 24 24" aria-hidden="true"><rect x="2.5" y="5" width="19" height="14" rx="2.5" /><path d="M7 9.5h3M14 9.5h3M7 14.5h3M14 14.5h3" /></svg>
            <div class="text">
              <span class="name">{candidate.port === 'mock' ? 'Practice deck' : 'Open TouchDeck'}</span>
              <span class="detail">{describeCandidate(candidate)}{candidate.port === 'mock' ? '' : ` · ${candidate.port}`}</span>
            </div>
            {#if $online && $deck?.port === candidate.port}
              <button class="small" onclick={() => leave()} data-testid="disconnect">Disconnect</button>
            {:else}
              <button
                class="small primary"
                onclick={() => connectTo(candidate.port)}
                disabled={$connecting !== null || ($online && $deck?.port !== candidate.port)}
                data-testid="connect">{$connecting === candidate.port ? 'Connecting…' : 'Connect'}</button
              >
            {/if}
          </li>
        {/each}
        {#each others as candidate (candidate.port)}
          <li class="deck other" data-testid="deck-row" data-port={candidate.port} data-status={candidate.status}>
            <div class="text">
              <span class="name">{candidate.port}</span>
              <span class="detail">{describeCandidate(candidate)}</span>
            </div>
            <button class="small" disabled data-testid="connect">Connect</button>
          </li>
        {/each}
        {#if $listed && !$scanning && $decks.length === 0}
          <li class="empty">No deck found. Check the USB cable, then look again.</li>
        {/if}
      </ul>

      {#if !$lost && $problem && !$online}
        <p class="problem" role="alert">{friendlyConnection($problem)}</p>
      {/if}

      <footer>
        <button class="small quiet" onclick={refresh} disabled={$scanning} data-testid="refresh">{$scanning ? 'Looking…' : 'Look again'}</button>
        <button
          class="small quiet"
          onclick={() => {
            open = false;
            onhelp();
          }}
          data-testid="connection-help">Connection details</button
        >
      </footer>
    </div>
  {/if}
</div>

<style>
  .connection {
    position: relative;
  }

  .pill {
    gap: 10px;
    min-height: 34px;
    padding: 0 10px 0 12px;
    border-color: var(--line);
    border-radius: 999px;
    background: var(--panel);
    color: var(--muted);
    font-weight: 500;
  }

  .pill[data-state='online'] {
    color: var(--text);
  }

  .pill[data-state='lost'] {
    border-color: rgb(240 180 85 / 0.45);
    color: var(--warn);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--faint);
    transition: background-color 200ms ease, box-shadow 200ms ease;
  }

  .dot[data-state='online'] {
    background: var(--good);
    box-shadow: 0 0 0 3px rgb(92 203 138 / 0.18);
  }

  .pill[data-state='lost'] .dot {
    background: var(--warn);
  }

  .chevron {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-linecap: round;
    stroke-linejoin: round;
    stroke-width: 1.6;
    opacity: 0.7;
  }

  .panel {
    position: absolute;
    top: calc(100% + 8px);
    right: 0;
    z-index: 20;
    display: grid;
    gap: var(--space-4);
    width: 360px;
    padding: var(--space-4);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-l);
    background: var(--panel);
    box-shadow: 0 18px 48px -12px rgb(0 0 0 / 0.65);
    transform-origin: top right;
    animation: open 180ms var(--ease-out);
  }

  @keyframes open {
    from {
      opacity: 0;
      transform: scale(0.97) translateY(-4px);
    }
  }

  header {
    display: grid;
    gap: var(--space-1);
  }

  .decks {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .deck {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: 10px var(--space-3);
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    background: var(--sunken);
  }

  .deck.other {
    opacity: 0.7;
  }

  .glyph {
    flex: none;
    width: 22px;
    height: 22px;
    fill: none;
    stroke: var(--accent);
    stroke-linecap: round;
    stroke-width: 1.6;
  }

  .text {
    display: grid;
    flex: 1;
    min-width: 0;
  }

  .name {
    font-weight: 600;
  }

  .detail {
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-s);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .deck button {
    flex: none;
  }

  .empty {
    padding: var(--space-3);
    border: 1px dashed var(--line-strong);
    border-radius: var(--radius-m);
    color: var(--muted);
    font-size: var(--text-s);
  }

  footer {
    display: flex;
    justify-content: space-between;
    margin: 0 calc(-1 * var(--space-2)) calc(-1 * var(--space-2));
  }

  @media (prefers-reduced-motion: reduce) {
    .panel {
      animation: fade 150ms ease;
    }

    @keyframes fade {
      from {
        opacity: 0;
      }
    }
  }
</style>
