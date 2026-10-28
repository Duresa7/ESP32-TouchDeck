// The link to the deck, shared by the top bar, the editor, and Troubleshooting: which decks are plugged in, which one
// is connected, and the deck console's lines.
import { tick } from 'svelte';
import { derived, get, writable } from 'svelte/store';
import { append, classify, type ConsoleEntry } from './console.ts';
import { connect, consoleSend, deckInfo, disconnect, listDecks, onConsoleLine, onLinkClosed, type Candidate, type DeckInfo } from './deck.ts';

export const decks = writable<Candidate[]>([]);
export const listed = writable(false);
export const scanning = writable(false);
export const deck = writable<DeckInfo | null>(null);
export const connecting = writable<string | null>(null);
/** The technical reason the last connect, list, or session failed. */
export const problem = writable<string | null>(null);
/** Whether the session ended on its own, rather than by Disconnect. */
export const lost = writable(false);
export const entries = writable<ConsoleEntry[]>([]);

export const online = derived(deck, ($deck) => $deck !== null && $deck.online);

export function connectable(candidate: Candidate) {
  return candidate.status === 'deck' || candidate.status === 'silent';
}

export async function refresh() {
  scanning.set(true);
  try {
    decks.set(await listDecks());
  } catch (error) {
    problem.set(String(error));
  } finally {
    scanning.set(false);
  }
  if (!get(listed)) {
    listed.set(true);
    await tick();
    performance.mark('list-ready');
  }
}

export async function connectTo(port: string) {
  connecting.set(port);
  problem.set(null);
  lost.set(false);
  performance.mark('connect-start');
  try {
    deck.set(await connect(port));
    await tick();
    performance.mark('connected');
  } catch (error) {
    problem.set(String(error));
  } finally {
    connecting.set(null);
  }
}

export async function leave() {
  lost.set(false);
  await disconnect();
}

export async function sendConsole(text: string) {
  entries.update((list) => append(list, { kind: 'sent', text }));
  try {
    await consoleSend(text);
  } catch (error) {
    problem.set(String(error));
  }
}

/** Starts listening for console lines and a lost session, and lists the decks. Returns the stop function. */
export function start(): () => void {
  const listeners = [
    onConsoleLine((text) => entries.update((list) => append(list, classify(text)))),
    onLinkClosed((closed) => {
      if (get(deck)?.session !== closed.session) return;
      deck.set(null);
      problem.set(closed.reason);
      lost.set(true);
      refresh();
    }),
  ];
  deckInfo().then((info) => {
    if (info) deck.set(info);
  });
  refresh();
  return () => listeners.forEach((listener) => listener.then((stop) => stop()));
}

/** What a found port is, in plain words. */
export function describeCandidate(candidate: Candidate): string {
  switch (candidate.status) {
    case 'deck':
      return `TouchDeck, software ${candidate.hello.version}`;
    case 'silent':
      return "Didn't answer yet. It may still be starting up.";
    case 'busy':
      return 'Another program is using it. Close that program, then look again.';
    case 'not-deck':
      return 'Not a TouchDeck';
    default:
      return "Can't be opened right now";
  }
}

/** The connection error in plain words; the raw reason stays available as detail. */
export function friendlyConnection(reason: string): string {
  const text = reason.toLowerCase();
  if (text.includes('access') || text.includes('denied') || text.includes('busy') || text.includes('in use')) {
    return 'Another program is using the deck. Close it, then connect again.';
  }
  if (text.includes('timed out') || text.includes('timeout') || text.includes('no reply') || text.includes('did not answer')) {
    return "The deck didn't answer. Unplug it, plug it back in, then connect again.";
  }
  return 'The connection to your deck dropped. Check the USB cable, then connect again.';
}
