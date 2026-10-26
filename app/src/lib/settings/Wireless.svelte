<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';

  interface WifiState {
    configured: boolean;
    ssid: string;
    state: 'off' | 'connecting' | 'connected' | 'failed';
    ip: string | null;
    rssi: number | null;
    reason: string | null;
    host: string;
  }
  interface AccessPoint { ssid: string; rssi: number; auth: string }

  let { online, session }: { online: boolean; session: number | null } = $props();
  let wifi = $state<WifiState | null>(null);
  let aps = $state<AccessPoint[]>([]);
  let ssid = $state('');
  let password = $state('');
  let busy = $state(false);
  let problem = $state('');
  const bytes = (text: string) => new TextEncoder().encode(text).length;
  const valid = $derived(bytes(ssid) >= 1 && bytes(ssid) <= 32 && (password === '' || (bytes(password) >= 8 && bytes(password) <= 63)));

  $effect(() => {
    const id = session;
    wifi = null;
    aps = [];
    ssid = '';
    password = '';
    problem = '';
    if (!online || id === null) return;
    let active = true;
    let polling = false;
    async function refresh() {
      if (polling) return;
      polling = true;
      try {
        const nextWifi = await invoke<WifiState>('wifi_state');
        if (active) { wifi = nextWifi; }
      } catch {
        if (active) problem = 'Could not read wireless status. Check that the deck firmware supports wireless setup.';
      } finally { polling = false; }
    }
    void refresh();
    const timer = setInterval(refresh, 1000);
    return () => { active = false; clearInterval(timer); };
  });

  async function scan() {
    busy = true;
    problem = '';
    try { aps = (await invoke<{ aps: AccessPoint[] }>('wifi_scan')).aps; }
    catch { problem = 'Could not scan for networks. Try again.'; }
    finally { busy = false; }
  }

  async function join(event: SubmitEvent) {
    event.preventDefault();
    if (!valid) return;
    busy = true;
    problem = '';
    const secret = password;
    password = '';
    try { wifi = await invoke<WifiState>('wifi_set', { ssid, password: secret }); }
    catch { problem = 'Could not save the network. Check the network name and password, then try again.'; }
    finally { busy = false; }
  }

  async function forgetWifi() {
    busy = true;
    problem = '';
    password = '';
    try { wifi = await invoke<WifiState>('wifi_forget'); }
    catch { problem = 'Could not forget the Wi-Fi network.'; }
    finally { busy = false; }
  }
</script>

<section class="wireless" data-testid="wireless">
  <h2>Wireless</h2>
  {#if !online}
    <p>Connect the deck over USB to set up its radios.</p>
  {:else}
    <h3>Wi-Fi</h3>
    <p data-testid="wifi-state" data-state={wifi?.state ?? 'loading'}>
      {wifi ? `${wifi.state}${wifi.configured ? ` · ${wifi.ssid}` : ''}` : 'Reading Wi-Fi status…'}
      {#if wifi?.ip} · {wifi.ip}{/if}
      {#if wifi?.rssi !== null && wifi?.rssi !== undefined} · {wifi.rssi} dBm{/if}
      {#if wifi?.reason} · {wifi.reason}{/if}
    </p>
    {#if wifi?.host}<p class="muted">{wifi.host}.local</p>{/if}
    <form onsubmit={join}>
      <button type="button" onclick={scan} disabled={busy} data-testid="wifi-scan">Scan networks</button>
      {#if aps.length}
        <label>Nearby networks
          <select data-testid="wifi-networks" value="" onchange={(event) => { ssid = event.currentTarget.value; password = ''; }}>
            <option value="" disabled>Choose a network</option>
            {#each aps as ap, i (i)}<option value={ap.ssid}>{ap.ssid} · {ap.rssi} dBm · {ap.auth}</option>{/each}
          </select>
        </label>
      {/if}
      <label>Network name<input bind:value={ssid} autocomplete="off" data-testid="wifi-ssid" disabled={busy} /></label>
      <label>Password<input type="password" bind:value={password} autocomplete="new-password" data-testid="wifi-password" disabled={busy} /></label>
      <p class="muted">Leave the password empty for an open network. Otherwise use 8 to 63 bytes.</p>
      <div class="row">
        <button class="primary" disabled={!valid || busy} data-testid="wifi-join">Join network</button>
        <button type="button" onclick={forgetWifi} disabled={busy || !wifi?.configured} data-testid="wifi-forget">Forget network</button>
      </div>
    </form>
    {#if problem}<p class="problem" role="alert" data-testid="wireless-error">{problem}</p>{/if}
  {/if}
</section>

<style>
  .wireless { display: grid; gap: 12px; max-width: 620px; margin-top: 16px; padding: 18px; border: 1px solid var(--line); border-radius: 12px; background: var(--panel); }
  form, label { display: grid; gap: 8px; }
  .row { display: flex; align-items: center; gap: 8px; }
  p, h2, h3 { margin: 0; }
  .muted { color: var(--muted); font-size: 12px; }
  .problem { color: var(--bad); }
</style>
