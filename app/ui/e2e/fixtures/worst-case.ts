import type { Layout, Tile } from '../../src/lib/editor/validate.ts';

const LABELS = [
  'Launch Photoshop',
  'Ctrl+Alt+Shift+F',
  'J',
  'Zoom mute toggle',
  '☕ Coffee break',
  'Привет мир',
  '日本語ボタン',
  'Wiśniewska',
  'Volume  max',
  'OBS: Scene 12',
  'WWWWWWWWWWWWWWWW',
  'iiiiiiiiiiiiiiii',
  '100% CPU',
  'Bartholomew F.',
  'Ünïcödé',
  '→ Next',
];

const key = (usage: number): Tile['action'] => ({ type: 'key', usage, mods: ['ctrl', 'alt', 'shift'] });

function full(): Layout['pages'][number] {
  const tiles: Tile[] = [];
  for (let i = 0; i < 64; i++) {
    tiles.push({ id: `t${i}`, label: LABELS[i % LABELS.length], row: Math.floor(i / 8), col: i % 8, rowSpan: 1, colSpan: 1, icon: null, action: key(58 + (i % 12)) });
  }
  return { id: 'full', title: 'Wiśniewska-Kowalczyk Pro', kind: 'grid', rows: 8, cols: 8, tiles };
}

const tile = (id: string, label: string, row: number, col: number, rowSpan: number, colSpan: number, action: Tile['action'], icon: string | null = null): Tile => ({
  id,
  label,
  row,
  col,
  rowSpan,
  colSpan,
  icon,
  action,
});

export const worstCase: Layout = {
  schema: 1,
  name: "Aleksandra's Studio Desk",
  orientation: 'landscape',
  pages: [
    full(),
    { id: 'one', title: 'W', kind: 'grid', rows: 1, cols: 1, tiles: [tile('w', 'W', 0, 0, 1, 1, { type: 'none' })] },
    {
      id: 'column',
      title: 'Launch everything today',
      kind: 'grid',
      rows: 8,
      cols: 1,
      tiles: LABELS.slice(0, 8).map((label, i) => tile(`c${i}`, label, i, 0, 1, 1, { type: 'launch', path: 'C:\\Program Files\\App\\app.exe', args: [], cwd: '' })),
    },
    { id: 'accounts', title: 'Usage on every account', kind: 'dashboard', source: 'cliproxyapi' },
    { id: 'empty', title: 'Empty', kind: 'grid', rows: 3, cols: 4, tiles: [] },
    {
      id: 'meters',
      title: 'Limits',
      kind: 'grid',
      rows: 2,
      cols: 4,
      tiles: [
        tile('m0', 'Claude 5h usage', 0, 0, 1, 1, { type: 'widget', source: 'cliproxyapi', field: 'five_hour_pct', account: '' }),
        tile('m1', 'Seven day window', 0, 1, 1, 1, { type: 'widget', source: 'cliproxyapi', field: 'seven_day_reset', account: '' }),
        tile('m2', 'Plan', 0, 2, 1, 1, { type: 'widget', source: 'cliproxyapi', field: 'plan', account: '' }),
        tile('m3', 'Unknown field', 0, 3, 1, 1, { type: 'widget', source: 'cliproxyapi', field: 'tokens_left', account: '' }),
        tile('m4', 'Requests in 10m', 1, 0, 1, 4, { type: 'widget', source: 'cliproxyapi', field: 'recent_10m', account: '' }),
      ],
    },
    {
      id: 'media',
      title: 'Media',
      kind: 'grid',
      rows: 2,
      cols: 8,
      tiles: [
        tile('p', 'Previous track', 0, 0, 2, 2, { type: 'consumer', usage: 182 }, 'lv:prev'),
        tile('pp', 'Play/Pause', 0, 2, 1, 1, { type: 'consumer', usage: 205 }, 'lv:play_pause'),
        tile('n', 'Next', 1, 2, 1, 1, { type: 'consumer', usage: 181 }, 'lv:next'),
        tile('mu', 'Mute everything', 0, 3, 2, 5, { type: 'consumer', usage: 226 }, 'lv:mute'),
      ],
    },
    {
      id: 'jump',
      title: 'Go',
      kind: 'grid',
      rows: 3,
      cols: 3,
      tiles: [
        tile('g0', 'Back to start', 0, 0, 1, 3, { type: 'page', page: 'full' }),
        tile('g1', 'Shift', 1, 0, 1, 1, { type: 'mod', mod: 'shift' }),
        tile('g2', 'Ctrl', 1, 1, 1, 1, { type: 'mod', mod: 'ctrl' }),
        tile('g3', 'Win+Shift+S', 2, 0, 1, 3, { type: 'key', usage: 22, mods: ['shift', 'win'] }),
      ],
    },
  ],
};
