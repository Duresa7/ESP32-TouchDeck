import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import { Dialog } from '@base-ui/react/dialog';
import { ChevronLeft, ChevronRight, KeyRound, X } from 'lucide-react';
import { useStore } from 'zustand';
import { brandfetchStore, DEVELOPER_PORTAL, logoCount, type BrandfetchStore } from '../../lib/brandfetch/settings.ts';
import { useCompanion } from '../../lib/companion/connection.ts';
import { useKeychainName } from '../../lib/companion/keychain.ts';
import { button } from '../button.ts';
import './brandfetch.css';

/** One setting that, once saved, shows only its state with Replace and Remove. */
function KeyField({
  label,
  hint,
  shown,
  secret,
  busy,
  testid,
  extra,
  onSave,
  onRemove,
}: {
  label: string;
  hint: ReactNode;
  /** What a saved value shows: the value itself, or "Set" for a secret. Null when unset. */
  shown: string | null;
  secret: boolean;
  busy: boolean;
  testid: string;
  /** More buttons beside Replace once the value is saved. */
  extra?: ReactNode;
  onSave(value: string): Promise<boolean>;
  onRemove(): Promise<boolean>;
}) {
  const id = useId();
  const input = useRef<HTMLInputElement>(null);
  const [replacing, setReplacing] = useState(false);
  const [filled, setFilled] = useState(false);
  const editing = shown === null || replacing;

  useEffect(() => {
    if (replacing) input.current?.focus();
  }, [replacing]);

  const save = async () => {
    const el = input.current;
    if (!el || !el.value.trim()) return;
    const value = el.value;
    // A secret leaves the field before the answer comes back.
    if (secret) el.value = '';
    setFilled(false);
    if (await onSave(value)) {
      if (input.current) input.current.value = '';
      setReplacing(false);
    }
  };

  const keys = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      void save();
    } else if (event.key === 'Escape' && replacing) {
      event.stopPropagation();
      setReplacing(false);
    }
  };

  return (
    <div className="field brandfetch-field" data-testid={testid} data-set={shown !== null}>
      <label className="field-label" htmlFor={editing ? id : undefined} id={`${id}-label`}>
        {label}
      </label>
      {editing ? (
        <div className="brandfetch-key-row">
          <input
            ref={input}
            id={id}
            type={secret ? 'password' : 'text'}
            className="input"
            placeholder="Not set"
            autoComplete="off"
            spellCheck={false}
            aria-describedby={`${id}-hint`}
            onInput={(event) => setFilled(event.currentTarget.value.trim().length > 0)}
            onKeyDown={keys}
            data-testid={`${testid}-input`}
          />
          <button type="button" className={button()} aria-disabled={!filled || busy} onClick={() => filled && !busy && void save()} data-testid={`${testid}-save`}>
            Save
          </button>
          {replacing && (
            <button type="button" className={button()} onClick={() => setReplacing(false)}>
              Cancel
            </button>
          )}
        </div>
      ) : (
        <div className="brandfetch-key-row">
          <span className="brandfetch-key-state" aria-labelledby={`${id}-label`} role="status" data-testid={`${testid}-state`}>
            {secret && <KeyRound size={16} strokeWidth={1.9} aria-hidden />}
            {shown}
          </span>
          {extra}
          <button type="button" className={button()} aria-disabled={busy} onClick={() => !busy && setReplacing(true)} data-testid={`${testid}-replace`}>
            Replace
          </button>
          <button type="button" className={button({ kind: 'danger' })} aria-disabled={busy} onClick={() => !busy && void onRemove()} data-testid={`${testid}-remove`}>
            Remove
          </button>
        </div>
      )}
      <p className="field-hint" id={`${id}-hint`}>
        {hint}
      </p>
    </div>
  );
}

function Keys({ store }: { store: BrandfetchStore }) {
  const view = useStore(store, (s) => s.view);
  const id = useId();
  const keychain = useKeychainName();
  if (view.kind === 'loading') return <p className="field-hint">Reading the Brandfetch settings…</p>;
  if (view.kind === 'unreadable') return <p className="settings-problem">{view.problem}</p>;
  const { status, busy, problem, notice } = view;
  const count = logoCount(status);
  const state = store.getState();
  const test = (
    <button type="button" className={button()} aria-disabled={busy} onClick={() => !busy && void state.testKey()} data-testid="brandfetch-key-test">
      Test key
    </button>
  );
  return (
    <>
      <section className="settings-section" aria-labelledby={`${id}-keys`}>
        <h3 id={`${id}-keys`}>Your keys</h3>
        <div className="group brandfetch-keys">
          <KeyField
            label="Client ID"
            hint="Turns on logo search in the icon picker. Searches go from this window straight to Brandfetch."
            shown={status.client_id}
            secret={false}
            busy={busy}
            testid="brandfetch-client-id"
            onSave={state.saveClientId}
            onRemove={state.forgetClientId}
          />
          <KeyField
            label="API key"
            hint={`Turns a picked logo into a Tile icon. Stored in ${keychain}; Open TouchDeck never shows it again. Each pick uses one Brand API request; Test key is free.`}
            shown={status.key_set ? 'Set' : null}
            secret
            busy={busy}
            testid="brandfetch-key"
            extra={test}
            onSave={state.saveKey}
            onRemove={state.forgetKey}
          />
          {notice && (
            <p className="field-hint brandfetch-notice" role="status" data-testid="brandfetch-notice">
              {notice}
            </p>
          )}
          {problem && (
            <p className="settings-problem brandfetch-problem" role="alert" data-testid="brandfetch-problem">
              {problem}
            </p>
          )}
        </div>
      </section>
      <section className="settings-section" aria-labelledby={`${id}-logos`}>
        <h3 id={`${id}-logos`}>Logos on this PC</h3>
        <div className="group">
          <div className="row">
            <span className="row-text">
              <span className="row-title" data-testid="brandfetch-count">
                {count ?? 'No Brandfetch logos yet'}
              </span>
              <span className="row-sub">
                {status.expired.length > 0
                  ? `${status.expired.length} went 30 days without a refresh, so this PC deleted their cached copy. Their Tiles keep showing them; pick them again to refresh them.`
                  : 'Each logo is fetched again from day 25, so no cached copy is older than 30 days.'}
              </span>
            </span>
          </div>
        </div>
      </section>
    </>
  );
}

/** The General view's row that opens this section. */
export function BrandfetchEntry({ onOpen, focus }: { onOpen(): void; focus: boolean }) {
  const companion = useCompanion();
  const store = brandfetchStore(companion);
  const view = useStore(store, (s) => s.view);
  const entry = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    void store.getState().load();
  }, [store]);
  useEffect(() => {
    if (focus) entry.current?.focus();
  }, [focus]);
  const on = view.kind === 'ready' && view.status.client_id !== null;
  return (
    <section className="settings-section">
      <div className="group">
        <button ref={entry} type="button" className="row row-button" onClick={onOpen} data-testid="brandfetch-entry">
          <span className="row-text">
            <span className="row-title">Brandfetch logos</span>
            <span className="row-sub">{on ? 'On. The icon picker searches Brandfetch with your client ID.' : 'Off. Add your own Brandfetch keys to search brand logos.'}</span>
          </span>
          <ChevronRight className="row-chevron" size={18} strokeWidth={1.9} aria-hidden />
        </button>
      </div>
    </section>
  );
}

export function BrandfetchView({ onBack, focusBack }: { onBack(): void; focusBack: boolean }) {
  const companion = useCompanion();
  const store = brandfetchStore(companion);
  const back = useRef<HTMLButtonElement>(null);
  const id = useId();
  useEffect(() => {
    void store.getState().load();
  }, [store]);
  useEffect(() => {
    if (focusBack) back.current?.focus();
  }, [focusBack]);

  return (
    <>
      <div className="dialog-head">
        <div className="dialog-head-start">
          <button ref={back} type="button" className={button({ kind: 'icon' })} aria-label="Back" onClick={onBack}>
            <ChevronLeft size={18} strokeWidth={1.9} aria-hidden />
          </button>
          <Dialog.Title className="dialog-title">Brandfetch logos</Dialog.Title>
        </div>
        <Dialog.Close className={button({ kind: 'icon' })} aria-label="Close">
          <X size={18} strokeWidth={1.9} aria-hidden />
        </Dialog.Close>
      </div>
      <p className="dialog-lede">Search brand logos in the icon picker with your own Brandfetch account. It stays off until you add a client ID.</p>
      <Keys store={store} />
      <section className="settings-section" aria-labelledby={`${id}-terms`}>
        <h3 id={`${id}-terms`}>Good to know</h3>
        <ul className="brandfetch-notes" data-testid="brandfetch-notes">
          <li>You use your own Brandfetch account and keys. Open TouchDeck ships none.</li>
          <li>Logos stay the trademarks of their brand owners.</li>
          <li>Brandfetch lets apps keep a logo for 30 days. This PC fetches each one again before then; the free developer plan includes about 100 Brand API requests.</li>
        </ul>
        <a className="brandfetch-portal" href={DEVELOPER_PORTAL} target="_blank" rel="noreferrer">
          Get a client ID and API key from Brandfetch
        </a>
      </section>
    </>
  );
}
