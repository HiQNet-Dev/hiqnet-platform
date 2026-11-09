import { useId } from 'react';
import type { InputHTMLAttributes, ReactNode, SelectHTMLAttributes } from 'react';

import { cn } from '../utils/cn';

type FieldProps = {
  label: string;
  hint?: string;
  error?: string;
  children: (props: { id: string; describedBy?: string }) => ReactNode;
};

export function FormField({ label, hint, error, children }: FieldProps) {
  const id = useId();
  const hintId = hint ? `${id}-hint` : undefined;
  const errorId = error ? `${id}-error` : undefined;
  const describedBy = [hintId, errorId].filter(Boolean).join(' ') || undefined;

  return (
    <div className={cn('form-field', error && 'form-field--error')}>
      <label className="form-field__label" htmlFor={id}>
        {label}
      </label>
      {children({ id, describedBy })}
      {hint && (
        <p className="form-field__hint" id={hintId}>
          {hint}
        </p>
      )}
      {error && (
        <p className="form-field__error" id={errorId} role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

export function Input({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return <input className={cn('input', className)} {...props} />;
}

export function Select({ className, ...props }: SelectHTMLAttributes<HTMLSelectElement>) {
  return <select className={cn('input select', className)} {...props} />;
}
