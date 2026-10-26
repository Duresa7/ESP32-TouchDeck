<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  let { port = '' }: { port?: string } = $props();
  type Progress = { state: 'Checking' | 'Rebooting' | 'Confirming' } | { state: 'Sending' | 'Recovering'; pct: number } | { state: 'Done'; message: string } | { state: 'Failed'; reason: string };
  let selected = $state<File>();
  let target = $state('');
  let busy = $state(false);
  let progress = $state<Progress>();
  let history = $state<string[]>([]);
  $effect(() => { if (port) target = port; });
  const label = (p: Progress) => p.state === 'Failed' ? `Failed: ${p.reason}` : p.state === 'Done' ? p.message : 'pct' in p ? `${p.state} ${p.pct}%` : p.state;
  function record(p: Progress) { progress = p; history = [...history, label(p)]; }
  async function run(recover: boolean) {
    if (!selected || !target || busy) return;
    busy = true; history = []; progress = undefined;
    try {
      if (selected.size > 6 * 1024 * 1024) throw new Error('Firmware bundle exceeds 6 MB');
      const bytes = Array.from(new Uint8Array(await selected.arrayBuffer()));
      await invoke('update_firmware', { bytes, port: target, recover });
    } catch (error) { record({ state: 'Failed', reason: String(error) }); }
    finally { busy = false; }
  }
  onMount(() => {
    const stop = listen<Progress>('firmware-update', (event) => record(event.payload));
    return () => { stop.then((unlisten) => unlisten()); };
  });
</script>
<main class="update panel">
  <h1>Update firmware</h1>
  <p>Select a TouchDeck firmware bundle, then update your deck. Keep USB connected until the update finishes.</p>
  <label>Firmware bundle <input type="file" accept=".tdfw" onchange={(event) => { selected = event.currentTarget.files?.[0]; }} disabled={busy} data-testid="update-file" /></label>
  <label>Deck port <input bind:value={target} placeholder="COM3 or /dev/ttyACM0" disabled={busy} data-testid="update-port" /></label>
  <div class="buttons">
    <button class="primary" onclick={() => run(false)} disabled={busy || !selected || !target} data-testid="update-start">Update deck</button>
    <button onclick={() => run(true)} disabled={busy || !selected || !target} data-testid="update-recover">Recover deck</button>
  </div>
  <p class="hint">Recover deck restores firmware through the USB bootloader and keeps your saved layouts.</p>
  {#if progress}
    <p role="status" data-testid="update-status">{label(progress)}</p>
    {#if 'pct' in progress}<progress max="100" value={progress.pct}></progress>{/if}
  {/if}
  <ol data-testid="update-history">{#each history as entry}<li>{entry}</li>{/each}</ol>
</main>
<style>
  .update { margin: 24px; padding: 24px; max-width: 720px; }
  label { display: flex; flex-direction: column; gap: 8px; margin: 20px 0; }
  .buttons { display: flex; gap: 12px; }
  ol { max-height: 220px; overflow: auto; color: var(--muted); }
</style>
