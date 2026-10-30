import { useEffect, useId, useRef, useState, type KeyboardEvent } from 'react';
import { AlertDialog } from '@base-ui/react/alert-dialog';
import { KeyRound } from 'lucide-react';
import { useStore } from 'zustand';
import type { DetailStore } from '../../lib/integrations/detail.ts';
import type { FormField } from '../../lib/integrations/model.ts';
import { button } from '../../shell/button.ts';

/**
 * A write-only secret. Once set it shows only "Set", with Replace and Forget. The input is uncontrolled, so what's
 * typed stays in the field until Save sends it, and Save empties the field before the answer comes back.
 */
export function SecretField({ store, field, integration }: { store: DetailStore; field: FormField; integration: string }) {
  const id = useId();
  const secret = useStore(store, (s) => s.secrets[field.name]);
  const error = useStore(store, (s) => s.errors[field.name]);
  const busy = useStore(store, (s) => s.busy);
  const input = useRef<HTMLInputElement>(null);
  const replaceButton = useRef<HTMLButtonElement>(null);
  const [replacing, setReplacing] = useState(false);
  const [filled, setFilled] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const set = secret?.set === true;
  const editing = !set || replacing;
  const noun = field.label.toLowerCase();

  useEffect(() => {
    if (replacing) input.current?.focus();
  }, [replacing]);

  const save = async () => {
    const el = input.current;
    if (!el) return;
    const value = el.value;
    el.value = '';
    setFilled(false);
    if (await store.putSecret({ field: field.name, value })) setReplacing(false);
  };

  const cancel = () => {
    if (input.current) input.current.value = '';
    setFilled(false);
    setReplacing(false);
    requestAnimationFrame(() => replaceButton.current?.focus());
  };

  const keys = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      void save();
    } else if (event.key === 'Escape' && replacing) {
      event.stopPropagation();
      cancel();
    }
  };

  return (
    <div className="field" data-testid="secret-field" data-field={field.name} data-set={set}>
      <label className="field-label" htmlFor={editing ? id : undefined} id={`${id}-label`}>
        {field.label}
      </label>
      {editing ? (
        <div className="secret-row">
          <input
            ref={input}
            id={id}
            type="password"
            className="input"
            placeholder={set ? undefined : 'Not set'}
            autoComplete="new-password"
            spellCheck={false}
            aria-invalid={error ? true : undefined}
            aria-describedby={error ? `${id}-error` : `${id}-hint`}
            onInput={(event) => setFilled(event.currentTarget.value.length > 0)}
            onKeyDown={keys}
            data-testid="secret-input"
          />
          <button type="button" className={button()} aria-disabled={!filled || busy !== null} onClick={() => filled && void save()} data-testid="secret-save">
            Save
          </button>
          {replacing && (
            <button type="button" className={button()} onClick={cancel} data-testid="secret-cancel">
              Cancel
            </button>
          )}
        </div>
      ) : (
        <div className="secret-row">
          <span className="secret-state" aria-labelledby={`${id}-label`} role="status" data-testid="secret-state">
            <KeyRound size={16} strokeWidth={1.9} aria-hidden />
            {secret?.hint ?? 'Set'}
          </span>
          <button ref={replaceButton} type="button" className={button()} onClick={() => setReplacing(true)} data-testid="secret-replace">
            Replace
          </button>
          <button type="button" className={button({ kind: 'danger' })} onClick={() => setConfirming(true)} data-testid="secret-forget">
            Forget
          </button>
        </div>
      )}
      {error && (
        <p className="field-error" id={`${id}-error`} data-testid="field-error">
          {error}
        </p>
      )}
      <p className="field-hint" id={`${id}-hint`}>
        Stored in Windows Credential Manager, never in a file.
      </p>
      <AlertDialog.Root open={confirming} onOpenChange={setConfirming}>
        <AlertDialog.Portal>
          <AlertDialog.Backdrop className="scrim" />
          <AlertDialog.Popup className="dialog dialog-confirm" data-testid="forget-dialog">
            <AlertDialog.Title className="dialog-title">Forget the {noun}?</AlertDialog.Title>
            <AlertDialog.Description className="dialog-text">{integration} stops until you add it again.</AlertDialog.Description>
            <div className="dialog-actions">
              <AlertDialog.Close className={button()}>Cancel</AlertDialog.Close>
              <button
                type="button"
                className={button({ kind: 'primary' })}
                onClick={() => {
                  setConfirming(false);
                  void store.forgetSecret(field.name);
                }}
                data-testid="forget-confirm"
              >
                Forget
              </button>
            </div>
          </AlertDialog.Popup>
        </AlertDialog.Portal>
      </AlertDialog.Root>
    </div>
  );
}
