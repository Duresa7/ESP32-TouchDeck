import { useId } from 'react';
import { Switch } from '@base-ui/react/switch';
import { useStore } from 'zustand';
import type { DetailStore } from '../../lib/integrations/detail.ts';
import { sameValues, transportWarning, type FormField } from '../../lib/integrations/model.ts';
import { button } from '../../shell/button.ts';

function Field({ store, field, warnKey }: { store: DetailStore; field: FormField; warnKey: boolean }) {
  const id = useId();
  const value = useStore(store, (s) => s.values[field.name]);
  const error = useStore(store, (s) => s.errors[field.name]);
  const text = typeof value === 'string' ? value : '';
  const warning = warnKey && field.kind === 'string' ? transportWarning(text) : null;
  const numeric = field.kind === 'integer' || field.kind === 'number';
  const input = field.kind === 'json' ? (
    <textarea id={id} className="input" rows={12} value={text} spellCheck={false} autoComplete="off" aria-invalid={error ? true : undefined} aria-describedby={error ? `${id}-error` : undefined} onChange={(event) => store.setValue(field.name, event.target.value)} data-testid="config-field" data-field={field.name} />
  ) : (
    <input
      id={id}
      className="input tabular"
      value={text}
      inputMode={field.kind === 'integer' ? 'numeric' : field.kind === 'number' ? 'decimal' : undefined}
      placeholder={field.placeholder}
      spellCheck={false}
      autoComplete="off"
      aria-invalid={error ? true : undefined}
      aria-describedby={error ? `${id}-error` : undefined}
      onChange={(event) => store.setValue(field.name, event.target.value)}
      data-testid="config-field"
      data-field={field.name}
    />
  );
  return (
    <div className="field">
      <label className="field-label" htmlFor={id}>
        {field.label}
      </label>
      {numeric && field.unit ? (
        <span className="input-unit">
          {input}
          <span>{field.unit}</span>
        </span>
      ) : (
        input
      )}
      {error && (
        <p className="field-error" id={`${id}-error`} data-testid="field-error">
          {error}
        </p>
      )}
      {warning && (
        <p className="notice-signal" role="status" data-testid="transport-warning">
          {warning}
        </p>
      )}
    </div>
  );
}

function SwitchField({ store, field }: { store: DetailStore; field: FormField }) {
  const id = useId();
  const on = useStore(store, (s) => s.values[field.name] === true);
  return (
    <div className="switch-row">
      <span className="field-label" id={id}>
        {field.label}
      </span>
      <Switch.Root className="switch" checked={on} onCheckedChange={(next) => store.setValue(field.name, next)} aria-labelledby={id} data-testid="config-switch" data-field={field.name}>
        <Switch.Thumb className="switch-thumb" />
      </Switch.Root>
    </div>
  );
}

function rows(fields: readonly FormField[]): FormField[][] {
  const out: FormField[][] = [];
  for (let i = 0; i < fields.length; i++) {
    const [field, next] = [fields[i], fields[i + 1]];
    if (field.kind === 'string' && (next?.kind === 'integer' || next?.kind === 'number')) {
      out.push([field, next]);
      i++;
    } else {
      out.push([field]);
    }
  }
  return out;
}

/** The settings form the Integration's schema builds, with Save. Secret fields have their own controls. */
export function ConfigForm({ store }: { store: DetailStore }) {
  const fields = useStore(store, (s) => s.fields);
  const hasSecrets = useStore(store, (s) => s.secretFields.length > 0);
  const dirty = useStore(store, (s) => !sameValues(s.fields, s.values, s.saved));
  const busy = useStore(store, (s) => s.busy);
  const switches = fields.filter((f) => f.kind === 'boolean');
  const inputs = fields.filter((f) => f.kind !== 'boolean');
  return (
    <form
      className="detail-section"
      noValidate
      onSubmit={(event) => {
        event.preventDefault();
        if (dirty) void store.save();
      }}
      data-testid="config-form"
    >
      {switches.map((field) => (
        <SwitchField key={field.name} store={store} field={field} />
      ))}
      {rows(inputs).map((row) =>
        row.length === 2 ? (
          <div className="form-row" key={row[0].name}>
            <Field store={store} field={row[0]} warnKey={hasSecrets} />
            <Field store={store} field={row[1]} warnKey={hasSecrets} />
          </div>
        ) : (
          <Field key={row[0].name} store={store} field={row[0]} warnKey={hasSecrets} />
        ),
      )}
      <div className="form-actions">
        <button type="submit" className={button({ kind: 'primary' })} aria-disabled={!dirty || busy !== null} disabled={busy === 'save'} data-testid="config-save">
          {busy === 'save' ? 'Saving…' : 'Save'}
        </button>
        {dirty && (
          <span className="form-state" data-testid="config-dirty">
            Unsaved changes
          </span>
        )}
      </div>
    </form>
  );
}
