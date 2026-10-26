<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';

  interface ApprovalRequest {
    id: number;
    page: string;
    tile: string;
    hash: string;
    program: string;
    args: string[];
    cwd: string;
    shell: boolean;
  }

  let queue = $state<ApprovalRequest[]>([]);
  let problem = $state<string | null>(null);
  const current = $derived(queue[0]);

  async function answer(choice: 'once' | 'always' | 'deny') {
    const request = current;
    if (!request) return;
    queue = queue.slice(1);
    problem = null;
    try {
      await invoke('approval_answer', { id: request.id, answer: choice });
    } catch (error) {
      problem = String(error);
    }
  }

  onMount(() => {
    invoke<ApprovalRequest[]>('pending_approvals').then((waiting) => {
      const known = new Set(queue.map((request) => request.id));
      queue = [...queue, ...waiting.filter((request) => !known.has(request.id))];
    });
    const stop = listen<ApprovalRequest>('approval-request', (event) => {
      if (!queue.some((request) => request.id === event.payload.id)) queue = [...queue, event.payload];
    });
    return () => stop.then((unlisten) => unlisten());
  });
</script>

{#if current}
  <div class="backdrop">
    <div class="modal" role="dialog" aria-modal="true" aria-labelledby="approval-title" data-testid="approval-modal" data-id={current.id} data-hash={current.hash}>
      <h2 id="approval-title">Run this command?</h2>
      <p class="from">Tile <code>{current.tile}</code> on page <code>{current.page}</code> asks to run:</p>
      <dl>
        <dt>Program</dt>
        <dd><code data-testid="approval-program">{current.program}</code></dd>
        <dt>Arguments</dt>
        <dd>
          <ol class="args" data-testid="approval-args">
            {#each current.args as arg, i (i)}
              <li><code data-testid="approval-arg">{arg}</code></li>
            {:else}
              <li class="none">none</li>
            {/each}
          </ol>
        </dd>
        <dt>Directory</dt>
        <dd><code data-testid="approval-cwd">{current.cwd || "the app's directory"}</code></dd>
        <dt>Shell</dt>
        <dd data-testid="approval-shell">{current.shell ? 'yes, through the system shell' : 'no'}</dd>
      </dl>
      {#if queue.length > 1}
        <p class="more">{queue.length - 1} more waiting</p>
      {/if}
      {#if problem}
        <p class="problem">{problem}</p>
      {/if}
      <div class="buttons">
        <button onclick={() => answer('deny')} data-testid="approval-deny">Deny</button>
        <button onclick={() => answer('always')} data-testid="approval-always">Always allow</button>
        <button class="primary" onclick={() => answer('once')} data-testid="approval-once">Allow once</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 10;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgb(0 0 0 / 0.55);
  }

  .modal {
    width: min(560px, 100%);
    max-height: 100%;
    overflow: auto;
    padding: 18px 20px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--panel);
  }

  h2 {
    margin: 0 0 8px;
    color: var(--text);
    font-size: 16px;
    letter-spacing: 0;
    text-transform: none;
  }

  .from {
    margin: 0 0 12px;
    color: var(--muted);
  }

  dl {
    display: grid;
    grid-template-columns: 96px minmax(0, 1fr);
    gap: 6px 10px;
    margin: 0;
  }

  dt {
    color: var(--muted);
    font-size: 12px;
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  code {
    font-family: var(--mono);
    font-size: 12.5px;
  }

  .args {
    margin: 0;
    padding-left: 20px;
  }

  .none,
  .more {
    color: var(--muted);
  }

  .problem {
    color: var(--bad);
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
</style>
