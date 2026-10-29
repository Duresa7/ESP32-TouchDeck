import { createStore } from 'zustand/vanilla';
import type { Job, JobMap } from './api.ts';
import type { Companion } from './client.ts';
import { ApiError, toApiError } from './errors.ts';

export interface JobsState {
  jobs: Record<string, Job>;
  sync(jobs: Job[]): void;
  upsert(job: Job): void;
}

export function createJobsStore() {
  return createStore<JobsState>()((set) => ({
    jobs: {},
    sync: (jobs) => set({ jobs: Object.fromEntries(jobs.map((job) => [job.id, job])) }),
    upsert: (job) =>
      set((state) => {
        const known = state.jobs[job.id];
        if (known && known.rev >= job.rev) return state;
        return { jobs: { ...state.jobs, [job.id]: job } };
      }),
  }));
}

export type JobsStore = ReturnType<typeof createJobsStore>;

export const jobsStore = createJobsStore();

const finished = (job: Job) => job.state === 'succeeded' || job.state === 'failed' || job.state === 'cancelled';

function outcome<K extends keyof JobMap>(job: Job): JobMap[K]['result'] {
  if (job.state === 'succeeded') return job.result as JobMap[K]['result'];
  if (job.error) throw toApiError(job.error);
  throw new ApiError(job.state === 'cancelled' ? 'job.cancelled' : 'internal', `The Job ended as ${job.state}.`);
}

export function waitForJob<K extends keyof JobMap>(
  store: JobsStore,
  companion: Pick<Companion, 'jobs'>,
  job: Job,
  pollMs = 2000,
): Promise<JobMap[K]['result']> {
  return new Promise((resolve, reject) => {
    let done = false;
    const settle = (latest: Job) => {
      if (done || !finished(latest)) return;
      done = true;
      unsubscribe();
      clearInterval(timer);
      try {
        resolve(outcome<K>(latest));
      } catch (error) {
        reject(error);
      }
    };
    const unsubscribe = store.subscribe((state) => {
      const latest = state.jobs[job.id];
      if (latest) settle(latest);
    });
    const timer = setInterval(() => {
      companion.jobs.get({ job: job.id }).then(
        (latest) => {
          store.getState().upsert(latest);
          settle(latest);
        },
        () => {},
      );
    }, pollMs);
    store.getState().upsert(job);
    settle(store.getState().jobs[job.id] ?? job);
  });
}
