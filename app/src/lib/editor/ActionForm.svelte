<script lang="ts" module>
  import type { Action, ActionType, Modifier } from './validate.ts';

  interface Choice {
    usage: number;
    name: string;
  }

  /** HID keyboard usages: F1 to F24, letters, and digits. */
  const KEYS: Choice[] = [
    ...Array.from({ length: 12 }, (_, i) => ({ usage: 58 + i, name: `F${i + 1}` })),
    ...Array.from({ length: 12 }, (_, i) => ({ usage: 104 + i, name: `F${i + 13}` })),
    ...Array.from({ length: 26 }, (_, i) => ({ usage: 4 + i, name: String.fromCharCode(65 + i) })),
    ...Array.from({ length: 9 }, (_, i) => ({ usage: 30 + i, name: String(i + 1) })),
    { usage: 39, name: '0' },
  ];

  /** HID consumer usages for the media keys. */
  const MEDIA: Choice[] = [
    { usage: 205, name: 'Play/Pause' },
    { usage: 181, name: 'Next track' },
    { usage: 182, name: 'Previous track' },
    { usage: 183, name: 'Stop' },
    { usage: 226, name: 'Mute' },
    { usage: 233, name: 'Volume up' },
    { usage: 234, name: 'Volume down' },
  ];

  const SOURCES = ['cliproxyapi'];

  function fresh(type: ActionType, pages: string[]): Action {
    switch (type) {
      case 'key':
        return { type, usage: 58, mods: [] };
      case 'mod':
        return { type, mod: 'ctrl' };
      case 'consumer':
        return { type, usage: 205 };
      case 'page':
        return { type, page: pages[0] ?? '' };
      case 'launch':
        return { type, path: '', args: [], cwd: '' };
      case 'open':
        return { type, target: '' };
      case 'command':
        return { type, program: '', args: [], cwd: '', shell: false };
      case 'widget':
        return { type, source: 'cliproxyapi', field: 'five_hour_pct', account: '' };
      default:
        return { type: 'none' };
    }
  }
</script>

<script lang="ts">
  import { onMount } from 'svelte';
  import { PROXY_FIELDS, sourcesStatus } from '../sources/api.ts';
  import { ACTION_TYPES, MODIFIERS } from './validate.ts';

  interface Props {
    action: Action;
    pages: { id: string; title: string }[];
    /** Commits an action and returns the refusal, or null when the layout took it. */
    onchange: (action: Action) => string | null;
  }

  let { action, pages, onchange }: Props = $props();

  // The form edits a draft, so a field can pass through an invalid value, such as an empty
  // path while typing, without the layout ever holding it.
  let draft = $state<Action>({ type: 'none' });
  let problem = $state<string | null>(null);
  let accountKeys = $state<string[]>([]);

  onMount(() => {
    let active = true;
    const poll = async () => {
      try {
        const status = await sourcesStatus();
        if (active) accountKeys = [...new Set(status.proxy.accounts.map((a) => a.key))];
      } catch { /* No poll data: keep the free-text account input available. */ }
    };
    void poll();
    const timer = setInterval(poll, 1000);
    return () => { active = false; clearInterval(timer); };
  });

  function setSource(source: string) {
    if (draft.type !== 'widget') return;
    if (source === 'cliproxyapi') {
      draft = { ...draft, source, field: PROXY_FIELDS.includes(draft.field) ? draft.field : 'five_hour_pct' };
      problem = onchange($state.snapshot(draft));
    } else {
      set('source', source);
    }
  }

  $effect.pre(() => {
    const next = JSON.parse(JSON.stringify(action)) as Action;
    if (JSON.stringify(next) !== JSON.stringify($state.snapshot(draft)) && problem === null) draft = next;
  });

  function commit() {
    problem = onchange($state.snapshot(draft) as Action);
  }

  function set(field: string, value: unknown) {
    (draft as Record<string, unknown>)[field] = value;
    commit();
  }

  function switchType(type: ActionType) {
    draft = fresh(type, pages.map((page) => page.id));
    commit();
  }

  function toggle(mod: Modifier, on: boolean) {
    if (draft.type !== 'key') return;
    const held = draft.mods;
    set('mods', MODIFIERS.filter((m) => (m === mod ? on : held.includes(m))));
  }

  const lines = (args: string[]) => args.join('\n');
  const split = (text: string) => (text === '' ? [] : text.split('\n'));
  const known = (list: Choice[], usage: number) => list.some((choice) => choice.usage === usage);
</script>

<div class="form" data-testid="action-form" data-type={draft.type}>
  <label>
    <span>Action</span>
    <select value={draft.type} onchange={(event) => switchType(event.currentTarget.value as ActionType)} data-testid="action-type">
      {#each ACTION_TYPES as type (type)}
        <option value={type}>{type}</option>
      {/each}
    </select>
  </label>

  {#if draft.type === 'key'}
    <label>
      <span>Key</span>
      <select value={draft.usage} onchange={(e) => set('usage', Number(e.currentTarget.value))} data-testid="action-key">
        {#if !known(KEYS, draft.usage)}
          <option value={draft.usage}>usage {draft.usage}</option>
        {/if}
        {#each KEYS as key (key.usage)}
          <option value={key.usage}>{key.name}</option>
        {/each}
      </select>
    </label>
    <div class="mods">
      {#each MODIFIERS as mod (mod)}
        <label class="check">
          <input
            type="checkbox"
            checked={draft.mods.includes(mod)}
            onchange={(event) => toggle(mod, event.currentTarget.checked)}
            data-testid="action-mod-{mod}"
          />
          {mod}
        </label>
      {/each}
    </div>
  {:else if draft.type === 'mod'}
    <label>
      <span>Modifier</span>
      <select value={draft.mod} onchange={(e) => set('mod', e.currentTarget.value)} data-testid="action-modifier">
        {#each MODIFIERS as mod (mod)}
          <option value={mod}>{mod}</option>
        {/each}
      </select>
    </label>
  {:else if draft.type === 'consumer'}
    <label>
      <span>Media key</span>
      <select value={draft.usage} onchange={(e) => set('usage', Number(e.currentTarget.value))} data-testid="action-media">
        {#if !known(MEDIA, draft.usage)}
          <option value={draft.usage}>usage {draft.usage}</option>
        {/if}
        {#each MEDIA as key (key.usage)}
          <option value={key.usage}>{key.name}</option>
        {/each}
      </select>
    </label>
  {:else if draft.type === 'page'}
    <label>
      <span>Page</span>
      <select value={draft.page} onchange={(e) => set('page', e.currentTarget.value)} data-testid="action-page">
        {#each pages as page (page.id)}
          <option value={page.id}>{page.title}</option>
        {/each}
      </select>
    </label>
  {:else if draft.type === 'launch'}
    <label><span>Path</span><input value={draft.path} oninput={(e) => set('path', e.currentTarget.value)} data-testid="action-path" spellcheck="false" /></label>
    <label>
      <span>Arguments, one per line</span>
      <textarea
        value={lines(draft.args)}
        oninput={(e) => set('args', split(e.currentTarget.value))}
        rows="2"
        data-testid="action-args"
        spellcheck="false"
      ></textarea>
    </label>
    <label><span>Working folder</span><input value={draft.cwd} oninput={(e) => set('cwd', e.currentTarget.value)} data-testid="action-cwd" spellcheck="false" /></label>
  {:else if draft.type === 'open'}
    <label><span>File, folder, or URL</span><input value={draft.target} oninput={(e) => set('target', e.currentTarget.value)} data-testid="action-target" spellcheck="false" /></label>
  {:else if draft.type === 'command'}
    <label><span>Program</span><input value={draft.program} oninput={(e) => set('program', e.currentTarget.value)} data-testid="action-program" spellcheck="false" /></label>
    <label>
      <span>Arguments, one per line</span>
      <textarea
        value={lines(draft.args)}
        oninput={(e) => set('args', split(e.currentTarget.value))}
        rows="2"
        data-testid="action-args"
        spellcheck="false"
      ></textarea>
    </label>
    <label><span>Working folder</span><input value={draft.cwd} oninput={(e) => set('cwd', e.currentTarget.value)} data-testid="action-cwd" spellcheck="false" /></label>
    <label class="check"><input type="checkbox" checked={draft.shell} onchange={(e) => set('shell', e.currentTarget.checked)} data-testid="action-shell" /> Run in a shell</label>
  {:else if draft.type === 'widget'}
    <label>
      <span>Source</span>
      <select value={draft.source} onchange={(e) => setSource(e.currentTarget.value)} data-testid="action-source">
        {#each SOURCES as source (source)}
          <option value={source}>{source}</option>
        {/each}
      </select>
    </label>
    <label><span>Field</span>
      {#if draft.source === 'cliproxyapi'}
        <select value={draft.field} onchange={(e) => set('field', e.currentTarget.value)} data-testid="action-field">
          {#if !PROXY_FIELDS.includes(draft.field)}<option value={draft.field}>{draft.field} (unknown)</option>{/if}
          {#each PROXY_FIELDS as field (field)}<option value={field}>{field}</option>{/each}
        </select>
      {:else}
        <input value={draft.field} oninput={(e) => set('field', e.currentTarget.value)} data-testid="action-field" spellcheck="false" />
      {/if}
    </label>
    <label><span>Account</span>
      {#if draft.source === 'cliproxyapi' && accountKeys.length}
        <select value={draft.account} onchange={(e) => set('account', e.currentTarget.value)} data-testid="action-account">
          <option value="">All accounts</option>
          {#if draft.account && !accountKeys.includes(draft.account)}<option value={draft.account}>{draft.account} (not in latest poll)</option>{/if}
          {#each accountKeys as account (account)}<option value={account}>{account}</option>{/each}
        </select>
      {:else}
        <input value={draft.account} oninput={(e) => set('account', e.currentTarget.value)} data-testid="action-account" spellcheck="false" />
      {/if}
    </label>
  {/if}

  {#if problem}
    <p class="problem" data-testid="action-error">{problem}</p>
  {/if}
</div>

<style>
  .form {
    display: grid;
    gap: 10px;
  }

  label {
    display: grid;
    gap: 4px;
  }

  label > span {
    color: var(--muted);
    font-size: 12px;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .mods {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
  }

  .problem {
    margin: 0;
    color: var(--bad);
    font-size: 12px;
  }
</style>
