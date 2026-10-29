import type { EditorStore } from '../../lib/editor/store.ts';
import type { GridPage, Tile } from '../../lib/editor/validate.ts';

export interface TileFieldsProps {
  store: EditorStore;
  page: GridPage;
  tile: Tile;
}

/** The selected Tile's fields: label, icon, background, size and its action. #83 fills this slot; until then it shows nothing. */
export function TileFields(_props: TileFieldsProps) {
  return null;
}
