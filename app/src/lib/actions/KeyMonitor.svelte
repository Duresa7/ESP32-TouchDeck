<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';

  interface Mirrored {
    seq: number;
    report: string;
    outcome: 'typed' | 'duplicate' | 'off' | 'unknown';
    gap: [number, number] | null;
    errors: string[];
    arrived_ms: number;
  }

  interface KeyDomEvent {
    type: 'down' | 'up';
    key: string;
    code: string;
    repeat: boolean;
    at_ms: number;
  }

  const REPORTS_KEPT = 50;
  const EVENTS_KEPT = 2000;

  let reports = $state<Mirrored[]>([]);
  let events = $state<KeyDomEvent[]>([]);
  let typed = $state('');
  let front = $state<boolean | null>(null);

  // F13 and the media keys type no text, so the monitor writes each key's name instead and
  // keeps every keydown and keyup apart from the mirrored reports.
  function record(event: KeyboardEvent) {
    event.preventDefault();
    const type = event.type === 'keydown' ? 'down' : 'up';
    const at_ms = performance.timeOrigin + performance.now();
    events = [...events.slice(-(EVENTS_KEPT - 1)), { type, key: event.key, code: event.code, repeat: event.repeat, at_ms }];
    if (type === 'down' && !event.repeat) typed = typed ? `${typed} ${event.key}` : event.key;
  }

  async function bringToFront() {
    // Refocusing the webview fires this handler again, so only ask when the window lacks focus.
    if (document.hasFocus()) {
      front = true;
      return;
    }
    front = null;
    front = await invoke<boolean>('focus_window');
  }

  function clear() {
    reports = [];
    events = [];
    typed = '';
  }

  onMount(() => {
    const stop = listen<Mirrored>('key-mirror', (event) => {
      reports = [...reports.slice(-(REPORTS_KEPT - 1)), event.payload];
    });
    return () => stop.then((unlisten) => unlisten());
  });
</script>

<div class="monitor">
  <div class="row">
    <textarea
      readonly
      value={typed}
      onkeydown={record}
      onkeyup={record}
      onfocus={bringToFront}
      placeholder="Click here, then press a button on your deck"
      aria-label="Key tester"
      data-testid="key-monitor"
      data-front={front === null ? null : String(front)}
    ></textarea>
    <button class="small" onclick={clear} data-testid="key-monitor-clear">Clear</button>
  </div>
  <div class="lists">
    <div>
      <h4>What the deck sent</h4>
      <ol data-testid="key-reports">
        {#each reports as report, i (i)}
          <li data-testid="key-report" data-outcome={report.outcome} data-seq={report.seq} data-arrived={report.arrived_ms}>
            {report.report}{report.gap ? ` (missed ${report.gap[0]} to ${report.gap[1]}, so every key was let go)` : ''}{report.outcome === 'typed' ? '' : ` (${report.outcome})`}
          </li>
        {/each}
      </ol>
    </div>
    <div>
      <h4>What this computer typed</h4>
      <ol data-testid="key-events">
        {#each events as event, i (i)}
          <li data-testid="key-event" data-type={event.type} data-key={event.key} data-at={event.at_ms}>{event.type} {event.key}</li>
        {/each}
      </ol>
    </div>
  </div>
</div>

<style>
  .monitor {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
  }

  .row {
    display: flex;
    gap: 8px;
    align-items: stretch;
  }

  textarea {
    flex: 1;
    min-width: 0;
    height: 64px;
    padding: 8px 10px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-s);
    background: var(--sunken);
    color: var(--text);
    font-family: var(--mono);
    font-size: var(--text-s);
    resize: none;
  }

  textarea:focus {
    border-color: var(--accent);
    outline: none;
  }

  .lists {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 12px;
    min-height: 0;
  }

  h4 {
    margin: 0 0 6px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 500;
  }

  ol {
    height: 140px;
    margin: 0;
    padding: 6px 8px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    background: var(--sunken);
    font-family: var(--mono);
    font-size: var(--text-xs);
    list-style: none;
  }
</style>
