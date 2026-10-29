// The Companion's actions and approvals routes and events, written by hand from docs/companion-api.md ("Actions and
// approvals", the event catalogue and "Command approvals happen on the PC") until #76 adds them to api.gen.ts. When it
// lands, its generated types replace this file and createClient drops the merge.
import type { DeviceId, DeviceKeyMirror } from './api.gen.ts';

/** A press the Companion's native dialog on the PC is asking about. No route answers it. */
export interface ApprovalRequest {
  id: string;
  device: DeviceId;
  page: string;
  tile: string;
  hash: string;
  kind: string;
  program: string;
  args: string[];
  cwd: string;
  shell: boolean;
  created_ms: number;
}

export type ApprovalAnswer = 'once' | 'always' | 'deny' | 'expired';

export interface ApprovalResolved {
  id: string;
  answer: ApprovalAnswer;
}

/** What happened to one press of a Tile that opens an app, opens a file or runs a command. */
export interface ActionLog {
  id: string;
  device: DeviceId;
  at_ms: number;
  page: string;
  tile: string;
  kind: string;
  status: string;
  detail: string;
  pid: number | null;
  exit_code: number | null;
}

/** An action someone chose Always allow for in the dialog on the PC. */
export interface AllowedAction {
  hash: string;
  kind: string;
  program: string;
  args: string[];
  cwd: string;
  shell: boolean;
  granted_ms: number;
}

export type KeyMirror = DeviceKeyMirror;

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === 'object' && value !== null && !Array.isArray(value);
const isStrings = (value: unknown): value is string[] => Array.isArray(value) && value.every((item) => typeof item === 'string');
const isNumberOrNull = (value: unknown) => value === null || typeof value === 'number';

export function isApprovalRequest(value: unknown): value is ApprovalRequest {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    typeof value.device === 'string' &&
    typeof value.page === 'string' &&
    typeof value.tile === 'string' &&
    typeof value.hash === 'string' &&
    typeof value.kind === 'string' &&
    typeof value.program === 'string' &&
    isStrings(value.args) &&
    typeof value.cwd === 'string' &&
    typeof value.shell === 'boolean' &&
    typeof value.created_ms === 'number'
  );
}

const ANSWERS: readonly unknown[] = ['once', 'always', 'deny', 'expired'] satisfies ApprovalAnswer[];

export function isApprovalResolved(value: unknown): value is ApprovalResolved {
  return isRecord(value) && typeof value.id === 'string' && ANSWERS.includes(value.answer);
}

export function isActionLog(value: unknown): value is ActionLog {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    typeof value.device === 'string' &&
    typeof value.at_ms === 'number' &&
    typeof value.page === 'string' &&
    typeof value.tile === 'string' &&
    typeof value.kind === 'string' &&
    typeof value.status === 'string' &&
    typeof value.detail === 'string' &&
    isNumberOrNull(value.pid) &&
    isNumberOrNull(value.exit_code)
  );
}

export function isKeyMirror(value: unknown): value is KeyMirror {
  if (!isRecord(value)) return false;
  const gap = value.gap;
  return (
    typeof value.device === 'string' &&
    typeof value.seq === 'number' &&
    typeof value.report === 'string' &&
    typeof value.outcome === 'string' &&
    (gap === null || (Array.isArray(gap) && gap.length === 2 && gap.every((n) => typeof n === 'number'))) &&
    isStrings(value.errors) &&
    typeof value.arrived_ms === 'number'
  );
}

/** The `approvals` key of a `sync` frame, keeping only well-formed requests. */
export function syncedApprovals(data: unknown): ApprovalRequest[] {
  return isRecord(data) && Array.isArray(data.approvals) ? data.approvals.filter(isApprovalRequest) : [];
}

/** `companion.approvals.*` and `companion.actions.*`, shaped like the generated client so #76's can replace it. */
export function createActionsApi(base: string, token: () => string) {
  async function send(method: string, route: string, query: Record<string, string> = {}): Promise<Response> {
    const url = new URL(`/api/v1${route}`, base);
    for (const [key, value] of Object.entries(query)) url.searchParams.set(key, value);
    const response = await fetch(url, { method, headers: { Authorization: `Bearer ${token()}` } });
    if (!response.ok) throw (await response.json()).error;
    return response;
  }
  const read = async <T>(answer: Promise<Response>): Promise<T> => (await answer).json();
  const part = (value: string) => encodeURIComponent(value);
  return {
    approvals: {
      list: () => read<ApprovalRequest[]>(send('GET', '/approvals')),
      show: async (path: { id: string }) => void (await send('POST', `/approvals/${part(path.id)}/show`)),
    },
    actions: {
      log: (_path: Record<string, never>, query: { device: DeviceId }) => read<ActionLog[]>(send('GET', '/actions/log', query)),
      allowed: () => read<AllowedAction[]>(send('GET', '/actions/allowed')),
      allowed_remove: async (path: { hash: string }) => void (await send('DELETE', `/actions/allowed/${part(path.hash)}`)),
    },
  };
}
