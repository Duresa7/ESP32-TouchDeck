<script lang="ts">
  import TileBox from './Tile.svelte';
  import type { GridPage, Orientation } from './validate.ts';

  interface Props {
    page: GridPage;
    orientation: Orientation;
    selected: string | null;
    onselect: (tile: string | null) => void;
    onmove: (tile: string, row: number, col: number) => void;
    onresize: (tile: string, rowSpan: number, colSpan: number) => void;
  }

  let { page, orientation, selected, onselect, onmove, onresize }: Props = $props();

  interface Drag {
    tile: string;
    mode: 'move' | 'resize';
    offsetRow: number;
    offsetCol: number;
    row: number;
    col: number;
    rowSpan: number;
    colSpan: number;
    moved: boolean;
  }

  let cells = $state<HTMLDivElement>();
  let drag = $state<Drag | null>(null);

  $effect.pre(() => {
    void page;
    performance.mark('page-render-start');
  });

  $effect(() => {
    void page;
    performance.measure('page-render', 'page-render-start');
  });

  /** The cell under a pointer, unclamped, so a drop past the edge reads as outside the grid. */
  function cellAt(event: PointerEvent): [number, number] {
    const rect = cells!.getBoundingClientRect();
    const row = Math.floor(((event.clientY - rect.top) / rect.height) * page.rows);
    const col = Math.floor(((event.clientX - rect.left) / rect.width) * page.cols);
    return [row, col];
  }

  function grab(event: PointerEvent, id: string, mode: 'move' | 'resize') {
    if (event.button !== 0) return;
    event.preventDefault();
    const tile = page.tiles.find((candidate) => candidate.id === id);
    if (!tile || !cells) return;
    const [row, col] = cellAt(event);
    drag = {
      tile: id,
      mode,
      offsetRow: row - tile.row,
      offsetCol: col - tile.col,
      row: tile.row,
      col: tile.col,
      rowSpan: tile.rowSpan,
      colSpan: tile.colSpan,
      moved: false,
    };
    onselect(id);
    window.addEventListener('pointermove', follow);
    window.addEventListener('pointerup', drop, { once: true });
  }

  function follow(event: PointerEvent) {
    if (!drag) return;
    const tile = page.tiles.find((candidate) => candidate.id === drag!.tile)!;
    const [row, col] = cellAt(event);
    if (drag.mode === 'move') {
      const next = { row: row - drag.offsetRow, col: col - drag.offsetCol };
      if (next.row !== drag.row || next.col !== drag.col) drag = { ...drag, ...next, moved: true };
    } else {
      const next = { rowSpan: Math.max(1, row - tile.row + 1), colSpan: Math.max(1, col - tile.col + 1) };
      if (next.rowSpan !== drag.rowSpan || next.colSpan !== drag.colSpan) drag = { ...drag, ...next, moved: true };
    }
  }

  function drop() {
    window.removeEventListener('pointermove', follow);
    const done = drag;
    drag = null;
    if (!done?.moved) return;
    if (done.mode === 'move') onmove(done.tile, done.row, done.col);
    else onresize(done.tile, done.rowSpan, done.colSpan);
  }

  function keys(event: KeyboardEvent) {
    const tile = page.tiles.find((candidate) => candidate.id === selected);
    const step = { ArrowUp: [-1, 0], ArrowDown: [1, 0], ArrowLeft: [0, -1], ArrowRight: [0, 1] }[event.key];
    if (!tile || !step) return;
    event.preventDefault();
    if (event.shiftKey) onresize(tile.id, tile.rowSpan + step[0], tile.colSpan + step[1]);
    else onmove(tile.id, tile.row + step[0], tile.col + step[1]);
  }

  const ghost = $derived.by(() => {
    if (!drag?.moved) return null;
    const tile = page.tiles.find((candidate) => candidate.id === drag!.tile);
    if (!tile) return null;
    return drag.mode === 'move'
      ? { row: drag.row, col: drag.col, rowSpan: tile.rowSpan, colSpan: tile.colSpan }
      : { row: tile.row, col: tile.col, rowSpan: drag.rowSpan, colSpan: drag.colSpan };
  });
</script>

<div class="frame" data-orientation={orientation}>
  <div class="deck" data-testid="deck-preview" data-orientation={orientation}>
    <div class="band">{page.title}</div>
    <div
      class="cells"
      bind:this={cells}
      style:grid-template-rows="repeat({page.rows}, minmax(0, 1fr))"
      style:grid-template-columns="repeat({page.cols}, minmax(0, 1fr))"
      data-testid="grid"
      data-page={page.id}
      data-rows={page.rows}
      data-cols={page.cols}
      tabindex="0"
      role="grid"
      aria-label="{page.title} tiles"
      onkeydown={keys}
      onpointerdown={(event) => {
        if (event.target === cells) onselect(null);
      }}
    >
      {#each Array.from({ length: page.rows * page.cols }) as _, index (index)}
        <div
          class="cell"
          style:grid-row={Math.floor(index / page.cols) + 1}
          style:grid-column={(index % page.cols) + 1}
        ></div>
      {/each}
      {#each page.tiles as tile (tile.id)}
        <TileBox
          {tile}
          selected={tile.id === selected}
          moving={drag?.moved === true && drag.tile === tile.id && drag.mode === 'move'}
          ongrab={(event, mode) => grab(event, tile.id, mode)}
          onpick={() => onselect(tile.id)}
        />
      {/each}
      {#if ghost}
        {@const inside =
          ghost.row >= 0 && ghost.col >= 0 && ghost.row + ghost.rowSpan <= page.rows && ghost.col + ghost.colSpan <= page.cols}
        {#if inside}
          <div
            class="ghost"
            data-testid="drag-ghost"
            style:grid-row="{ghost.row + 1} / span {ghost.rowSpan}"
            style:grid-column="{ghost.col + 1} / span {ghost.colSpan}"
          ></div>
        {/if}
      {/if}
    </div>
  </div>
</div>

<style>
  .frame {
    display: grid;
    place-items: center;
    width: 100%;
    height: 100%;
    min-height: 0;
    container-type: size;
  }

  .deck {
    display: flex;
    flex-direction: column;
    width: min(100cqw, calc(100cqh * 1024 / 600));
    aspect-ratio: 1024 / 600;
    padding: 1.2%;
    border: 6px solid #05070b;
    border-radius: 16px;
    background: #0a0d13;
    box-shadow: 0 10px 30px rgb(0 0 0 / 0.35);
  }

  .deck[data-orientation='portrait'] {
    width: min(100cqw, calc(100cqh * 600 / 1024));
    aspect-ratio: 600 / 1024;
  }

  .band {
    flex: none;
    padding: 0 4px 6px;
    overflow: hidden;
    color: var(--muted);
    font-size: 12px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cells {
    position: relative;
    display: grid;
    flex: 1;
    gap: 6px;
    min-height: 0;
    outline: none;
  }

  .cells:focus-visible {
    box-shadow: 0 0 0 2px rgb(59 130 246 / 0.35);
  }

  .cell {
    border: 1px dashed #1e2738;
    border-radius: 10px;
    pointer-events: none;
  }

  .ghost {
    z-index: 2;
    border: 2px dashed var(--accent);
    border-radius: 10px;
    background: rgb(59 130 246 / 0.12);
    pointer-events: none;
  }
</style>
