<script lang="ts">
  import { editor } from './layoutStore.ts';

  interface Props {
    online: boolean;
    busy: boolean;
    onopen: () => void;
    onsave: () => void;
  }

  let { online, busy, onopen, onsave }: Props = $props();

  let open = $state(false);
  let root = $state<HTMLDivElement>();

  function run(action: () => void) {
    open = false;
    action();
  }

  function outside(event: PointerEvent) {
    if (open && root && !root.contains(event.target as Node)) open = false;
  }

  function keys(event: KeyboardEvent) {
    if (!open) return;
    if (event.key === 'Escape') {
      open = false;
      root?.querySelector<HTMLButtonElement>('.toggle')?.focus();
      return;
    }
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
    const items = [...(root?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? [])];
    if (!items.length) return;
    event.preventDefault();
    const at = items.indexOf(document.activeElement as HTMLButtonElement);
    const next = event.key === 'ArrowDown' ? (at + 1) % items.length : (at - 1 + items.length) % items.length;
    items[next].focus();
  }
</script>

<svelte:window onpointerdown={outside} onkeydown={keys} />

<div class="menu" bind:this={root}>
  <button class="toggle" onclick={() => (open = !open)} aria-haspopup="menu" aria-expanded={open} data-testid="layout-menu">
    Layout
    <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 6l4 4 4-4" /></svg>
  </button>
  {#if open}
    <div class="list" role="menu" aria-label="Layout">
      <button role="menuitem" onclick={() => run(() => editor.pull())} disabled={!online || busy} data-testid="pull">
        <strong>{busy ? 'Working…' : 'Load from deck'}</strong>
        <span>{online ? 'Replace these buttons with the ones on your deck' : 'Connect your deck first'}</span>
      </button>
      <button role="menuitem" onclick={() => run(onopen)} data-testid="import">
        <strong>Open a file…</strong>
        <span>A layout you saved before</span>
      </button>
      <button role="menuitem" onclick={() => run(onsave)} data-testid="export">
        <strong>Save to a file…</strong>
        <span>Keep a copy or share it</span>
      </button>
      <hr />
      <button role="menuitem" onclick={() => run(() => editor.startStarter())} data-testid="menu-starter">
        <strong>Start with examples</strong>
        <span>Media controls and shortcuts</span>
      </button>
      <button role="menuitem" onclick={() => run(() => editor.startNew())} data-testid="menu-new">
        <strong>Start blank</strong>
        <span>One empty page</span>
      </button>
    </div>
  {/if}
</div>

<style>
  .menu {
    position: relative;
  }

  .toggle svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-linecap: round;
    stroke-linejoin: round;
    stroke-width: 1.6;
  }

  .list {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 20;
    display: grid;
    width: 280px;
    padding: 6px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-m);
    background: var(--panel);
    box-shadow: 0 18px 48px -12px rgb(0 0 0 / 0.65);
    transform-origin: top right;
    animation: open 160ms var(--ease-out);
  }

  @keyframes open {
    from {
      opacity: 0;
      transform: scale(0.97);
    }
  }

  [role='menuitem'] {
    display: grid;
    justify-items: start;
    gap: 2px;
    min-height: 0;
    padding: 8px 10px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    text-align: left;
  }

  [role='menuitem'] strong {
    font-weight: 500;
  }

  [role='menuitem'] span {
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 400;
  }

  @media (hover: hover) and (pointer: fine) {
    [role='menuitem']:hover:not(:disabled) {
      border: 0;
      background: var(--raised);
    }
  }

  [role='menuitem']:active:not(:disabled) {
    transform: none;
    background: var(--raised);
  }

  hr {
    width: 100%;
    margin: 4px 0;
    border: 0;
    border-top: 1px solid var(--line);
  }

  @media (prefers-reduced-motion: reduce) {
    .list {
      animation: none;
    }
  }
</style>
