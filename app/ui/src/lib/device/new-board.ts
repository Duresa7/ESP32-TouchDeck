import type { NewBoard } from '../../companion/api.ts';
import type { Companion } from '../../companion/client.ts';
import type { JobsStore } from '../../companion/jobs.ts';
import { createFirmwareJob, type FirmwareWords, type PhaseProgress, type UpdateView } from './firmware.ts';

export type BoardStep = 'detected' | 'waiting' | 'installing' | 'done' | 'declined' | 'failed';

export const DECLINED = 'Declined on this PC. Nothing changed on the board.';
export const INSTALL_FAILED = "Couldn't install Firmware. Unplug the board, plug it back in, then retry.";

function installProgress(progress: PhaseProgress | null): string {
  const writing = progress?.phase === 'sending' || progress?.phase === 'recovering';
  return writing && progress.pct !== null ? `Installing Firmware… ${Math.round(progress.pct)}%` : 'Installing Firmware…';
}

export const installWords: FirmwareWords = {
  waiting: 'Confirm on this PC to continue.',
  progress: installProgress,
  done: () => 'Firmware installed',
  failed: (error) => (error.code === 'firmware.declined' ? DECLINED : INSTALL_FAILED),
};

export function boardStep(view: UpdateView): BoardStep {
  switch (view.step) {
    case 'idle':
    case 'cancelled':
      return 'detected';
    case 'waiting':
      return 'waiting';
    case 'running':
      return 'installing';
    case 'done':
      return 'done';
    case 'failed':
      return view.code === 'firmware.declined' ? 'declined' : 'failed';
    default: {
      const _exhaustive: never = view;
      return _exhaustive;
    }
  }
}

export interface BoardInstallDeps {
  companion: Pick<Companion, 'firmware' | 'jobs'>;
  jobs: JobsStore;
  board: Pick<NewBoard, 'id'>;
  pollMs?: number;
}

export function createBoardInstall({ companion, jobs, board, pollMs }: BoardInstallDeps) {
  return createFirmwareJob({
    companion,
    jobs,
    pollMs,
    words: installWords,
    context: () => ({ id: board.id, device: 'the board', version: '' }),
    run: (id, bundle) => companion.firmware.install({ device: id }, null, { bundle }),
  });
}

export type BoardInstallStore = ReturnType<typeof createBoardInstall>;
