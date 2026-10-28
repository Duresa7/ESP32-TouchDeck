<script lang="ts" module>
  import type { Action, ActionType, Modifier } from './validate.ts';

  interface Choice {
    usage: number;
    name: string;
  }

  /** HID keyboard usages: F1 to F24, letters, and digits. */
  const KEYS: Choice[] = [
    ...Array.from({ length: 26 }, (_, i) => ({ usage: 4 + i, name: String.fromCharCode(65 + i) })),
    ...Array.from({ length: 9 }, (_, i) => ({ usage: 30 + i, name: String(i + 1) })),
    { usage: 39, name: '0' },
    ...Array.from({ length: 12 }, (_, i) => ({ usage: 58 + i, name: `F${i + 1}` })),
    ...Array.from({ length: 12 }, (_, i) => ({ usage: 104 + i, name: `F${i + 13}` })),
  ];

  /** HID consumer usages for the media keys. */
  const MEDIA: Choice[] = [
    { usage: 205, name: 'Play or pause' },
    { usage: 181, name: 'Next track' },
    { usage: 182, name: 'Previous track' },
    { usage: 183, name: 'Stop' },
    { usage: 226, name: 'Mute' },
    { usage: 233, name: 'Volume up' },
    { usage: 234, name: 'Volume down' },
  ];

  const MOD_NAMES: Record<Modifier, string> = { ctrl: 'Ctrl', shift: 'Shift', alt: 'Alt', win: 'Windows key' };

  /** What each action does, grouped by where it happens. */
  const GROUPS: { name: string; types: { type: ActionType; name: string }[] }[] = [
    {
      name: 'On this computer, like a keyboard',
      types: [
        { type: 'key', name: 'Press a key or shortcut' },
        { type: 'consumer', name: 'Control music and volume' },
        { type: 'mod', name: 'Hold Ctrl, Shift, Alt, or Windows' },
      ],
    },
    {
      name: 'On this computer, through Open TouchDeck',
      types: [
        { type: 'launch', name: 'Open an app' },
        { type: 'open', name: 'Open a file, folder, or website' },
        { type: 'command', name: 'Run a command' },
      ],
    },
    {
      name: 'On the deck',
      types: [
        { type: 'page', name: 'Go to another page' },
        { type: 'widget', name: 'Show AI usage' },
        { type: 'none', name: 'Nothing' },
      ],
    },
  ];

  /** The numbers an AI usage button can show. */
  const FIELD_NAMES: Record<string, string> = {
    five_hour_pct: '5-hour limit used',
    seven_day_pct: '7-day limit used',
    five_hour_reset: 'Time until the 5-hour limit resets',
    seven_day_reset: 'Time until the 7-day limit resets',
    plan: 'Plan name',
    status: 'Account status',
    success: 'Requests that worked',
    failed: 'Requests that failed',
    recent_10m: 'Requests in the last 10 minutes',
  };

  const SOURCES = [{ id: 'cliproxyapi', name: 'Your CLIProxyAPI hub' }];

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
  import { friendlyRefusal } from './messages.ts';
  import { MODIFIERS } from './validate.ts';

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
      } catch {
        /* No poll data: keep the free-text account input available. */
      }
    };
    void poll();
    const timer = setInterval(poll, 1000);
    return () => {
      active = false;
      clearInterval(timer);
    };
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
  function spell(action: Action): string {
    if (action.type !== 'key') return '';
    const key = KEYS.find((k) => k.usage === action.usage)?.name ?? `key ${action.usage}`;
    return [...action.mods.map((m) => MOD_NAMES[m].replace(' key', '')), key].join(' + ');
  }

  const shortcut = $derived(spell(draft));
</script>

<div class="form" data-testid="action-form" data-type={draft.type}>
  <label class="field">
    <span class="name visually-hidden">What the button does</span>
    <select value={draft.type} onchange={(event) => switchType(event.currentTarget.value as ActionType)} data-testid="action-type">
      {#each GROUPS as group (group.name)}
        <optgroup label={group.name}>
          {#each group.types as option (option.type)}
            <option value={option.type}>{option.name}</option>
          {/each}
        </optgroup>
      {/each}
    </select>
  </label>

  {#if draft.type === 'key'}
    <label class="field">
      <span class="name">Key</span>
      <select value={draft.usage} onchange={(e) => set('usage', Number(e.currentTarget.value))} data-testid="action-key">
        {#if !known(KEYS, draft.usage)}
          <option value={draft.usage}>Key code {draft.usage}</option>
        {/if}
        {#each KEYS as key (key.usage)}
          <option value={key.usage}>{key.name}</option>
        {/each}
      </select>
    </label>
    <fieldset class="field">
      <legend class="name">Hold down with it</legend>
      <div class="mods">
        {#each MODIFIERS as mod (mod)}
          <label class="check">
            <input
              type="checkbox"
              checked={draft.mods.includes(mod)}
              onchange={(event) => toggle(mod, event.currentTarget.checked)}
              data-testid="action-mod-{mod}"
            />
            {MOD_NAMES[mod]}
          </label>
        {/each}
      </div>
    </fieldset>
    <p class="hint">Presses <strong class="keys">{shortcut}</strong> on this computer.</p>
  {:else if draft.type === 'mod'}
    <label class="field">
      <span class="name">Key</span>
      <select value={draft.mod} onchange={(e) => set('mod', e.currentTarget.value)} data-testid="action-modifier">
        {#each MODIFIERS as mod (mod)}
          <option value={mod}>{MOD_NAMES[mod]}</option>
        {/each}
      </select>
    </label>
    <p class="hint">Tap it once and it's held for your next key. Tap it twice quickly and it stays on until you tap it again.</p>
  {:else if draft.type === 'consumer'}
    <label class="field">
      <span class="name">Control</span>
      <select value={draft.usage} onchange={(e) => set('usage', Number(e.currentTarget.value))} data-testid="action-media">
        {#if !known(MEDIA, draft.usage)}
          <option value={draft.usage}>Media code {draft.usage}</option>
        {/if}
        {#each MEDIA as key (key.usage)}
          <option value={key.usage}>{key.name}</option>
        {/each}
      </select>
    </label>
  {:else if draft.type === 'page'}
    <label class="field">
      <span class="name">Page</span>
      <select value={draft.page} onchange={(e) => set('page', e.currentTarget.value)} data-testid="action-page">
        {#each pages as page (page.id)}
          <option value={page.id}>{page.title}</option>
        {/each}
      </select>
    </label>
  {:else if draft.type === 'launch'}
    <label class="field">
      <span class="name">App</span>
      <input value={draft.path} oninput={(e) => set('path', e.currentTarget.value)} placeholder="notepad.exe, or the full path to an app" data-testid="action-path" spellcheck="false" />
    </label>
    <details class="more">
      <summary>More options</summary>
      <div class="extra">
        <label class="field">
          <span class="name">Start it with, one per line</span>
          <textarea value={lines(draft.args)} oninput={(e) => set('args', split(e.currentTarget.value))} rows="2" data-testid="action-args" spellcheck="false"></textarea>
        </label>
        <label class="field">
          <span class="name">Start it in this folder</span>
          <input value={draft.cwd} oninput={(e) => set('cwd', e.currentTarget.value)} data-testid="action-cwd" spellcheck="false" />
        </label>
      </div>
    </details>
  {:else if draft.type === 'open'}
    <label class="field">
      <span class="name">File, folder, or web address</span>
      <input value={draft.target} oninput={(e) => set('target', e.currentTarget.value)} placeholder="https://example.com" data-testid="action-target" spellcheck="false" />
    </label>
    <p class="hint">Opens it the way double-clicking it would.</p>
  {:else if draft.type === 'command'}
    <label class="field">
      <span class="name">Program</span>
      <input value={draft.program} oninput={(e) => set('program', e.currentTarget.value)} data-testid="action-program" spellcheck="false" />
    </label>
    <label class="field">
      <span class="name">Arguments, one per line</span>
      <textarea value={lines(draft.args)} oninput={(e) => set('args', split(e.currentTarget.value))} rows="2" data-testid="action-args" spellcheck="false"></textarea>
    </label>
    <label class="field">
      <span class="name">Run it in this folder</span>
      <input value={draft.cwd} oninput={(e) => set('cwd', e.currentTarget.value)} placeholder="Open TouchDeck's folder" data-testid="action-cwd" spellcheck="false" />
    </label>
    <label class="check"><input type="checkbox" checked={draft.shell} onchange={(e) => set('shell', e.currentTarget.checked)} data-testid="action-shell" /> Run it through the system shell</label>
    <p class="hint">Open TouchDeck shows you the exact command and asks before it runs.</p>
  {:else if draft.type === 'widget'}
    <label class="field">
      <span class="name">From</span>
      <select value={draft.source} onchange={(e) => setSource(e.currentTarget.value)} data-testid="action-source">
        {#each SOURCES as source (source.id)}
          <option value={source.id}>{source.name}</option>
        {/each}
      </select>
    </label>
    <label class="field">
      <span class="name">Show</span>
      {#if draft.source === 'cliproxyapi'}
        <select value={draft.field} onchange={(e) => set('field', e.currentTarget.value)} data-testid="action-field">
          {#if !PROXY_FIELDS.includes(draft.field)}<option value={draft.field}>{draft.field} (not recognised)</option>{/if}
          {#each PROXY_FIELDS as field (field)}<option value={field}>{FIELD_NAMES[field] ?? field}</option>{/each}
        </select>
      {:else}
        <input value={draft.field} oninput={(e) => set('field', e.currentTarget.value)} data-testid="action-field" spellcheck="false" />
      {/if}
    </label>
    <label class="field">
      <span class="name">Account</span>
      {#if draft.source === 'cliproxyapi' && accountKeys.length}
        <select value={draft.account} onchange={(e) => set('account', e.currentTarget.value)} data-testid="action-account">
          <option value="">All accounts</option>
          {#if draft.account && !accountKeys.includes(draft.account)}<option value={draft.account}>{draft.account} (not seen lately)</option>{/if}
          {#each accountKeys as account (account)}<option value={account}>{account}</option>{/each}
        </select>
      {:else}
        <input value={draft.account} oninput={(e) => set('account', e.currentTarget.value)} placeholder="All accounts" data-testid="action-account" spellcheck="false" />
      {/if}
    </label>
    <p class="hint">The deck shows the number with a bar, and updates it while Open TouchDeck runs. Set up the hub under AI usage.</p>
  {:else if draft.type === 'none'}
    <p class="hint">The button shows on the deck but does nothing. Handy as a label.</p>
  {/if}

  {#if problem}
    <p class="problem" data-testid="action-error" data-reason={problem}>{friendlyRefusal(problem)}</p>
  {/if}
</div>

<style>
  .form {
    display: grid;
    gap: var(--space-3);
  }

  fieldset {
    margin: 0;
    padding: 0;
    border: 0;
  }

  .mods {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-4);
  }

  .keys {
    color: var(--text);
    font-weight: 600;
  }

  .extra {
    display: grid;
    gap: var(--space-3);
    margin-top: var(--space-3);
  }
</style>
