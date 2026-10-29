import { useId, type ReactNode, type SelectHTMLAttributes } from 'react';
import clsx from 'clsx';
import { ChevronsUpDown } from 'lucide-react';

/** The ids a field's control points at: its label, its hint and its error. */
export interface FieldIds {
  control: string;
  hint: string;
  error: string;
}

export function useFieldIds(): FieldIds {
  const id = useId();
  return { control: id, hint: `${id}-hint`, error: `${id}-error` };
}

/** `aria-describedby` and `aria-invalid` for a field's control, naming only the hint and error that show. */
export function described(ids: FieldIds, hint: ReactNode, error: string | null) {
  const by = [hint ? ids.hint : null, error ? ids.error : null].filter(Boolean).join(' ');
  return { 'aria-describedby': by || undefined, 'aria-invalid': error ? true : undefined };
}

/** A label over a control, then its hint and the reason the Layout rules refused it. */
export function Field({ ids, label, hint, error, children, testId }: { ids: FieldIds; label: ReactNode; hint?: ReactNode; error?: string | null; children: ReactNode; testId?: string }) {
  return (
    <div className="field" data-testid={testId}>
      <label className="field-label" htmlFor={ids.control}>
        {label}
      </label>
      {children}
      {hint && (
        <p className="field-hint" id={ids.hint}>
          {hint}
        </p>
      )}
      <FieldError id={ids.error} error={error ?? null} />
    </div>
  );
}

export function FieldError({ id, error }: { id: string; error: string | null }) {
  if (!error) return null;
  return (
    <p className="field-error" id={id} data-testid="field-error">
      {error}
    </p>
  );
}

/** A native select drawn as the design's select: the value and an up-down chevron. */
export function Select({ className, children, ...props }: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <span className="select">
      <select className={clsx('input select-input', className)} {...props}>
        {children}
      </select>
      <ChevronsUpDown className="select-chevron" size={16} strokeWidth={1.9} aria-hidden />
    </span>
  );
}
