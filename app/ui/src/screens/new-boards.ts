import { useStore } from 'zustand';
import { createStore } from 'zustand/vanilla';
import type { DeviceSummary, NewBoard } from '../companion/api.ts';
import type { Companion } from '../companion/client.ts';
import { jobsStore } from '../companion/jobs.ts';
import { createBoardInstall, type BoardInstallStore } from '../lib/device/new-board.ts';

export interface BoardsUiState {
  ignored: ReadonlySet<string>;
  started: Readonly<Record<string, NewBoard>>;
  ignore(id: string): void;
  restore(id: string): void;
  start(board: NewBoard): void;
  end(id: string): void;
}

export const boardsUi = createStore<BoardsUiState>()((set) => ({
  ignored: new Set(),
  started: {},
  ignore: (id) => set((s) => ({ ignored: new Set(s.ignored).add(id) })),
  restore: (id) =>
    set((s) => {
      const ignored = new Set(s.ignored);
      ignored.delete(id);
      return { ignored };
    }),
  start: (board) => set((s) => ({ started: { ...s.started, [board.id]: board } })),
  end: (id) =>
    set((s) => {
      const { [id]: _ended, ...started } = s.started;
      return { started };
    }),
}));

export function useBoardsUi<T>(select: (state: BoardsUiState) => T): T {
  return useStore(boardsUi, select);
}

export interface BoardCards {
  shown: NewBoard[];
  ignored: NewBoard[];
}

export function boardCards(boards: readonly NewBoard[], devices: readonly Pick<DeviceSummary, 'id'>[], ui: Pick<BoardsUiState, 'ignored' | 'started'>): BoardCards {
  const known = new Set(devices.map((d) => d.id));
  const listed = new Set(boards.map((b) => b.id));
  const installing = Object.values(ui.started).filter((b) => !known.has(b.id));
  return {
    shown: [...boards.filter((b) => !ui.ignored.has(b.id) || ui.started[b.id]), ...installing.filter((b) => !listed.has(b.id))].filter((b) => !known.has(b.id)),
    ignored: boards.filter((b) => ui.ignored.has(b.id) && !ui.started[b.id] && !known.has(b.id)),
  };
}

const installs = new WeakMap<Companion, Map<string, BoardInstallStore>>();

export function boardInstallFor(companion: Companion, board: Pick<NewBoard, 'id'>): BoardInstallStore {
  let byBoard = installs.get(companion);
  if (!byBoard) {
    byBoard = new Map();
    installs.set(companion, byBoard);
  }
  let found = byBoard.get(board.id);
  if (!found) {
    found = createBoardInstall({ companion, jobs: jobsStore, board });
    byBoard.set(board.id, found);
  }
  return found;
}
