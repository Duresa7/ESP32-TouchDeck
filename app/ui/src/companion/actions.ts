import { useStore } from 'zustand';
import { createStore } from 'zustand/vanilla';
import type { ActionLog, AllowedAction, ApprovalRequest, ApprovalResolved, DeviceKeyMirror } from './api.ts';

const LOG_KEPT = 100;
const MIRROR_KEPT = 50;

export interface ActionsState {
  approvals: ApprovalRequest[];
  resolved: string[];
  log: Record<string, ActionLog[]>;
  allowed: AllowedAction[] | null;
  allowedRev: number;
  mirror: Record<string, DeviceKeyMirror[]>;
  watching: Record<string, number>;
  syncApprovals(approvals: ApprovalRequest[]): void;
  listed(approvals: ApprovalRequest[]): void;
  requested(request: ApprovalRequest): void;
  answered(resolved: ApprovalResolved): void;
  loadLog(device: string, entries: ActionLog[]): void;
  logged(entry: ActionLog): void;
  setAllowed(allowed: AllowedAction[]): void;
  mirrored(report: DeviceKeyMirror): void;
  clearMirror(device: string): void;
  watchKeys(device: string): () => void;
}

const byId = (entries: ActionLog[]) => [...new Map(entries.map((entry) => [entry.id, entry])).values()];

export function createActionsStore() {
  return createStore<ActionsState>()((set) => ({
    approvals: [],
    resolved: [],
    log: {},
    allowed: null,
    allowedRev: 0,
    mirror: {},
    watching: {},
    syncApprovals: (approvals) => set({ approvals, resolved: [] }),
    listed: (approvals) =>
      set((state) => {
        const known = new Set(state.approvals.map((request) => request.id));
        const fresh = approvals.filter((request) => !known.has(request.id) && !state.resolved.includes(request.id));
        return fresh.length ? { approvals: [...state.approvals, ...fresh] } : state;
      }),
    requested: (request) =>
      set((state) => (state.approvals.some((known) => known.id === request.id) ? state : { approvals: [...state.approvals, request] })),
    answered: ({ id, answer }) =>
      set((state) => ({
        approvals: state.approvals.filter((request) => request.id !== id),
        resolved: [...state.resolved.slice(-(LOG_KEPT - 1)), id],
        allowedRev: answer === 'always' ? state.allowedRev + 1 : state.allowedRev,
      })),
    loadLog: (device, entries) =>
      set((state) => ({
        log: { ...state.log, [device]: byId([...entries, ...(state.log[device] ?? [])]).sort((a, b) => a.at_ms - b.at_ms).slice(-LOG_KEPT) },
      })),
    logged: (entry) =>
      set((state) => ({ log: { ...state.log, [entry.device]: byId([...(state.log[entry.device] ?? []), entry]).slice(-LOG_KEPT) } })),
    setAllowed: (allowed) => set({ allowed }),
    mirrored: (report) =>
      set((state) => ({ mirror: { ...state.mirror, [report.device]: [...(state.mirror[report.device] ?? []).slice(-(MIRROR_KEPT - 1)), report] } })),
    clearMirror: (device) => set((state) => ({ mirror: { ...state.mirror, [device]: [] } })),
    watchKeys: (device) => {
      set((state) => ({ watching: { ...state.watching, [device]: (state.watching[device] ?? 0) + 1 } }));
      let done = false;
      return () => {
        if (done) return;
        done = true;
        set((state) => {
          const { [device]: count = 0, ...rest } = state.watching;
          return { watching: count > 1 ? { ...rest, [device]: count - 1 } : rest };
        });
      };
    },
  }));
}

export type ActionsStore = ReturnType<typeof createActionsStore>;

export const actionsStore = createActionsStore();

export function useActions<T>(select: (state: ActionsState) => T): T {
  return useStore(actionsStore, select);
}
