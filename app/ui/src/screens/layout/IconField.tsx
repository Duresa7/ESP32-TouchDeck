import type { ActionType, Tile } from '../../lib/editor/validate.ts';
import { TileGlyph } from './glyphs.tsx';

const ICON_NAMES: Record<string, string> = {
  'lv:prev': 'Previous',
  'lv:play_pause': 'Play and pause',
  'lv:next': 'Next',
  'lv:mute': 'Speaker off',
  'lv:volume_mid': 'Volume low',
  'lv:volume_max': 'Volume high',
};

const iconName = (icon: string | null) => (icon === null ? 'No icon' : (ICON_NAMES[icon] ?? 'Custom icon'));

export function IconField({ tile, kind }: { tile: Tile; kind: ActionType }) {
  return (
    <div className="field">
      <span className="field-label">Icon</span>
      <div className="icon-field" data-testid="tile-icon">
        <span className="icon-chip" aria-hidden>
          <TileGlyph icon={tile.icon} kind={kind} size={18} />
        </span>
        <span className={tile.icon === null ? 'icon-name is-empty' : 'icon-name'}>{iconName(tile.icon)}</span>
      </div>
    </div>
  );
}
