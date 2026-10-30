const rgb565 = (r: number, g: number, b: number) => ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3);

export function panelColor(r: number, g: number, b: number): string {
  const value = rgb565(r, g, b);
  const r5 = (value >> 11) & 0x1f;
  const g6 = (value >> 5) & 0x3f;
  const b5 = value & 0x1f;
  const hex = (n: number) => n.toString(16).padStart(2, '0');
  return `#${hex((r5 << 3) | (r5 >> 2))}${hex((g6 << 2) | (g6 >> 4))}${hex((b5 << 3) | (b5 >> 2))}`;
}

export function withOpacity(hex: string, opa: number): string {
  const n = Number.parseInt(hex.slice(1), 16);
  return `rgb(${n >> 16} ${(n >> 8) & 0xff} ${n & 0xff} / ${(opa / 255).toFixed(3)})`;
}

const LV_OPA_60 = 153;

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
  staleOpa: LV_OPA_60,
  widget: { valueDy: 16, captionDy: 30, barH: 6, barBottom: 12, barRadius: 3 },
  dash: { rowGap: 8, radius: 10, pad: 20, barH: 12, rowsMax: 8 },
  graph: { pad: 20, top: 56, radius: 10, lineW: 4, areaMix: 36 },
  anim: { settleMs: 260, bounceOutMs: 110, bounceBackMs: 150, pressMs: 120, dotMs: 160 },
  font: { tile: 48, title: 28, caption: 28, small: 14 },
} as const;

const LV_SYMBOL = {
  PREV: '\uF048',
  PLAY: '\uF04B',
  PAUSE: '\uF04C',
  NEXT: '\uF051',
  MUTE: '\uF026',
  VOLUME_MID: '\uF027',
  VOLUME_MAX: '\uF028',
} as const;

export const SYMBOLS: Record<string, string> = {
  'lv:prev': LV_SYMBOL.PREV,
  'lv:play_pause': LV_SYMBOL.PLAY + LV_SYMBOL.PAUSE,
  'lv:next': LV_SYMBOL.NEXT,
  'lv:mute': LV_SYMBOL.MUTE,
  'lv:volume_mid': LV_SYMBOL.VOLUME_MID,
  'lv:volume_max': LV_SYMBOL.VOLUME_MAX,
};
