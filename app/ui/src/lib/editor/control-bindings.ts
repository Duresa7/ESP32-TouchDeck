import type { IntegrationSummary } from '../companion/api.ts';
import type { Layout, Tile } from './validate.ts';

export type ControlPart = 'drag' | 'previous' | 'play_pause' | 'next' | 'mute';

export type KnownIntegration = Pick<IntegrationSummary, 'id' | 'name' | 'streams'>;

export interface ControlBinding {
  page: { id: string; title: string };
  tile: { id: string; label: string };
  integration: { kind: 'known'; id: string; name: string } | { kind: 'unknown'; id: string };
  account: { kind: 'default' } | { kind: 'unknown'; id: string };
  controls: ControlPart[];
}

const CONTROL_WORDS: Record<ControlPart, string> = {
  drag: 'Drag to change',
  previous: 'Previous track',
  play_pause: 'Play or pause',
  next: 'Next track',
  mute: 'Mute',
};

export const controlWords = (controls: readonly ControlPart[]) => controls.map((part) => CONTROL_WORDS[part]).join(' · ');

export function tileControls(tile: Tile, integration: KnownIntegration | undefined): ControlPart[] {
  const { action } = tile;
  if (action.type !== 'widget' || action.source === '') return [];
  switch (tile.render) {
    case 'now-playing':
      return ['previous', 'play_pause', 'next'];
    case 'bar': {
      const settable = integration ? integration.streams.some((stream) => stream.id === action.field && stream.settable) : true;
      const parts: ControlPart[] = [];
      if (tile.input && settable) parts.push('drag');
      if (tile.streams.muted) parts.push('mute');
      return parts;
    }
    case 'graph':
    case 'gif':
    case undefined:
      return [];
    default: {
      const exhaustive: never = tile;
      return exhaustive;
    }
  }
}

const byPlace = (a: Tile, b: Tile) => a.row - b.row || a.col - b.col;

export function controlBindings(layout: Layout, integrations: readonly KnownIntegration[]): ControlBinding[] {
  return layout.pages.flatMap((page) => {
    if (page.kind !== 'grid') return [];
    return page.tiles.toSorted(byPlace).flatMap((tile): ControlBinding[] => {
      if (tile.action.type !== 'widget') return [];
      const { source, account } = tile.action;
      const found = integrations.find((integration) => integration.id === source);
      const controls = tileControls(tile, found);
      if (!controls.length) return [];
      return [
        {
          page: { id: page.id, title: page.title },
          tile: { id: tile.id, label: tile.label },
          integration: found ? { kind: 'known', id: found.id, name: found.name } : { kind: 'unknown', id: source },
          account: account === '' ? { kind: 'default' } : { kind: 'unknown', id: account },
          controls,
        },
      ];
    });
  });
}
