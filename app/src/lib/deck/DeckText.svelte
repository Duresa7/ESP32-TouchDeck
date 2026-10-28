<script lang="ts">
  import { FONTS, type TextView } from './scene.ts';

  interface Props {
    view: TextView;
    /** Clip to the label's box, as a fixed-size LVGL label does. */
    clip?: boolean;
  }

  let { view, clip = false }: Props = $props();

  const lineHeight = $derived(FONTS[view.size].lineHeight);
  // A label with a fixed height clips the lines that run past it, as LVGL clips a fixed-size label.
  const overflows = $derived(view.label.lines.length * lineHeight > view.label.height);
  const words = $derived(view.label.lines.map((line) => line.letters.map((l) => String.fromCodePoint(l.letter)).join('')).join(' '));
</script>

<!-- Each letter sits on the pixel LVGL gives it, so wrapping and centring match the deck. -->
<div
  class="text"
  class:clip={clip || overflows}
  style:left="{view.x}px"
  style:top="{view.y}px"
  style:width="{view.label.width}px"
  style:height="{view.label.height}px"
  style:color={view.color}
  style:opacity={view.opacity / 255}
  style:font-size="{view.size}px"
  style:line-height="{lineHeight}px"
  aria-label={words}
  role="img"
>
  {#each view.label.lines as line, l (l)}
    {#each line.letters as letter, i (i)}
      {#if letter.placeholder}
        <span class="box" style:left="{line.x + letter.x}px" style:top="{line.y}px" style:width="{letter.width}px" style:height="{lineHeight}px"
        ></span>
      {:else}
        <span style:left="{line.x + letter.x}px" style:top="{line.y}px">{String.fromCodePoint(letter.letter)}</span>
      {/if}
    {/each}
  {/each}
</div>

<style>
  .text {
    position: absolute;
    pointer-events: none;
    font-family: 'TouchDeck Montserrat', 'TouchDeck Symbols';
    font-weight: 500;
    font-kerning: none;
    font-synthesis: none;
    white-space: pre;
  }

  .clip {
    overflow: hidden;
  }

  span {
    position: absolute;
  }

  .box {
    box-sizing: border-box;
    border: 1px solid currentColor;
  }
</style>
