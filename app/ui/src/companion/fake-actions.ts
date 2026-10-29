import { isWebAddress, processOf } from '../lib/editor/actions.ts';
import { parseLayout, type Orientation } from '../lib/editor/validate.ts';
import type { ActionLog, AllowedAction, Answer, ApprovalRequest, DeviceSummary } from './api.ts';
import { revisionOf } from './fake-layouts.ts';

type Frame = Record<string, unknown> & { type: string };
type Scripted = Exclude<Answer, 'expired'> | 'fail' | 'never';

export interface FakeActionsOptions {
  devices: Map<string, DeviceSummary>;
  publish(frame: Frame): void;
  publishTopic(topic: string, device: string, frame: Frame): void;
  stored(device: string, orientation: Orientation): string | null;
}

const json = (status: number, body: unknown) =>
  new Response(status === 204 ? null : JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

const failure = (status: number, code: string, message: string) => json(status, { error: { code, message, details: null } });

const SCRIPTED: readonly string[] = ['once', 'always', 'deny', 'fail', 'never'];
const PROMPT_FAILED = 'The approval prompt failed, so the action was denied: The scripted prompt failed.';
const LOG_KEPT = 100;

export function createFakeActions(options: FakeActionsOptions) {
  const { devices, publish, publishTopic, stored } = options;
  const pending: ApprovalRequest[] = [];
  const allowed = new Map<string, AllowedAction>();
  const log: ActionLog[] = [];
  const script: Scripted[] = [];
  const mirrorSeq = new Map<string, number>();
  let minted = 0;
  let pid = 4100;
  let shown = 0;

  function record(entry: Omit<ActionLog, 'id' | 'at_ms'>) {
    minted += 1;
    const logged: ActionLog = { id: `log_${minted}`, at_ms: Date.now(), ...entry };
    log.push(logged);
    log.splice(0, Math.max(0, log.length - LOG_KEPT));
    publish({ type: 'action.logged', data: logged });
  }

  function run(device: string, page: string, tile: string, started: Pick<ApprovalRequest, 'kind' | 'program'>, detail = '') {
    pid += 1;
    const base = { device, page, tile, kind: started.kind };
    record({ ...base, status: 'started', detail, pid, exit_code: null });
    if (started.kind !== 'open') setTimeout(() => record({ ...base, status: 'exited', detail: '', pid, exit_code: 0 }), 20);
  }

  function resolve(request: ApprovalRequest, answer: Exclude<Answer, 'expired'>, detail = '') {
    const at = pending.indexOf(request);
    if (at < 0) return;
    pending.splice(at, 1);
    publish({ type: 'approval.resolved', data: { id: request.id, answer } });
    answerNext();
    if (answer === 'deny') {
      record({ device: request.device, page: request.page, tile: request.tile, kind: request.kind, status: 'denied', detail, pid: null, exit_code: null });
      return;
    }
    if (answer === 'always') {
      const { hash, kind, program, args, cwd, shell } = request;
      allowed.set(hash, { hash, kind, program, args, cwd, shell, granted_ms: Date.now() });
    }
    run(request.device, request.page, request.tile, request, answer === 'always' ? 'Always allowed' : 'Allowed once');
  }

  function answerNext() {
    const request = pending[0];
    const answer = script[0];
    if (!request || answer === undefined) return;
    script.shift();
    if (answer === 'fail') setTimeout(() => resolve(request, 'deny', PROMPT_FAILED), 0);
    else if (answer !== 'never') setTimeout(() => resolve(request, answer), 0);
  }

  function press(device: string, page: string, tile: string, kind: string) {
    const summary = devices.get(device);
    const orientation: Orientation = summary?.orientation === 'portrait' ? 'portrait' : 'landscape';
    const text = stored(device, orientation);
    const parsed = text === null ? null : parseLayout(text);
    const grid = parsed?.ok ? parsed.layout.pages.find((p) => p.id === page) : undefined;
    const found = grid?.kind === 'grid' ? grid.tiles.find((t) => t.id === tile) : undefined;
    if (!found) {
      record({ device, page, tile, kind, status: 'unknown', detail: "no such tile in the Device's Layout", pid: null, exit_code: null });
      return;
    }
    const process = processOf(found.action);
    if (!process) return;
    if (process.kind === 'open' && isWebAddress(process.program)) {
      run(device, page, tile, process);
      return;
    }
    const hash = revisionOf(JSON.stringify([process.kind, process.program, process.args, process.cwd, process.shell]));
    if (allowed.has(hash)) {
      run(device, page, tile, process, 'Always allowed');
      return;
    }
    if (pending.some((request) => request.device === device && request.hash === hash)) return;
    minted += 1;
    const request: ApprovalRequest = { id: `apr_${minted}`, device, page, tile, hash, ...process, created_ms: Date.now() };
    pending.push(request);
    publish({ type: 'approval.requested', data: request });
    if (pending.length === 1) answerNext();
  }

  function mirror(device: string, event: Record<string, unknown>) {
    const seq = (mirrorSeq.get(device) ?? 0) + 1;
    mirrorSeq.set(device, seq);
    const report = event.type === 'con' ? `con ${event.usage}` : `kbd ${event.mods ?? 0} [${Array.isArray(event.keys) ? event.keys.join(', ') : ''}]`;
    const outcome = pending.length ? 'suppressed' : 'typed';
    publishTopic('key-mirror', device, {
      type: 'device.key-mirror',
      device,
      data: { device, seq, report, outcome, gap: null, errors: [], arrived_ms: Date.now() },
    });
  }

  return {
    handle(request: Request, path: string): Response | null {
      const method = request.method;
      let match: RegExpExecArray | null;
      if (path === '/approvals') return method === 'GET' ? json(200, pending) : failure(405, 'route.method_not_allowed', 'Use GET.');
      if ((match = /^\/approvals\/([^/]+)\/show$/.exec(path))) {
        if (method !== 'POST') return failure(405, 'route.method_not_allowed', 'Use POST.');
        const id = decodeURIComponent(match[1]);
        if (!pending.some((r) => r.id === id)) return failure(404, 'approval.not_found', 'The request was answered or expired.');
        shown += 1;
        return json(204, null);
      }
      if (path === '/actions/log') {
        if (method !== 'GET') return failure(405, 'route.method_not_allowed', 'Use GET.');
        const device = new URL(request.url).searchParams.get('device');
        return json(200, log.filter((entry) => device === null || entry.device === device));
      }
      if (path === '/actions/allowed') return method === 'GET' ? json(200, [...allowed.values()]) : failure(405, 'route.method_not_allowed', 'Use GET.');
      if ((match = /^\/actions\/allowed\/([^/]+)$/.exec(path))) {
        if (method !== 'DELETE') return failure(405, 'route.method_not_allowed', 'Use DELETE.');
        return allowed.delete(decodeURIComponent(match[1])) ? json(204, null) : failure(404, 'action.not_found', 'No always-allowed action with that hash.');
      }
      return null;
    },

    async test(request: Request, path: string): Promise<Response | null> {
      let match: RegExpExecArray | null;
      if (request.method !== 'POST') return null;
      if ((match = /^\/test\/devices\/([^/]+)\/press$/.exec(path))) {
        const body: { page?: unknown; tile?: unknown; kind?: unknown } = await request.json();
        press(decodeURIComponent(match[1]), String(body.page), String(body.tile), String(body.kind ?? ''));
        return json(204, null);
      }
      if (path === '/test/approvals/script') {
        const body: { answers?: unknown } = await request.json();
        const answers = Array.isArray(body.answers) ? body.answers : [];
        if (!answers.every((answer): answer is Scripted => typeof answer === 'string' && SCRIPTED.includes(answer))) {
          return failure(400, 'request.invalid', 'Answers are once, always, deny, fail or never.');
        }
        script.splice(0, script.length, ...answers);
        answerNext();
        return json(204, null);
      }
      if ((match = /^\/test\/devices\/([^/]+)\/emit$/.exec(path))) {
        const body: { events?: unknown } = await request.json();
        const device = decodeURIComponent(match[1]);
        for (const event of Array.isArray(body.events) ? body.events : []) {
          if (typeof event === 'object' && event !== null && event.m === 'key.report') mirror(device, event);
        }
        return json(204, null);
      }
      return null;
    },

    pending: () => [...pending],
    get shown() {
      return shown;
    },
    press,
  };
}

export type FakeActions = ReturnType<typeof createFakeActions>;
