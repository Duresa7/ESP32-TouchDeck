<script lang="ts">
  import { getVersion } from '@tauri-apps/api/app';
  import { onMount } from 'svelte';
  import { appUpdater, appUpdateState } from './api.ts';
  import { parseUpdateError } from './controller.ts';

  let { online }: { online: boolean } = $props();
  let version = $state('');
  let installError = $state('');
  const busy = $derived(['checking', 'downloading', 'installing'].includes($appUpdateState.kind));
  onMount(() => { getVersion().then((value) => version = value).catch(() => version = 'unknown'); });

  async function install() {
    installError = '';
    try { await appUpdater.install(); }
    catch (error) { installError = parseUpdateError(error).message; }
  }
</script>

<section class="app-updates" aria-labelledby="app-updates-title" data-testid="app-updates">
  <h2 id="app-updates-title">App updates</h2>
  <p>TouchDeck desktop app {version}. Deck firmware is updated separately in the Update tab.</p>
  <button onclick={() => { installError = ''; appUpdater.check(); }} disabled={busy} data-testid="app-update-check">
    {$appUpdateState.kind === 'checking' ? 'Checking for updates…' : 'Check for updates'}
  </button>
  {#if $appUpdateState.kind === 'current'}
    <p role="status">You’re on the latest app version.</p>
  {:else if $appUpdateState.kind === 'error'}
    <p class="problem" role="alert">{$appUpdateState.error.message}</p>
    {#if $appUpdateState.error.kind === 'network'}
      <p>Offline or a release may be unavailable. You can keep using this version and try again later.</p>
    {/if}
  {:else if 'update' in $appUpdateState}
    <h3>Version {$appUpdateState.update.version}</h3>
    <pre class="notes">{$appUpdateState.update.notes || 'No release notes provided.'}</pre>
    {#if $appUpdateState.kind === 'available'}
      <button onclick={() => appUpdater.download()} data-testid="app-update-download">Download update</button>
      <p>Downloads and verifies the signed installer. TouchDeck stays open.</p>
    {:else if $appUpdateState.kind === 'downloading'}
      <p role="status">Downloading update: {Math.round($appUpdateState.progress.downloaded / 1024)} KB
        {#if $appUpdateState.progress.total !== null} of {Math.round($appUpdateState.progress.total / 1024)} KB{/if}</p>
      <progress max={$appUpdateState.progress.total ?? undefined} value={$appUpdateState.progress.total === null ? undefined : $appUpdateState.progress.downloaded}></progress>
      <p>The signature is verified before installation is offered.</p>
    {:else if $appUpdateState.kind === 'ready'}
      <p role="status">Download verified. Save or export any unsaved layout edits before installing.</p>
      <p>Installation closes TouchDeck. Open it again when the installer finishes; your saved settings stay in place.</p>
      {#if online}<p>Disconnect the deck and wait for transfers to finish before installing.</p>{/if}
      <button onclick={install} disabled={online} data-testid="app-update-install">Install and close TouchDeck</button>
    {:else if $appUpdateState.kind === 'installing'}
      <p role="status">Starting the installer. TouchDeck will close; installation progress appears in the installer.</p>
    {/if}
  {/if}
  {#if installError}<p class="problem" role="alert">{installError}</p>{/if}
</section>

<style>
  .app-updates { margin-top: 16px; display: grid; gap: 12px; max-width: 620px; padding: 18px; border: 1px solid var(--line); border-radius: 12px; background: var(--panel); }
  p, h2, h3 { margin: 0; }
  p { color: var(--muted); }
  .notes { white-space: pre-wrap; overflow-wrap: anywhere; max-height: 240px; overflow: auto; font: inherit; }
  .problem { color: var(--bad); }
  button { justify-self: start; }
</style>
