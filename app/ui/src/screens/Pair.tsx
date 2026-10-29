import { useEffect, useRef, useState, type FormEvent } from 'react';
import { createCompanion } from '../companion/api.ts';
import { BROWSER_SESSION_KEY } from '../companion/session.ts';
import './pair.css';

export function takePairingCode(location: Location, history: History): string {
  const fragment = new URLSearchParams(location.hash.slice(1)).get('code');
  const query = new URLSearchParams(location.search).get('code');
  history.replaceState(null, '', '/pair');
  return fragment ?? query ?? '';
}

function browserLabel(): string {
  const ua = navigator.userAgent;
  const browser = ua.includes('Edg/') ? 'Edge' : ua.includes('Firefox/') ? 'Firefox' : ua.includes('Chrome/') ? 'Chrome' : 'Browser';
  return ua.includes('Windows') ? `${browser} on Windows` : browser;
}

type PairState = { kind: 'idle' } | { kind: 'pairing' } | { kind: 'failed'; message: string };
export function Pair({ initialCode }: { initialCode: string }) {
  const [code, setCode] = useState(initialCode);
  const [state, setState] = useState<PairState>({ kind: 'idle' });
  const started = useRef(false);
  async function pair(value: string) {
    setState({ kind: 'pairing' });
    try {
      const result = await createCompanion(location.origin, () => '').auth.pair({}, null, { code: value, label: browserLabel() });
      if (!/^tds_[A-Za-z0-9_-]{43}$/.test(result.token)) throw new Error('Invalid session');
      localStorage.setItem(BROWSER_SESSION_KEY, result.token);
      location.replace('/');
    } catch (error: unknown) {
      const errorCode = typeof error === 'object' && error !== null && 'code' in error ? error.code : null;
      const message = errorCode === 'auth.code_locked'
        ? 'Pairing was blocked after wrong codes. Start pairing again.'
        : errorCode === 'auth.code_invalid'
          ? 'This pairing code has expired or is wrong. Choose Open in browser from the tray icon again.'
          : "Can't reach the Companion. Check that Open TouchDeck is running in the tray, then retry.";
      setState({ kind: 'failed', message });
    }
  }
  useEffect(() => {
    if (initialCode && !started.current) { started.current = true; void pair(initialCode); }
  }, [initialCode]);
  function submit(event: FormEvent) { event.preventDefault(); void pair(code); }
  return <main className="pair-page"><form onSubmit={submit} className="pair-form">
    <h1>Pair this browser</h1>
    <p>Enter the code Open TouchDeck shows on this PC.</p>
    <label htmlFor="pair-code">Code</label>
    <input id="pair-code" autoComplete="off" spellCheck={false} value={code} maxLength={64}
      onChange={event => setCode(event.target.value)} disabled={state.kind === 'pairing'} />
    <button type="submit" disabled={!code.trim() || state.kind === 'pairing'}>{state.kind === 'pairing' ? 'Pairing…' : 'Pair'}</button>
    {state.kind === 'failed' && <p role="alert">{state.message}</p>}
    <p>Pair this browser again after the Companion restarts.</p>
  </form></main>;
}
