import type { EditorStore } from '../../lib/editor/store.ts';
import type { GridPage, Tile } from '../../lib/editor/validate.ts';

export interface TileFieldsProps {
  store: EditorStore;
  page: GridPage;
  tile: Tile;
}

export function TileFields(_props: TileFieldsProps) {
  return null;
}
