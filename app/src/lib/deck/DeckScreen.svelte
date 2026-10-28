<script lang="ts">
  import { onDestroy } from 'svelte';
  import { iconPixels, loadIcon, paint } from '../editor/icons.ts';
  import type { Layout, Tile } from '../editor/validate.ts';
  import DeckText from './DeckText.svelte';
  import { cellAt, div, geometry, screenSize, tileFits, tileRect } from './geometry.ts';
  import { easeOut, goTo, move, nearestPage, pager, press, release, restX } from './pager.ts';
  import { buildDots, buildPage, buildTitle, sampleRows, sampleWidget, type TileView } from './scene.ts';
  import { THEME, withOpacity } from './theme.ts';

  interface Props {
    layout: Layout;
    page: string | null;
    mode: 'edit' | 'preview';
    selected?: string | null;
    /** One deck pixel per screen pixel instead of fitting the space. */
    actualSize?: boolean;
    onpage?: (id: string) => void;
    onselect?: (id: string | null) => void;
    onmove?: (id: string, row: number, col: number) => void;
    onresize?: (id: string, rowSpan: number, colSpan: number) => void;
    onadd?: (row: number, col: number) => void;
  }

  let { layout, page, mode, selected = null, actualSize = false, onpage, onselect, onmove, onresize, onadd }: Props = $props();

  /** The bezel around the glass, in window pixels. */
  const BEZEL = 14;

  const g = $derived(geometry(screenSize(layout.orientation), layout.fullScreen === true));
  const index = $derived(Math.max(0, layout.pages.findIndex((p) => p.id === page)));
  const current = $derived(layout.pages[index]);
  const options = { widget: sampleWidget, rows: sampleRows };
  const views = $derived(mode === 'edit' ? [buildPage(current, g, options)] : layout.pages.map((p) => buildPage(p, g, options)));
  const editing = $derived(mode === 'edit');

  let width = $state(0);
  let height = $state(0);
  const scale = $derived.by(() => {
    if (actualSize) return 1 / (globalThis.devicePixelRatio || 1);
    const fit = Math.min((width - 2 * BEZEL) / g.screen.w, (height - 2 * BEZEL) / g.screen.h, 1);
    return Math.max(fit, 0.05);
  });

  $effect(() => {
    for (const view of views) for (const tile of view.tiles) if (tile.image) loadIcon(tile.image.id);
  });

  $effect.pre(() => {
    void current;
    performance.mark('page-render-start');
  });

  $effect(() => {
    void views;
    performance.measure('page-render', 'page-render-start');
  });

  // Preview: the pages slide as on the deck, with its 260 ms LVGL ease-out, and the title follows the nearest page.
  const reduced = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
  const deckPager = pager({ pageW: 1, slop: 1, swipe: 1 }, 1);
  let trackX = $state(0);
  let frame = 0;
  let animating = false;
  let shownIndex = $state(0);
  let pressedTile = $state<string | null>(null);
  let mods = $state<Record<string, 'one-shot' | 'locked'>>({});
  let lastModTap: Record<string, number> = {};

  $effect(() => {
    Object.assign(deckPager, { pageW: g.screen.w, slop: g.slop, swipe: g.swipe, count: layout.pages.length });
    if (mode === 'edit') {
      stop();
      deckPager.index = index;
      trackX = 0;
      shownIndex = index;
      return;
    }
    if (deckPager.index !== index || shownIndex !== index) {
      const from = trackX;
      goTo(deckPager, index);
      shownIndex = index;
      slide(from, restX(deckPager), !animating && from === -index * g.screen.w);
    }
  });

  const titleIndex = $derived(mode === 'edit' ? index : nearestPage(trackX, g.screen.w, layout.pages.length));
  const title = $derived(buildTitle(layout.pages[titleIndex]?.title ?? '', g));
  const dots = $derived(buildDots(g, layout.pages.length, editing ? index : shownIndex));

  function stop() {
    cancelAnimationFrame(frame);
    animating = false;
  }

  function run(from: number, to: number, ms: number, curve: (t: number) => number, done?: () => void) {
    stop();
    if (reduced || ms <= 0) {
      trackX = to;
      done?.();
      return;
    }
    animating = true;
    const start = performance.now();
    const step = (now: number) => {
      const t = Math.min(1, (now - start) / ms);
      trackX = Math.round(from + (to - from) * curve(t));
      if (t < 1) frame = requestAnimationFrame(step);
      else {
        animating = false;
        done?.();
      }
    };
    frame = requestAnimationFrame(step);
  }

  function slide(from: number, to: number, instant = false) {
    if (instant || from === to) {
      stop();
      trackX = to;
      return;
    }
    run(from, to, THEME.anim.settleMs, easeOut);
  }

  function bounce(direction: -1 | 1) {
    const rest = restX(deckPager);
    const out = rest + direction * g.bouncePx;
    run(rest, out, THEME.anim.bounceOutMs, easeOut, () => run(out, rest, THEME.anim.bounceBackMs, (t) => 1 - easeOut(1 - t)));
  }

  function showPage(to: number, animate = true) {
    const from = trackX;
    if (to !== shownIndex) mods = Object.fromEntries(Object.entries(mods).filter(([, state]) => state === 'locked'));
    goTo(deckPager, to);
    shownIndex = to;
    slide(from, restX(deckPager), !animate);
    const id = layout.pages[to]?.id;
    if (id && id !== page) onpage?.(id);
  }

  onDestroy(stop);

  let screen = $state<HTMLDivElement>();

  /** A pointer's position in deck pixels. */
  function toScreen(event: PointerEvent) {
    const box = screen!.getBoundingClientRect();
    return { x: (event.clientX - box.left) / scale, y: (event.clientY - box.top) / scale };
  }

  function tileOf(event: Event): Tile | undefined {
    const box = (event.target as HTMLElement).closest<HTMLElement>('[data-testid="tile"]');
    if (!box || current.kind !== 'grid') return undefined;
    const view = layout.pages[shownIndex];
    return view?.kind === 'grid' ? view.tiles.find((t) => t.id === box.dataset.id) : undefined;
  }

  // Preview input follows ui_deck.c: a press on a moving page stops it and never taps, a sideways swipe changes the
  // page before the lift, and only a tap that stayed inside the slop runs the tile.
  function previewDown(event: PointerEvent) {
    if (editing || event.button !== 0) return;
    const caught = animating;
    if (caught) slide(trackX, restX(deckPager), true);
    const point = toScreen(event);
    press(deckPager, Math.trunc(point.x), Math.trunc(point.y));
    pressedTile = caught ? null : (tileOf(event)?.id ?? null);
    window.addEventListener('pointermove', previewMove);
    window.addEventListener('pointerup', previewUp, { once: true });
    window.addEventListener('pointercancel', previewUp, { once: true });
  }

  function previewMove(event: PointerEvent) {
    const point = toScreen(event);
    const before = deckPager.press;
    const m = move(deckPager, point.x, point.y);
    if (before === 'tap' && m.press !== 'tap') pressedTile = null;
    if (m.changed) showPage(m.to);
    else if (m.blocked !== 0) bounce(m.blocked);
  }

  function previewUp(event: PointerEvent) {
    window.removeEventListener('pointermove', previewMove);
    window.removeEventListener('pointercancel', previewUp);
    window.removeEventListener('pointerup', previewUp);
    const tapped = pressedTile;
    const tap = release(deckPager);
    pressedTile = null;
    if (tap && tapped && event.type === 'pointerup') tapTile(tapped);
  }

  /** What a tap does that the deck shows: a page jump, or a modifier's one-shot and locked looks. */
  function tapTile(id: string) {
    const source = layout.pages[shownIndex];
    if (source?.kind !== 'grid') return;
    const tile = source.tiles.find((t) => t.id === id);
    if (!tile) return;
    const action = tile.action;
    if (action.type === 'page') {
      const to = layout.pages.findIndex((p) => p.id === action.page);
      if (to >= 0 && to !== shownIndex) showPage(to);
    } else if (action.type === 'mod') {
      const now = performance.now();
      const state = mods[action.mod];
      const quick = now - (lastModTap[action.mod] ?? -Infinity) <= 300;
      lastModTap = { ...lastModTap, [action.mod]: now };
      const next = state === undefined ? 'one-shot' : state === 'one-shot' && quick ? 'locked' : undefined;
      const rest = Object.fromEntries(Object.entries(mods).filter(([m]) => m !== action.mod));
      mods = next ? { ...rest, [action.mod]: next } : rest;
    } else if (action.type === 'key' || action.type === 'consumer') {
      mods = Object.fromEntries(Object.entries(mods).filter(([, state]) => state === 'locked'));
    }
  }

  function previewKey(event: KeyboardEvent) {
    if (editing) return;
    const to = { ArrowLeft: shownIndex - 1, ArrowRight: shownIndex + 1 }[event.key];
    if (to === undefined) return;
    event.preventDefault();
    if (to >= 0 && to < layout.pages.length) showPage(to, false);
  }

  // Edit: drag a tile to move it and its corner to resize it. The cell under the pointer comes from the deck's own
  // cell edges, so a drop lands where the tile will be drawn.
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

  let drag = $state<Drag | null>(null);

  function cellFor(event: PointerEvent): [number, number] {
    const point = toScreen(event);
    return current.kind === 'grid' ? cellAt(current, g.grid, g.gap, point.x, point.y) : [0, 0];
  }

  function grab(event: PointerEvent, id: string, how: 'move' | 'resize') {
    if (!editing || event.button !== 0 || current.kind !== 'grid') return;
    event.preventDefault();
    event.stopPropagation();
    const tile = current.tiles.find((t) => t.id === id);
    if (!tile) return;
    const [row, col] = cellFor(event);
    drag = { tile: id, mode: how, offsetRow: row - tile.row, offsetCol: col - tile.col, row: tile.row, col: tile.col, rowSpan: tile.rowSpan, colSpan: tile.colSpan, moved: false };
    onselect?.(id);
    (event.currentTarget as HTMLElement).focus?.({ preventScroll: true });
    window.addEventListener('pointermove', follow);
    window.addEventListener('pointerup', drop, { once: true });
  }

  function follow(event: PointerEvent) {
    if (!drag || current.kind !== 'grid') return;
    const tile = current.tiles.find((t) => t.id === drag!.tile);
    if (!tile) return;
    const [row, col] = cellFor(event);
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
    if (done.mode === 'move') onmove?.(done.tile, done.row, done.col);
    else onresize?.(done.tile, done.rowSpan, done.colSpan);
  }

  const ghost = $derived.by(() => {
    if (!drag?.moved || current.kind !== 'grid') return null;
    const tile = current.tiles.find((t) => t.id === drag!.tile);
    if (!tile) return null;
    const span =
      drag.mode === 'move'
        ? { row: drag.row, col: drag.col, rowSpan: tile.rowSpan, colSpan: tile.colSpan }
        : { row: tile.row, col: tile.col, rowSpan: drag.rowSpan, colSpan: drag.colSpan };
    if (!tileFits(current, span)) return null;
    const blocked = current.tiles.some(
      (other) =>
        other.id !== tile.id &&
        span.row < other.row + other.rowSpan &&
        other.row < span.row + span.rowSpan &&
        span.col < other.col + other.colSpan &&
        other.col < span.col + span.colSpan,
    );
    return { rect: tileRect(current, span, g.grid, g.gap), blocked };
  });

  /** Empty cells, which take a new tile on a click while editing. */
  const freeCells = $derived.by(() => {
    if (!editing || current.kind !== 'grid') return [];
    const cells = [];
    for (let row = 0; row < current.rows; row++) {
      for (let col = 0; col < current.cols; col++) {
        const taken = current.tiles.some((t) => row >= t.row && row < t.row + t.rowSpan && col >= t.col && col < t.col + t.colSpan);
        if (!taken) cells.push({ row, col, rect: tileRect(current, { row, col, rowSpan: 1, colSpan: 1 }, g.grid, g.gap) });
      }
    }
    return cells;
  });

  function tileKeys(event: KeyboardEvent, tile: TileView) {
    if (!editing) {
      if (event.key === 'Enter' || event.key === ' ') {
        event.preventDefault();
        tapTile(tile.id);
      }
      return;
    }
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onselect?.(tile.id);
      return;
    }
    if (event.key === 'Escape') {
      onselect?.(null);
      return;
    }
    const step = { ArrowUp: [-1, 0], ArrowDown: [1, 0], ArrowLeft: [0, -1], ArrowRight: [0, 1] }[event.key];
    const source = current.kind === 'grid' ? current.tiles.find((t) => t.id === tile.id) : undefined;
    if (!step || !source) return;
    event.preventDefault();
    onselect?.(tile.id);
    if (event.shiftKey) onresize?.(tile.id, source.rowSpan + step[0], source.colSpan + step[1]);
    else onmove?.(tile.id, source.row + step[0], source.col + step[1]);
  }

  function describe(tile: TileView): string {
    const source = layout.pages[editing ? index : shownIndex];
    const label = source?.kind === 'grid' ? source.tiles.find((t) => t.id === tile.id)?.label : undefined;
    return label ?? tile.id;
  }

  const lift = (rect: { x: number; y: number }) => ({ x: rect.x - g.grid.x, y: rect.y - g.grid.y });
  const lock = THEME.lock;
</script>

<div
  class="stage"
  class:actual={actualSize}
  bind:clientWidth={width}
  bind:clientHeight={height}
  data-testid="deck-preview"
  data-orientation={layout.orientation}
  data-mode={mode}
  data-full-screen={g.fullScreen}
  data-page={layout.pages[editing ? index : shownIndex]?.id}
>
  <div class="device" style:width="{g.screen.w * scale + 2 * BEZEL}px" style:height="{g.screen.h * scale + 2 * BEZEL}px">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="screen"
      bind:this={screen}
      data-testid="deck-screen"
      data-scale={scale}
      style:width="{g.screen.w}px"
      style:height="{g.screen.h}px"
      style:transform="scale({scale})"
      style:background={THEME.color.bg}
      style:--s={scale}
      style:--deck-tile={THEME.color.tile}
      style:--deck-tile-down={THEME.color.tileDown}
      style:--deck-glow={THEME.color.glow}
      style:--deck-glow-soft={withOpacity(THEME.color.glow, THEME.glowOpa)}
      style:--deck-lock={THEME.color.lock}
      style:--deck-dot={THEME.color.dotIdle}
      style:--deck-dot-on={THEME.color.dotActive}
      onpointerdown={previewDown}
      onkeydown={previewKey}
    >
      {#if title}
        <div data-testid="deck-title"><DeckText view={title} /></div>
      {/if}

      <div class="band" style:top="{g.band.y}px" style:width="{g.band.w}px" style:height="{g.band.h}px">
        <div class="track" style:transform="translateX({editing ? 0 : trackX}px)">
          {#each views as view, i (view.id)}
            {@const here = editing || i === shownIndex}
            <div class="page" style:left="{editing ? 0 : i * g.screen.w}px" style:width="{g.screen.w}px" style:height="{g.band.h}px" aria-hidden={!here}>
              {#if view.kind === 'grid'}
                {@const source = layout.pages[editing ? index : i]}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="grid"
                  style:left="{g.grid.x - g.band.x}px"
                  style:top="{g.grid.y - g.band.y}px"
                  style:width="{g.grid.w}px"
                  style:height="{g.grid.h}px"
                  data-testid={here ? 'grid' : undefined}
                  data-page={view.id}
                  data-rows={source.kind === 'grid' ? source.rows : 1}
                  data-cols={source.kind === 'grid' ? source.cols : 1}
                  role="group"
                  aria-label="{view.title} buttons"
                  onpointerdown={(event) => {
                    if (editing && event.target === event.currentTarget) onselect?.(null);
                  }}
                >
                  {#each freeCells as cell (`${cell.row}:${cell.col}`)}
                    <button
                      class="slot"
                      style:left="{lift(cell.rect).x}px"
                      style:top="{lift(cell.rect).y}px"
                      style:width="{cell.rect.w}px"
                      style:height="{cell.rect.h}px"
                      onclick={() => onadd?.(cell.row, cell.col)}
                      data-testid="empty-cell"
                      data-row={cell.row}
                      data-col={cell.col}
                      aria-label="Add a button in row {cell.row + 1}, column {cell.col + 1}"
                      tabindex="-1"
                    >
                      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>
                    </button>
                  {/each}

                  {#each view.tiles as tile (tile.id)}
                    {@const t = source.kind === 'grid' ? source.tiles.find((s) => s.id === tile.id) : undefined}
                    {@const mod = t?.action.type === 'mod' ? mods[t.action.mod] : undefined}
                    <div
                      class="tile"
                      class:pressed={!editing && pressedTile === tile.id && i === shownIndex}
                      class:selected={editing && selected === tile.id}
                      class:moving={drag?.moved === true && drag.tile === tile.id && drag.mode === 'move'}
                      class:one-shot={mod === 'one-shot'}
                      class:locked={mod === 'locked'}
                      style:left="{lift(tile.rect).x}px"
                      style:top="{lift(tile.rect).y}px"
                      style:width="{tile.rect.w}px"
                      style:height="{tile.rect.h}px"
                      data-testid="tile"
                      data-id={tile.id}
                      data-row={t?.row}
                      data-col={t?.col}
                      data-row-span={t?.rowSpan}
                      data-col-span={t?.colSpan}
                      data-action={tile.action}
                      role="button"
                      tabindex={here ? 0 : -1}
                      aria-label={describe(tile)}
                      aria-pressed={editing ? selected === tile.id : undefined}
                      onpointerdown={(event) => grab(event, tile.id, 'move')}
                      onclick={() => editing && onselect?.(tile.id)}
                      onkeydown={(event) => tileKeys(event, tile)}
                    >
                      <div class="face" style:background-color={mod === 'locked' ? THEME.color.modLocked : undefined}></div>
                      <div class="content">
                        {#each tile.texts as text, n (n)}
                          <DeckText view={text} />
                        {/each}
                        {#if tile.image}
                          {#if $iconPixels[tile.image.id]}
                            <canvas
                              class="image"
                              style:left="{tile.image.x}px"
                              style:top="{tile.image.y}px"
                              use:paint={$iconPixels[tile.image.id]}
                              data-testid="tile-glyph"
                              data-id={tile.image.id}
                            ></canvas>
                          {/if}
                        {/if}
                        {#if tile.bar}
                          <div class="bar" style:left="{tile.bar.x}px" style:top="{tile.bar.y}px" style:width="{tile.bar.w}px" style:height="{tile.bar.h}px" style:border-radius="{tile.bar.radius}px"></div>
                        {/if}
                        {#if mod === 'locked'}
                          <span class="shackle" style:right="{lock.inset + div(lock.w - lock.shackleW, 2)}px" style:top="{div(tile.rect.h, 2) - div(lock.shackleH, 2) + div(lock.shackleH - lock.h, 2)}px"></span>
                          <span class="body" style:right="{lock.inset}px" style:top="{div(tile.rect.h, 2) - div(lock.bodyH, 2) + div(lock.h - lock.bodyH, 2)}px"></span>
                        {/if}
                      </div>
                      {#if editing && selected === tile.id}
                        <span
                          class="grip"
                          data-testid="resize-handle"
                          onpointerdown={(event) => grab(event, tile.id, 'resize')}
                          role="presentation"
                        ></span>
                      {/if}
                    </div>
                  {/each}

                  {#if ghost && here}
                    <div
                      class="ghost"
                      class:blocked={ghost.blocked}
                      data-testid="drag-ghost"
                      style:left="{lift(ghost.rect).x}px"
                      style:top="{lift(ghost.rect).y}px"
                      style:width="{ghost.rect.w}px"
                      style:height="{ghost.rect.h}px"
                    ></div>
                  {/if}
                </div>
              {:else}
                <div class="rows" data-testid={here ? 'dashboard-page' : undefined}>
                  {#each view.rows as row, r (r)}
                    <div class="row" style:left="{row.rect.x - g.band.x}px" style:top="{row.rect.y - g.band.y}px" style:width="{row.rect.w}px" style:height="{row.rect.h}px">
                      {#each row.texts as text, n (n)}
                        <DeckText view={text} />
                      {/each}
                      {#if row.bar}
                        <div class="bar" style:left="{row.bar.x}px" style:top="{row.bar.y}px" style:width="{row.bar.w}px" style:height="{row.bar.h}px" style:border-radius="{row.bar.radius}px"></div>
                      {/if}
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      </div>

      {#each dots as dot, d (d)}
        <span class="dot" class:active={dot.active} style:left="{dot.x}px" style:top="{dot.y}px" style:width="{dot.w}px" style:height="{dot.h}px" data-testid="deck-dot"></span>
      {/each}
    </div>
  </div>
</div>

<style>
  .stage {
    display: grid;
    place-items: center;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .stage.actual {
    overflow: auto;
    place-items: start center;
  }

  .device {
    position: relative;
    flex: none;
    border-radius: 18px;
    background: #05070b;
    box-shadow:
      0 0 0 1px rgb(255 255 255 / 0.06),
      inset 0 1px 0 rgb(255 255 255 / 0.05),
      0 24px 60px -20px rgb(0 0 0 / 0.7);
  }

  .screen {
    position: absolute;
    top: 14px;
    left: 14px;
    overflow: hidden;
    transform-origin: 0 0;
    user-select: none;
    touch-action: none;
    contain: layout paint;
  }

  .band {
    position: absolute;
    left: 0;
    overflow: hidden;
  }

  .track,
  .page,
  .grid,
  .rows {
    position: absolute;
    top: 0;
    left: 0;
  }

  .track {
    will-change: transform;
  }

  .grid {
    position: absolute;
  }

  .tile,
  .slot,
  .ghost,
  .row,
  .bar,
  .image,
  .dot {
    position: absolute;
  }

  .tile {
    outline: none;
  }

  .face {
    position: absolute;
    inset: 0;
    border-radius: 20px;
    background-color: var(--deck-tile);
    transition:
      background-color 120ms cubic-bezier(0.333, 0.879, 0.667, 0.928),
      box-shadow 120ms cubic-bezier(0.333, 0.879, 0.667, 0.928);
  }

  .content {
    position: absolute;
    inset: 0;
    overflow: hidden;
  }

  /* tile_down: the pressed fill, a 3 px border, and a 28 px glow spread by 2 px at 120 of 255. */
  .tile.pressed .face {
    background-color: var(--deck-tile-down) !important;
    box-shadow:
      inset 0 0 0 3px var(--deck-glow),
      0 0 14px 2px var(--deck-glow-soft);
  }

  .tile.one-shot .face {
    outline: 4px solid var(--deck-glow);
    outline-offset: 2px;
  }

  .shackle,
  .body {
    position: absolute;
    box-sizing: border-box;
  }

  .shackle {
    width: 18px;
    height: 20px;
    border: 4px solid var(--deck-lock);
    border-bottom: 0;
    border-radius: 9px 9px 0 0;
  }

  .body {
    width: 26px;
    height: 20px;
    border-radius: 4px;
    background: var(--deck-lock);
  }

  .image {
    width: 64px;
    height: 64px;
    image-rendering: pixelated;
  }

  .bar {
    background: var(--deck-glow);
  }

  .row {
    overflow: hidden;
    border-radius: 10px;
    background: var(--deck-tile);
  }

  .dot {
    border-radius: 999px;
    background: var(--deck-dot);
    transition:
      background-color 160ms cubic-bezier(0.333, 0.879, 0.667, 0.928),
      left 160ms cubic-bezier(0.333, 0.879, 0.667, 0.928),
      width 160ms cubic-bezier(0.333, 0.879, 0.667, 0.928);
  }

  .dot.active {
    background: var(--deck-dot-on);
  }

  /* Editing marks, drawn over the deck and sized in window pixels whatever the scale. */
  .stage[data-mode='edit'] .tile {
    cursor: grab;
  }

  .tile.selected::after,
  .tile:focus-visible::after {
    position: absolute;
    inset: calc(-5px / var(--s));
    border: calc(2px / var(--s)) solid var(--accent);
    border-radius: calc(20px + 5px / var(--s));
    content: '';
    pointer-events: none;
  }

  .tile:focus-visible:not(.selected)::after {
    border-style: dashed;
  }

  .tile.moving {
    opacity: 0.4;
  }

  .slot {
    display: grid;
    place-items: center;
    padding: 0;
    border: calc(1.5px / var(--s)) dashed rgb(232 236 248 / 0.14);
    border-radius: 20px;
    background: transparent;
    color: rgb(232 236 248 / 0.28);
    cursor: copy;
    transition:
      background-color 150ms ease,
      border-color 150ms ease,
      color 150ms ease;
  }

  .slot svg {
    width: calc(22px / var(--s));
    height: calc(22px / var(--s));
    fill: none;
    stroke: currentColor;
    stroke-linecap: round;
    stroke-width: 2;
  }

  @media (hover: hover) and (pointer: fine) {
    .slot:hover {
      border-color: rgb(72 160 255 / 0.6);
      background: rgb(72 160 255 / 0.06);
      color: #48a0ff;
    }
  }

  .grip {
    position: absolute;
    right: calc(-7px / var(--s));
    bottom: calc(-7px / var(--s));
    z-index: 2;
    width: calc(16px / var(--s));
    height: calc(16px / var(--s));
    border: calc(2px / var(--s)) solid var(--bg, #121722);
    border-radius: 50%;
    background: var(--accent);
    cursor: nwse-resize;
  }

  .ghost {
    z-index: 3;
    border: calc(2px / var(--s)) dashed var(--accent);
    border-radius: 20px;
    background: rgb(72 160 255 / 0.14);
    pointer-events: none;
  }

  .ghost.blocked {
    border-color: var(--bad);
    background: rgb(240 114 114 / 0.14);
  }

  @media (prefers-reduced-motion: reduce) {
    .face,
    .dot,
    .slot {
      transition: none;
    }
  }
</style>
