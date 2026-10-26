import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface Hello {
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
  | { status: 'net-unpaired'; host: string; tcp_port: number; deck_id?: string; name?: string }
  | { status: 'net-paired'; host: string; tcp_port: number; deck_id: string; name?: string; hello?: Hello }
  | { status: 'silent' }
  | { status: 'busy' | 'not-deck' | 'unavailable'; detail: string }
);

export interface DeckInfo {
  session: number;
  port: string;
  hello: Hello;
  online: boolean;
}

export interface LinkClosed {
  session: number;
  port: string;
  reason: string;
}

export interface NetPairOffer {
  port: string;
  host: string;
  tcp_port: number;
  deck_id: string;
  name?: string;
  remaining_ms: number;
  attempts_left: number;
}

export interface NetPairStep {
  paired: boolean;
  locked: boolean;
  attempts_left: number;
  deck_id: string;
  port: string;
  error: string | null;
}

export const listDecks = () => invoke<Candidate[]>('list_decks');
export const connect = (port: string) => invoke<DeckInfo>('connect', { port });
export const disconnect = () => invoke<void>('disconnect');
export const deckInfo = () => invoke<DeckInfo | null>('deck_info');
export const consoleSend = (line: string) => invoke<void>('console_send', { line });

export const netAddHost = (host: string) => invoke<string>('net_add_host', { host });
export const netPairStart = (port: string, name?: string) => invoke<NetPairOffer>('net_pair_start', { port, name });
export const netPairCode = (code: string) => invoke<NetPairStep>('net_pair_code', { code });
export const netPairCancel = () => invoke<void>('net_pair_cancel');
export const netForgetDeck = (port: string, deckId?: string) =>
  invoke<void>('net_forget_deck', { port, deckId });

export const onConsoleLine = (handler: (line: string) => void): Promise<UnlistenFn> =>
  listen<string>('console-line', (event) => handler(event.payload));
export const onLinkClosed = (handler: (closed: LinkClosed) => void): Promise<UnlistenFn> =>
  listen<LinkClosed>('link-closed', (event) => handler(event.payload));

export function describe(candidate: Candidate): string {
  switch (candidate.status) {
    case 'deck':
      return `TouchDeck ${candidate.hello.version}`;
    case 'net-paired':
      return candidate.hello
        ? `TouchDeck ${candidate.hello.version} · Wi-Fi (${candidate.host})`
        : `${candidate.name ?? 'TouchDeck'} · Paired over Wi-Fi (${candidate.host})`;
    case 'net-unpaired':
      return `${candidate.name ?? 'TouchDeck'} · Wi-Fi (${candidate.host}) · Unpaired`;
    case 'silent':
      return 'No answer to hello';
    case 'busy':
      return 'In use by another program';
    default:
      return candidate.detail;
  }
}