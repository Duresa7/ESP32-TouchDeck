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
  let status = $state<{ kind: 'info' | 'ok' | 'error'; text: string } | null>(null);
  let rebootOffered = $state(false);
  let busy = $state(false);

  const idleValid = $derived(Number.isInteger(idle) && idle >= IDLE_MIN && idle <= IDLE_MAX);

  $effect(() => {
    if (!online || session === null) return;
    invoke<DeckSettings>('settings_get')
      .then((settings) => {
        idle = settings.idle_timeout_s;
        orientation = settings.orientation;
        status = { kind: 'info', text: 'Read the settings from the deck' };
      })
      .catch((error) => (status = { kind: 'error', text: `Could not read the settings: ${String(error)}` }));
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
        text: rebootOffered ? 'Saved. The orientation applies after the deck reboots.' : 'Saved on the deck.',
      };
    } catch (error) {
      status = { kind: 'error', text: `Save failed: ${String(error)}` };
    } finally {
      busy = false;
    }
  }

  async function reboot() {
    busy = true;
    try {
      await invoke('reboot_deck');
      rebootOffered = false;
      status = { kind: 'info', text: 'The deck is rebooting. Connect again once it is back.' };
    } catch (error) {
      status = { kind: 'error', text: `Reboot failed: ${String(error)}` };
    } finally {
      busy = false;
    }
  }
</script>

<section class="settings" data-testid="settings">
  <form class="card" onsubmit={save}>
    <h2>Deck settings</h2>
    <label>
      <span>Screen off after, in seconds ({IDLE_MIN} to {IDLE_MAX})</span>
      <input type="number" min={IDLE_MIN} max={IDLE_MAX} step="1" bind:value={idle} disabled={!online} data-testid="settings-idle" />
    </label>
    {#if !idleValid}
      <p class="problem" data-testid="settings-idle-error">Enter a whole number of seconds from {IDLE_MIN} to {IDLE_MAX}.</p>
    {/if}
    <label>
      <span>Orientation</span>
      <select bind:value={orientation} disabled={!online} data-testid="settings-orientation">
        <option value="landscape">Landscape</option>
        <option value="portrait">Portrait</option>
      </select>
    </label>
    <div class="row">
      <button class="primary" disabled={!online || !idleValid || busy} data-testid="settings-save">Save</button>
      {#if rebootOffered}
        <button type="button" onclick={reboot} disabled={!online || busy} data-testid="settings-reboot">Reboot deck</button>
      {/if}
    </div>
    <p class="status" data-testid="settings-status" data-kind={status?.kind ?? 'none'}>
      {online ? (status?.text ?? '') : 'Connect a deck to change its settings.'}
    </p>
  </form>
</section>

<style>
  .settings {
    flex: 1;
    min-height: 0;
    padding: 16px;
    overflow-y: auto;
  }

  .card {
    display: grid;
    gap: 14px;
    max-width: 460px;
    padding: 18px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--panel);
  }

  label {
    display: grid;
    gap: 6px;
  }

  label > span {
    color: var(--muted);
    font-size: 12px;
  }

  .row {
    display: flex;
    gap: 8px;
  }

  .problem,
  .status {
    margin: 0;
    font-size: 13px;
  }

  .problem,
  .status[data-kind='error'] {
    color: var(--bad);
  }

  .status {
    color: var(--muted);
  }

  .status[data-kind='ok'] {
    color: var(--online);
  }
</style>
