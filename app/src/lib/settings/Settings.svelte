<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import type { Orientation } from '../editor/validate.ts';

  interface Props {
    online: boolean;
    session: number | null;
  }

  interface DeckSettings {
    idle_timeout_s: number;
    orientation: Orientation;
  }

  const IDLE_MIN = 1;
  const IDLE_MAX = 86400;

  let { online, session }: Props = $props();

  let idle = $state<number>(600);
  let orientation = $state<Orientation>('landscape');
  let status = $state<{ kind: 'info' | 'ok' | 'error'; text: string; detail?: string } | null>(null);
  let rebootOffered = $state(false);
  let busy = $state(false);

  const idleValid = $derived(Number.isInteger(idle) && idle >= IDLE_MIN && idle <= IDLE_MAX);

  /** A number of seconds as a person says it. */
  function spoken(seconds: number): string {
    if (!Number.isFinite(seconds) || seconds <= 0) return '';
    const parts: string[] = [];
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = seconds % 60;
    if (h) parts.push(`${h} ${h === 1 ? 'hour' : 'hours'}`);
    if (m) parts.push(`${m} ${m === 1 ? 'minute' : 'minutes'}`);
    if (s) parts.push(`${s} ${s === 1 ? 'second' : 'seconds'}`);
    return parts.join(' ');
  }

  $effect(() => {
    if (!online || session === null) return;
    invoke<DeckSettings>('settings_get')
      .then((settings) => {
        idle = settings.idle_timeout_s;
        orientation = settings.orientation;
        status = { kind: 'info', text: "Showing your deck's current settings." };
      })
      .catch((error) => (status = { kind: 'error', text: "Couldn't read your deck's settings. Reconnect it, then try again.", detail: String(error) }));
  });

  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (!idleValid) return;
    busy = true;
    try {
      const reply = await invoke<DeckSettings & { reboot_required?: boolean }>('settings_put', {
        settings: { idle_timeout_s: idle, orientation },
      });
      idle = reply.idle_timeout_s;
      orientation = reply.orientation;
      rebootOffered = reply.reboot_required === true;
      status = {
        kind: 'ok',
        text: rebootOffered ? 'Saved. Restart the deck to turn its screen.' : 'Saved on your deck.',
      };
    } catch (error) {
      status = { kind: 'error', text: "Couldn't save to your deck. Check that it's connected, then try again.", detail: String(error) };
    } finally {
      busy = false;
    }
  }

  async function reboot() {
    busy = true;
    try {
      await invoke('reboot_deck');
      rebootOffered = false;
      status = { kind: 'info', text: 'Your deck is restarting. Connect again once its buttons are back.' };
    } catch (error) {
      status = { kind: 'error', text: "Couldn't restart your deck. Unplug it and plug it back in instead.", detail: String(error) };
    } finally {
      busy = false;
    }
  }
</script>

<form class="deck-settings" onsubmit={save} data-testid="settings">
  <header class="intro">
    <h1>Your deck</h1>
    <p class="hint">These settings are saved on the deck itself, so they stay with it on any computer.</p>
  </header>

  {#if !online}
    <div class="notice" data-tone="info"><p>Connect your deck to change its settings.</p></div>
  {/if}

  <div class="section">
    <header>
      <h3>Screen</h3>
    </header>
    <label class="field narrow">
      <span class="name">Turn the screen off after</span>
      <div class="with-unit">
        <input type="number" min={IDLE_MIN} max={IDLE_MAX} step="1" bind:value={idle} disabled={!online} data-testid="settings-idle" />
        <span>seconds</span>
      </div>
      <span class="hint">{idleValid ? `That's ${spoken(idle)} without a touch. Touch the screen to wake it.` : ''}</span>
    </label>
    {#if !idleValid}
      <p class="problem" data-testid="settings-idle-error">Enter a whole number of seconds from {IDLE_MIN} to {IDLE_MAX}, which is one day.</p>
    {/if}
  </div>

  <div class="section">
    <header>
      <h3>Which way it stands</h3>
      <p class="hint">Landscape is wide, portrait is tall. Each way keeps its own buttons, and the deck restarts to turn.</p>
    </header>
    <label class="field narrow">
      <span class="name visually-hidden">Orientation</span>
      <select bind:value={orientation} disabled={!online} data-testid="settings-orientation">
        <option value="landscape">Landscape, wide</option>
        <option value="portrait">Portrait, tall</option>
      </select>
    </label>
  </div>

  <div class="actions">
    <button class="primary" disabled={!online || !idleValid || busy} data-testid="settings-save">Save to deck</button>
    {#if rebootOffered}
      <button type="button" onclick={reboot} disabled={!online || busy} data-testid="settings-reboot">Restart deck now</button>
    {/if}
  </div>
  <p class="status" data-testid="settings-status" data-kind={status?.kind ?? 'none'} role="status">
    {online ? (status?.text ?? '') : ''}
  </p>
  {#if online && status?.detail}
    <details class="more"><summary>Details</summary><div class="body">{status.detail}</div></details>
  {/if}
</form>

<style>
  .deck-settings {
    display: grid;
    gap: var(--space-5);
  }

  .intro {
    display: grid;
    gap: var(--space-2);
  }

  .narrow {
    max-width: 320px;
  }

  .with-unit {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    color: var(--muted);
  }

  .with-unit input {
    max-width: 140px;
  }

  .actions {
    display: flex;
    gap: var(--space-2);
  }

  .status {
    min-height: 1.5em;
    color: var(--muted);
  }

  .status[data-kind='error'] {
    color: var(--bad);
  }

  .status[data-kind='ok'] {
    color: var(--good);
  }
</style>
