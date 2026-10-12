// The interface's building blocks: buttons, switches, choices, keys, messages. Every one works
// with the keyboard alone and says what it is to screen readers.

import { type ReactNode, useEffect, useId, useRef } from 'react';

type Tone = 'primary' | 'secondary' | 'quiet' | 'danger';

const TONES: Record<Tone, string> = {
  primary:
    'bg-[var(--accent)] text-[var(--on-accent)] hover:brightness-110 disabled:opacity-50 forced-colors:border forced-colors:border-[ButtonText]',
  secondary: 'border border-[var(--line)] bg-[var(--surface)] hover:bg-[var(--hover)] disabled:opacity-50',
  quiet: 'text-[var(--accent)] hover:bg-[var(--hover)] disabled:opacity-50',
  danger: 'border border-[var(--danger)] text-[var(--danger)] hover:bg-[var(--hover)] disabled:opacity-50',
};

export function Button({
  tone = 'secondary',
  onClick,
  children,
  disabled = false,
  label,
  type = 'button',
}: {
  tone?: Tone;
  onClick?: () => void;
  children: ReactNode;
  disabled?: boolean;
  /** A fuller name for screen readers when the visible text is short ("Rename “Brand”"). */
  label?: string;
  type?: 'button' | 'submit';
}) {
  return (
    <button
      type={type}
      disabled={disabled}
      aria-label={label}
      onClick={onClick}
      className={`inline-flex min-h-8 items-center justify-center gap-2 rounded-lg px-3 py-1.5 text-[13px] font-semibold transition-colors duration-150 ${TONES[tone]}`}
    >
      {children}
    </button>
  );
}

/** An on/off setting, read by screen readers as a switch with its label and description. */
export function Toggle({
  label,
  description,
  checked,
  onChange,
  disabled = false,
}: {
  label: string;
  description?: string;
  checked: boolean;
  onChange: (on: boolean) => void;
  disabled?: boolean;
}) {
  const id = useId();
  return (
    <div className="flex items-start justify-between gap-6 py-3">
      <div className="min-w-0">
        <label htmlFor={id} className="font-semibold">
          {label}
        </label>
        {description ? (
          <p id={`${id}-d`} className="mt-0.5 text-[13px] text-[var(--muted)]">
            {description}
          </p>
        ) : null}
      </div>
      <button
        id={id}
        type="button"
        role="switch"
        aria-checked={checked}
        aria-describedby={description ? `${id}-d` : undefined}
        disabled={disabled}
        onClick={() => {
          onChange(!checked);
        }}
        className={`relative mt-0.5 h-6 w-11 shrink-0 rounded-full border transition-colors duration-150 disabled:opacity-50 ${
          checked ? 'border-[var(--accent)] bg-[var(--accent)]' : 'border-[var(--line)] bg-[var(--hover)]'
        }`}
      >
        <span
          aria-hidden="true"
          className={`absolute top-0.5 h-[18px] w-[18px] rounded-full bg-[var(--surface)] shadow transition-[left] duration-150 forced-colors:border forced-colors:border-[CanvasText] ${
            checked ? 'left-[22px]' : 'left-0.5'
          }`}
        />
      </button>
    </div>
  );
}

/** One choice among a few, as radio buttons. */
export function Choice<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: readonly { value: T; label: string; description?: string }[];
  onChange: (value: T) => void;
}) {
  const name = useId();
  return (
    <fieldset className="py-3">
      <legend className="font-semibold">{label}</legend>
      <div className="mt-2 flex flex-wrap gap-2">
        {options.map((o) => (
          <label
            key={o.value}
            className={`flex cursor-pointer items-center gap-2 rounded-lg border px-3 py-1.5 text-[13px] has-[:focus-visible]:outline has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-[var(--focus)] ${
              o.value === value ? 'border-[var(--accent)] font-semibold' : 'border-[var(--line)]'
            }`}
          >
            <input
              type="radio"
              name={name}
              value={o.value}
              checked={o.value === value}
              onChange={() => {
                onChange(o.value);
              }}
              className="accent-[var(--accent)]"
            />
            {o.label}
          </label>
        ))}
      </div>
    </fieldset>
  );
}

/** A shortcut as keys: "Win+Alt+Space" → [Win] [Alt] [Space], read as "Win plus Alt plus Space". */
export function Keys({ shortcut }: { shortcut: string }) {
  const keys = shortcut.split('+');
  return (
    <span className="inline-flex items-center gap-1">
      {keys.map((k, i) => (
        <span key={`${k}-${String(i)}`} className="inline-flex items-center">
          <kbd className="rounded-md border border-b-2 border-[var(--line)] bg-[var(--surface)] px-1.5 py-0.5 font-mono text-[12px]">
            {k}
          </kbd>
          {i < keys.length - 1 ? <span className="sr-only"> plus </span> : null}
        </span>
      ))}
    </span>
  );
}

/** A short result of the last action, read out by screen readers. */
export function Status({ text, error = false }: { text: string | null; error?: boolean }) {
  return (
    <p
      role={error ? 'alert' : 'status'}
      className={`min-h-5 text-[13px] ${error ? 'text-[var(--danger)]' : 'text-[var(--muted)]'}`}
    >
      {text}
    </p>
  );
}

export function Section({ title, children, intro }: { title: string; intro?: ReactNode; children: ReactNode }) {
  const id = useId();
  return (
    <section aria-labelledby={id} className="mt-8 first:mt-0">
      <h2 id={id} className="font-display text-lg font-semibold tracking-tight">
        {title}
      </h2>
      {intro ? <div className="mt-1 text-[13px] text-[var(--muted)]">{intro}</div> : null}
      <div className="mt-3 divide-y divide-[var(--line)] rounded-xl border border-[var(--line)] bg-[var(--surface)] px-4">
        {children}
      </div>
    </section>
  );
}

/** A modal question with the keyboard trapped inside (the browser's own dialog). */
export function Confirm({
  open,
  title,
  body,
  confirm,
  onConfirm,
  onCancel,
}: {
  open: boolean;
  title: string;
  body: string;
  confirm: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) d.showModal();
    if (!open && d.open) d.close();
  }, [open]);
  const titleId = useId();
  const bodyId = useId();
  return (
    <dialog
      ref={ref}
      aria-labelledby={titleId}
      aria-describedby={bodyId}
      onCancel={(e) => {
        e.preventDefault();
        onCancel();
      }}
      className="m-auto max-w-md rounded-2xl border border-[var(--line)] bg-[var(--surface)] p-6 text-[var(--text)] shadow-xl backdrop:bg-black/40"
    >
      <h2 id={titleId} className="font-display text-lg font-semibold">
        {title}
      </h2>
      <p id={bodyId} className="mt-2 text-[var(--muted)]">
        {body}
      </p>
      <div className="mt-6 flex justify-end gap-2">
        <Button onClick={onCancel}>Cancel</Button>
        <Button tone="danger" onClick={onConfirm}>
          {confirm}
        </Button>
      </div>
    </dialog>
  );
}
