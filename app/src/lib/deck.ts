import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

interface Hello {
  product: string;
  proto: number;
  elf_sha256: string;
  version: string;
  running: string;
  sinks: string[];
  recovery: boolean;
  layout: string;
}

export const HELLO_FIELDS = ['product', 'proto', 'elf_sha256', 'version', 'running', 'sinks', 'recovery', 'layout'] as const;

export type Candidate = { port: string } & (
  | { status: 'deck'; hello: Hello }
  | { status: 'silent' }
  | { status: 'busy' | 'not-deck' | 'unavailable'; detail: string }
);

export interface DeckInfo {
  session: number;
  port: string;
  hello: Hello;
  online: boolean;
}

interface LinkClosed {
  session: number;
  port: string;
  reason: string;
}

export const listDecks = () => invoke<Candidate[]>('list_decks');
export const connect = (port: string) => invoke<DeckInfo>('connect', { port });
export const disconnect = () => invoke<void>('disconnect');
export const deckInfo = () => invoke<DeckInfo | null>('deck_info');
export const consoleSend = (line: string) => invoke<void>('console_send', { line });

export const onConsoleLine = (handler: (line: string) => void): Promise<UnlistenFn> =>
  listen<string>('console-line', (event) => handler(event.payload));
export const onLinkClosed = (handler: (closed: LinkClosed) => void): Promise<UnlistenFn> =>
  listen<LinkClosed>('link-closed', (event) => handler(event.payload));

export function describe(candidate: Candidate): string {
  switch (candidate.status) {
    case 'deck':
      return `TouchDeck ${candidate.hello.version}`;
    case 'silent':
      return 'No answer to hello';
    case 'busy':
      return 'In use by another program';
    default:
      return candidate.detail;
  }
}
