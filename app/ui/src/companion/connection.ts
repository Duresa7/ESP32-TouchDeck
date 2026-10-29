import { createContext, useContext } from 'react';
import { createClient, type Companion } from './client.ts';
import { devicesStore, type DevicesStore } from './devices.ts';
import { openEvents, openWebSocket, type Events, type OpenSocket } from './events.ts';
import { jobsStore, type JobsStore } from './jobs.ts';
import { pickSession, type Session } from './session.ts';

/** Where the UI finds its Companion: the page's own origin, unless the demo fakes one. */
export interface Environment {
  base: string;
  session: Session;
  open: OpenSocket;
}

export function pageEnvironment(target: Window): Environment {
  return { base: target.location.origin, session: pickSession(target), open: openWebSocket };
}

export function eventsUrl(base: string): string {
  const url = new URL('/api/v1/events', base);
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  return url.href;
}

export interface Connection {
  companion: Companion;
  events: Events;
}

/** Starts the client and the event stream that feeds the Devices store. */
export function connect(
  env: Environment,
  store: DevicesStore = devicesStore,
  onNewBuild = () => window.location.reload(),
  jobs: JobsStore = jobsStore,
): Connection {
  return {
    companion: createClient(env.base, env.session),
    events: openEvents({ url: eventsUrl(env.base), session: env.session, store, jobs, open: env.open, onNewBuild }),
  };
}

export const CompanionContext = createContext<Companion | null>(null);

export function useCompanion(): Companion {
  const companion = useContext(CompanionContext);
  if (!companion) throw new Error('useCompanion needs a CompanionContext provider');
  return companion;
}
