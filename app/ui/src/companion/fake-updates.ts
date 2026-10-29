import type { UpdateOffer, UpdatesBridge } from '../state/updates.ts';
import type { UpdateCheck, UpdateDownload, UpdateInstall } from './session.ts';

export interface FakeUpdatesScript {
  check: UpdateCheck;
  download: UpdateDownload;
  /** `quit` stands for a successful install: the call never resolves, as the app would quit. */
  install: UpdateInstall | 'quit';
  /** The installer's size in bytes. Null sends progress without a total. */
  size: number | null;
  /** The download pauses at this fraction until `finish()`. */
  holdAt: number | null;
  stepMs: number;
}

export const sampleOffer: UpdateOffer = {
  state: 'available',
  version: '1.1.0',
  current: '0.2.0',
  notes: '## Changes\n- Volume Tiles follow your finger as you drag.\n- Proxmox Tiles can show storage use.\n- Devices reconnect faster after this PC wakes.',
  date: '2026-11-02T09:00:00Z',
};

const STEPS = 10;
const INSTALLER_BYTES = 64_000_000;

const pause = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

export function createFakeUpdates(initial: Partial<FakeUpdatesScript> = {}) {
  let script: FakeUpdatesScript = { check: { state: 'none' }, download: { state: 'ready' }, install: 'quit', size: INSTALLER_BYTES, holdAt: null, stepMs: 40, ...initial };
  let release: (() => void) | null = null;
  const calls = { check: 0, download: 0, install: 0 };

  const bridge: UpdatesBridge = {
    async check() {
      calls.check += 1;
      await pause(script.stepMs);
      return structuredClone(script.check);
    },
    async download(onProgress) {
      calls.download += 1;
      const { size, holdAt, stepMs } = script;
      const bytes = size ?? INSTALLER_BYTES;
      for (let step = 1; step <= STEPS; step += 1) {
        const fraction = step / STEPS;
        if (holdAt !== null && fraction > holdAt) {
          onProgress({ downloaded: Math.round(bytes * holdAt), total: size });
          await new Promise<void>((resolve) => {
            release = resolve;
          });
          break;
        }
        await pause(stepMs);
        onProgress({ downloaded: Math.round(bytes * fraction), total: size });
      }
      return structuredClone(script.download);
    },
    install() {
      calls.install += 1;
      const { install } = script;
      return install === 'quit' ? new Promise<never>(() => {}) : pause(script.stepMs).then(() => structuredClone(install));
    },
  };

  return {
    bridge,
    calls,
    script(next: Partial<FakeUpdatesScript>) {
      script = { ...script, ...next };
    },
    /** Lets a held download finish, with whatever outcome is scripted by then. */
    finish() {
      release?.();
      release = null;
    },
  };
}

export type FakeUpdates = ReturnType<typeof createFakeUpdates>;
