<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';

  let { port = '' }: { port?: string } = $props();

  type Progress =
    | { state: 'Checking' | 'Rebooting' | 'Confirming' }
    | { state: 'Sending' | 'Recovering'; pct: number }
    | { state: 'Done'; message: string }
    | { state: 'Failed'; reason: string };

  let selected = $state<File>();
  let target = $state('');
  let busy = $state(false);
  let progress = $state<Progress>();
  let history = $state<{ raw: string; text: string }[]>([]);

  $effect(() => {
    if (port) target = port;
  });

  /** The update's own words for a step, which the log keeps. */
  const raw = (p: Progress) => (p.state === 'Failed' ? `Failed: ${p.reason}` : p.state === 'Done' ? p.message : 'pct' in p ? `${p.state} ${p.pct}%` : p.state);

  /** The same step for a person. */
  function spoken(p: Progress): string {
    switch (p.state) {
      case 'Checking':
        return 'Checking the update file…';
      case 'Sending':
        return `Sending the new software to your deck… ${p.pct}%`;
      case 'Recovering':
        return `Repairing your deck… ${p.pct}%`;
      case 'Rebooting':
        return 'Restarting your deck…';
      case 'Confirming':
        return 'Making sure the new software started…';
      case 'Done':
        if (p.message === 'Firmware updated') return 'Done. Your deck is running the new software.';
        if (p.message === 'Already on this build') return 'Your deck already has this version, so nothing changed.';
        return `Done. ${p.message}.`;
      case 'Failed':
        if (p.reason.includes('rolled back')) return "The new software didn't start, so your deck went back to its old version. It keeps working. Try another update file, or choose Repair.";
        if (p.reason.includes('confirm timeout')) return "Your deck didn't come back after restarting. Unplug it and plug it back in. If its buttons don't return, choose Repair.";
        return "The update didn't finish, and your deck keeps its old software. Check the file and the cable, then try again.";
    }
  }

  function record(p: Progress) {
    progress = p;
    history = [...history, { raw: raw(p), text: spoken(p) }];
  }

  async function run(recover: boolean) {
    if (!selected || !target || busy) return;
    busy = true;
    history = [];
    progress = undefined;
    try {
      if (selected.size > 6 * 1024 * 1024) throw new Error('Firmware bundle exceeds 6 MB');
      const bytes = Array.from(new Uint8Array(await selected.arrayBuffer()));
      await invoke('update_firmware', { bytes, port: target, recover });
    } catch (error) {
      record({ state: 'Failed', reason: String(error) });
    } finally {
      busy = false;
    }
  }

  onMount(() => {
    const stop = listen<Progress>('firmware-update', (event) => record(event.payload));
    return () => {
      stop.then((unlisten) => unlisten());
    };
  });
</script>

<div class="update" data-testid="deck-software">
  <header class="intro">
    <h1>Deck software</h1>
    <p class="hint">Install a new version of the software that runs on your deck. Your buttons and settings stay on it.</p>
  </header>

  <ol class="steps">
    <li>
      <span class="name">Choose the update file</span>
      <span class="hint">It ends in .tdfw and comes with each Open TouchDeck release.</span>
      <label class="button small file">
        {selected ? selected.name : 'Choose file…'}
        <input type="file" accept=".tdfw" onchange={(event) => (selected = event.currentTarget.files?.[0])} disabled={busy} data-testid="update-file" />
      </label>
    </li>
    <li>
      <span class="name">Keep the deck plugged in, then update</span>
      <span class="hint">It takes about a minute. The deck restarts once on its own.</span>
      <div class="buttons">
        <button class="primary" onclick={() => run(false)} disabled={busy || !selected || !target} data-testid="update-start">Update deck</button>
      </div>
    </li>
  </ol>

  {#if progress}
    <div class="notice" data-tone={progress.state === 'Failed' ? 'bad' : progress.state === 'Done' ? undefined : 'info'}>
      <div class="progress">
        <p role="status" data-testid="update-status" data-raw={raw(progress)}>{spoken(progress)}</p>
        {#if progress.state === 'Failed'}<details class="more"><summary>Details</summary><div class="body">{progress.reason}</div></details>{/if}
        {#if 'pct' in progress}<progress max="100" value={progress.pct}></progress>{/if}
      </div>
    </div>
  {/if}

  <details class="more">
    <summary>Update log</summary>
    <ol class="history" data-testid="update-history">
      {#each history as entry, i (i)}<li data-raw={entry.raw}>{entry.raw}</li>{/each}
    </ol>
  </details>

  <section class="section">
    <header>
      <h3>Repair the deck</h3>
      <p class="hint">If an update stopped partway or the deck won't start, Repair puts the software back through the deck's USB loader. Your buttons stay.</p>
    </header>
    <div class="repair">
      <label class="field port">
        <span class="name">USB port</span>
        <input bind:value={target} placeholder="COM3" disabled={busy} data-testid="update-port" />
      </label>
      <button onclick={() => run(true)} disabled={busy || !selected || !target} data-testid="update-recover">Repair deck</button>
    </div>
    <p class="hint">Open TouchDeck fills in the port of the connected deck. Choose the update file above first.</p>
  </section>
</div>

<style>
  .update {
    display: grid;
    gap: var(--space-5);
  }

  .intro {
    display: grid;
    gap: var(--space-2);
  }

  .steps {
    display: grid;
    gap: var(--space-4);
    margin: 0;
    padding-left: 1.4em;
  }

  .steps li {
    display: grid;
    justify-items: start;
    gap: var(--space-1);
  }

  .steps li::marker {
    color: var(--muted);
    font-family: var(--font-display);
  }

  .steps .name {
    font-weight: 600;
  }

  .file {
    margin-top: var(--space-2);
  }

  .file input {
    display: none;
  }

  .buttons {
    margin-top: var(--space-2);
  }

  .progress {
    display: grid;
    flex: 1;
    gap: var(--space-2);
  }

  progress {
    width: 100%;
    accent-color: var(--accent);
  }

  .history {
    max-height: 220px;
    margin: var(--space-2) 0 0;
    overflow: auto;
    color: var(--muted);
    font-family: var(--mono);
    font-size: var(--text-xs);
  }

  .repair {
    display: flex;
    align-items: flex-end;
    gap: var(--space-3);
  }

  .port {
    max-width: 200px;
  }
</style>
