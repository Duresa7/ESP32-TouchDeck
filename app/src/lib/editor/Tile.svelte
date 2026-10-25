<script lang="ts">
  import type { Action, Tile } from './validate.ts';

  interface Props {
    tile: Tile;
    selected: boolean;
    moving: boolean;
    ongrab: (event: PointerEvent, mode: 'move' | 'resize') => void;
    onpick: () => void;
  }

  let { tile, selected, moving, ongrab, onpick }: Props = $props();

  function summary(action: Action): string {
    switch (action.type) {
      case 'key':
        return [...action.mods, `usage ${action.usage}`].join('+');
      case 'mod':
        return `hold ${action.mod}`;
      case 'consumer':
        return `media ${action.usage}`;
      case 'page':
        return `go to ${action.page}`;
      case 'launch':
        return action.path;
      case 'open':
        return action.target;
      case 'command':
        return action.program;
      case 'widget':
        return `${action.source} ${action.field}`;
      default:
        return 'none';
    }
  }
</script>

<div
  class="tile"
  class:selected
  class:moving
  style:grid-row="{tile.row + 1} / span {tile.rowSpan}"
  style:grid-column="{tile.col + 1} / span {tile.colSpan}"
  data-testid="tile"
  data-id={tile.id}
  data-row={tile.row}
  data-col={tile.col}
  data-row-span={tile.rowSpan}
  data-col-span={tile.colSpan}
  data-action={tile.action.type}
  onpointerdown={(event) => ongrab(event, 'move')}
  onclick={onpick}
  onkeydown={(event) => {
    if (event.key === 'Enter' || event.key === ' ') onpick();
  }}
  role="button"
  tabindex="-1"
>
  {#if tile.icon}
    <span class="icon">{tile.icon.replace(/^lv:/, '')}</span>
  {/if}
  <span class="label">{tile.label}</span>
  <span class="action" data-type={tile.action.type}>{summary(tile.action)}</span>
  {#if selected}
    <span
      class="grip"
      data-testid="resize-handle"
      onpointerdown={(event) => {
        event.stopPropagation();
        ongrab(event, 'resize');
      }}
      role="presentation"
    ></span>
  {/if}
</div>

<style>
  .tile {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    min-width: 0;
    min-height: 0;
    padding: 4px 6px;
    overflow: hidden;
    border: 1px solid #2f3b50;
    border-radius: 10px;
    background: linear-gradient(180deg, #243047, #1b2436);
    cursor: grab;
    user-select: none;
    touch-action: none;
  }

  .tile.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px rgb(59 130 246 / 0.35);
  }

  .tile.moving {
    opacity: 0.45;
  }

  .label {
    max-width: 100%;
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .icon,
  .action {
    max-width: 100%;
    overflow: hidden;
    color: var(--muted);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .action[data-type='none'] {
    opacity: 0.6;
  }

  .grip {
    position: absolute;
    right: 3px;
    bottom: 3px;
    width: 14px;
    height: 14px;
    border-right: 3px solid var(--accent);
    border-bottom: 3px solid var(--accent);
    border-radius: 0 0 6px 0;
    cursor: nwse-resize;
  }
</style>
