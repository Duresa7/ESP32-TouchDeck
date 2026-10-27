// The deck's look, ported from firmware/components/ui/ui_theme.c. Colors pass through RGB565 and back the way
// ui_color() widens them, so each one is the color the panel shows and a deck screenshot reads back.

const rgb565 = (r: number, g: number, b: number) => ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3);

/** A UI_RGB565 token as the panel shows it, widened as ui_color() does: the low bits stay zero. */
export function panelColor(r: number, g: number, b: number): string {
  const value = rgb565(r, g, b);
  const hex = (n: number) => n.toString(16).padStart(2, '0');
  return `#${hex(((value >> 11) & 0x1f) << 3)}${hex(((value >> 5) & 0x3f) << 2)}${hex((value & 0x1f) << 3)}`;
}

/** A #rrggbb color at an LVGL opacity from 0 to 255, as CSS. */
export function withOpacity(hex: string, opa: number): string {
  const n = Number.parseInt(hex.slice(1), 16);
  return `rgb(${n >> 16} ${(n >> 8) & 0xff} ${n & 0xff} / ${(opa / 255).toFixed(3)})`;
}

export const THEME = {
  color: {
    bg: panelColor(8, 8, 16),
    tile: panelColor(24, 32, 48),
    tileDown: panelColor(40, 64, 104),
    text: panelColor(232, 236, 248),
    title: panelColor(120, 132, 152),
    caption: panelColor(160, 172, 192),
    dotIdle: panelColor(64, 72, 88),
    dotActive: panelColor(72, 160, 255),
    glow: panelColor(72, 160, 255),
    modLocked: panelColor(32, 96, 192),
    lock: panelColor(232, 236, 248),
  },
  tileRadius: 20,
  tileDownBorder: 3,
  glowWidth: 28,
  glowSpread: 2,
  glowOpa: 120,
  modOutlineWidth: 4,
  modOutlinePad: 2,
  lock: { w: 26, h: 32, shackleW: 18, shackleH: 20, stroke: 4, bodyH: 20, radius: 4, inset: 18 },
  media: { gap: 14, labelInset: 16 },
  /** LV_OPA_40: app and widget tiles while no app is connected. */
  dimOpa: 102,
  /** LV_OPA_60: a stale widget value. */
  staleOpa: 153,
  widget: { valueDy: 16, captionDy: 30, barH: 6, barBottom: 12, barRadius: 3 },
  dash: { rowGap: 8, radius: 10, pad: 20, barH: 12, rowsMax: 8 },
  anim: { settleMs: 260, bounceOutMs: 110, bounceBackMs: 150, pressMs: 120, dotMs: 160 },
  font: { tile: 48, title: 28, caption: 28, small: 14 },
} as const;

/** The built-in `lv:` icons as the LV_SYMBOL glyphs the deck draws in the tile font. */
export const SYMBOLS: Record<string, string> = {
  'lv:prev': '',
  'lv:play_pause': '',
  'lv:next': '',
  'lv:mute': '',
  'lv:volume_mid': '',
  'lv:volume_max': '',
};
