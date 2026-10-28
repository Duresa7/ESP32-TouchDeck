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
      <h2 id="approval-title">Let this button run a command?</h2>
      <p class="from">The button <strong>{current.tile}</strong> on page <strong>{current.page}</strong> wants to run this on your computer. Only allow it if you set it up.</p>
      <dl>
        <dt>Program</dt>
        <dd><code data-testid="approval-program">{current.program}</code></dd>
        <dt>With</dt>
        <dd>
          <ol class="args" data-testid="approval-args">
            {#each current.args as arg, i (i)}
              <li><code data-testid="approval-arg">{arg}</code></li>
            {:else}
              <li class="none">nothing else</li>
            {/each}
          </ol>
        </dd>
        <dt>In folder</dt>
        <dd><code data-testid="approval-cwd">{current.cwd || "Open TouchDeck's folder"}</code></dd>
        <dt>System shell</dt>
        <dd data-testid="approval-shell" data-shell={current.shell}>{current.shell ? 'Yes, it runs through the system shell' : 'No'}</dd>
      </dl>
      {#if queue.length > 1}
        <p class="more">{queue.length - 1} more {queue.length - 1 === 1 ? 'button is' : 'buttons are'} waiting after this one.</p>
      {/if}
      {#if problem}
        <p class="problem">Open TouchDeck couldn't pass on your answer. Press the button on the deck again. ({problem})</p>
      {/if}
      <div class="buttons">
        <button onclick={() => answer('deny')} data-testid="approval-deny">Don't run</button>
        <button onclick={() => answer('always')} data-testid="approval-always" title="Runs this exact command from now on without asking">Always allow</button>
        <button class="primary" onclick={() => answer('once')} data-testid="approval-once">Run once</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: grid;
    place-items: center;
    padding: var(--space-5);
    background: rgb(4 6 10 / 0.6);
    animation: fade 180ms ease;
  }

  .modal {
    display: grid;
    gap: var(--space-4);
    width: min(560px, 100%);
    max-height: 100%;
    overflow: auto;
    padding: var(--space-5);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-l);
    background: var(--panel);
    box-shadow: 0 24px 64px -16px rgb(0 0 0 / 0.7);
    animation: rise 220ms var(--ease-out);
  }

  @keyframes fade {
    from {
      opacity: 0;
    }
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: scale(0.97);
    }
  }

  .from {
    color: var(--muted);
  }

  .from strong {
    color: var(--text);
  }

  dl {
    display: grid;
    grid-template-columns: 110px minmax(0, 1fr);
    gap: var(--space-2) var(--space-3);
    margin: 0;
    padding: var(--space-4);
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    background: var(--sunken);
  }

  dt {
    color: var(--muted);
    font-size: var(--text-s);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  code {
    font-family: var(--mono);
    font-size: var(--text-s);
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
    gap: var(--space-2);
  }

  @media (prefers-reduced-motion: reduce) {
    .modal {
      animation: fade 180ms ease;
    }
  }
</style>
