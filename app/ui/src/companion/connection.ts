import { createContext, useContext } from 'react';
import { actionsStore, type ActionsStore } from './actions.ts';
import { createClient, type Companion } from './client.ts';
import { devicesStore, type DevicesStore } from './devices.ts';
import { openEvents, openWebSocket, type Events, type OpenSocket } from './events.ts';
import { integrationsStore, type IntegrationsStore } from './integrations.ts';
import { jobsStore, type JobsStore } from './jobs.ts';
import { pickSession, type Session } from './session.ts';
import type { UpdatesBridge } from '../state/updates.ts';

export interface Environment {
  base: string;
  session: Session;
  open: OpenSocket;
  updates?: UpdatesBridge;
  pickFolder?: PickFolder;
}

export function pageEnvironment(target: Window): Environment {
  const bridge = target.openTouchDeck;
  return { base: target.location.origin, session: pickSession(target), open: openWebSocket, updates: bridge?.updates, pickFolder: bridge?.pickFolder };
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

export function connect(
  env: Environment,
  store: DevicesStore = devicesStore,
  onNewBuild = () => window.location.reload(),
  jobs: JobsStore = jobsStore,
  actions: ActionsStore = actionsStore,
  integrations: IntegrationsStore = integrationsStore,
): Connection {
  return {
    companion: createClient(env.base, env.session),
    events: openEvents({ url: eventsUrl(env.base), session: env.session, store, jobs, actions, integrations, open: env.open, onNewBuild }),
  };
}

export const CompanionContext = createContext<Companion | null>(null);

export function useCompanion(): Companion {
  const companion = useContext(CompanionContext);
  if (!companion) throw new Error('useCompanion needs a CompanionContext provider');
  return companion;
}

export const EventsContext = createContext<Events | null>(null);

// Only the Electron window can open the system folder dialog. In a browser this is null.
export type PickFolder = () => Promise<string | null>;
export const FolderPickerContext = createContext<PickFolder | null>(null);

export function useFolderPicker(): PickFolder | null {
  return useContext(FolderPickerContext);
}

export function useEvents(): Events {
  const events = useContext(EventsContext);
  if (!events) throw new Error('useEvents needs an EventsContext provider');
  return events;
}
