<script lang="ts">
  import ActionForm from './ActionForm.svelte';
  import IconPicker from './IconPicker.svelte';
  import { ICONS, type Action, type Edit, type Layout, type Tile, moveTile, resizeTile, setAction, setIcon, setLabel } from './validate.ts';

  interface Props {
    page: string;
    tile: Tile;
    pages: { id: string; title: string }[];
    /** Applies an edit to the layout and returns the refusal, or null when it took. */
    apply: (edit: (layout: Layout) => Edit) => string | null;
    onremove: () => void;
  }

  let { page, tile, pages, apply, onremove }: Props = $props();

  let label = $state('');
  let labelProblem = $state<string | null>(null);

  $effect.pre(() => {
    label = tile.label;
    labelProblem = null;
  });

  function number(event: Event & { currentTarget: HTMLInputElement }) {
    return Math.trunc(event.currentTarget.valueAsNumber);
  }
</script>

<div class="props" data-testid="tile-props" data-id={tile.id}>
  <div class="head">
    <span class="id">{tile.id}</span>
    <button class="danger" onclick={onremove} data-testid="remove-tile">Remove tile</button>
  </div>

  <label>
    <span>Label</span>
    <input
      bind:value={label}
      oninput={() => (labelProblem = apply((l) => setLabel(l, page, tile.id, label)))}
      maxlength="32"
      data-testid="tile-label"
    />
  </label>
  {#if labelProblem}
    <p class="problem" data-testid="label-error">{labelProblem}</p>
  {/if}

  <label>
    <span>Icon</span>
    <select
      value={tile.icon ?? ''}
      onchange={(event) => apply((l) => setIcon(l, page, tile.id, event.currentTarget.value || null))}
      data-testid="tile-icon"
    >
      <option value="">No icon</option>
      {#each ICONS as icon (icon)}
        <option value={icon}>{icon.slice(3)}</option>
      {/each}
      {#if tile.icon && !(ICONS as readonly string[]).includes(tile.icon)}
        <option value={tile.icon}>{tile.icon}</option>
      {/if}
    </select>
  </label>
  <IconPicker {page} {tile} {apply} />

  <div class="cells">
    <label><span>Row</span><input type="number" min="0" max="7" value={tile.row} onchange={(e) => apply((l) => moveTile(l, page, tile.id, number(e), tile.col))} data-testid="tile-row" /></label>
    <label><span>Column</span><input type="number" min="0" max="7" value={tile.col} onchange={(e) => apply((l) => moveTile(l, page, tile.id, tile.row, number(e)))} data-testid="tile-col" /></label>
    <label><span>Rows</span><input type="number" min="1" max="8" value={tile.rowSpan} onchange={(e) => apply((l) => resizeTile(l, page, tile.id, number(e), tile.colSpan))} data-testid="tile-row-span" /></label>
    <label><span>Columns</span><input type="number" min="1" max="8" value={tile.colSpan} onchange={(e) => apply((l) => resizeTile(l, page, tile.id, tile.rowSpan, number(e)))} data-testid="tile-col-span" /></label>
  </div>

  {#key tile.id}
    <ActionForm action={tile.action} {pages} onchange={(action: Action) => apply((l) => setAction(l, page, tile.id, action))} />
  {/key}
</div>

<style>
  .props {
    display: grid;
    gap: 12px;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .id {
    font-family: var(--mono);
    color: var(--muted);
  }

  label {
    display: grid;
    gap: 4px;
  }

  label > span {
    color: var(--muted);
    font-size: 12px;
  }

  .cells {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
  }

  .problem {
    margin: -6px 0 0;
    color: var(--bad);
    font-size: 12px;
  }

  .danger {
    color: var(--bad);
  }
</style>
