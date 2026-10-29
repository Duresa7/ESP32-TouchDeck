import { useStore } from 'zustand';
import { createStore } from 'zustand/vanilla';
import type { ActionLog, AllowedAction, ApprovalRequest, ApprovalResolved, KeyMirror } from './actions-api.ts';

const LOG_KEPT = 100;
const MIRROR_KEPT = 50;

/** What the event stream has said about actions: pending approvals, the Action log and the key mirror. */
export interface ActionsState {
  /** Requests the dialog on the PC is asking about, oldest first. */
  approvals: ApprovalRequest[];
  /** Ids already answered or expired, so a late `approvals.list` can't bring one back. */
  resolved: string[];
  /** Each Device's last presses, oldest first. */
  log: Record<string, ActionLog[]>;
  /** Always-allowed actions, or null before the first read. */
  allowed: AllowedAction[] | null;
  /** Counts `always` answers, so a view of the allowed list knows to read it again. */
  allowedRev: number;
  /** Each Device's last mirrored key reports, oldest first. */
  mirror: Record<string, KeyMirror[]>;
  /** How many views want each Device's `key-mirror` topic. */
  watching: Record<string, number>;
  /** `sync`: the snapshot replaces the pending list. */
  syncApprovals(approvals: ApprovalRequest[]): void;
  /** `approvals.list`: adds what the stream hasn't reported, unless it was resolved since. */
  listed(approvals: ApprovalRequest[]): void;
  requested(request: ApprovalRequest): void;
  answered(resolved: ApprovalResolved): void;
  /** `actions.log`: a Device's last presses, merged with any the stream already sent. */
  loadLog(device: string, entries: ActionLog[]): void;
  logged(entry: ActionLog): void;
  setAllowed(allowed: AllowedAction[]): void;
  mirrored(report: KeyMirror): void;
  clearMirror(device: string): void;
  /** Asks the stream for a Device's key mirror until the returned function is called. */
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
