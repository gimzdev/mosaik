// The interface's building blocks: buttons, switches, choices, keys, messages, dialogs, menus.
// Every one works with the keyboard alone and says what it is to screen readers.

import { type ReactNode, createContext, useCallback, useContext, useEffect, useId, useRef, useState } from 'react';

/** Mosaïk's mark: four tiles of the palette. */
export function Mark({ size = 22 }: { size?: number }) {
  return (
    <svg aria-hidden="true" width={size} height={size} viewBox="0 0 32 32" className="shrink-0">
      <rect x="2" y="2" width="16" height="16" rx="3" fill="var(--color-lapis)" />
      <rect x="21" y="2" width="9" height="9" rx="3" fill="var(--color-saffron)" />
      <rect x="21" y="14" width="9" height="16" rx="3" fill="var(--color-verdigris)" />
      <rect x="2" y="21" width="16" height="9" rx="3" fill="var(--color-madder)" />
    </svg>
  );
}

type Tone = 'primary' | 'secondary' | 'quiet' | 'danger';

const TONES: Record<Tone, string> = {
  primary:
    'bg-[var(--accent)] text-[var(--on-accent)] hover:brightness-110 forced-colors:border forced-colors:border-[ButtonText]',
  secondary: 'border border-[var(--line)] bg-[var(--surface)] hover:bg-[var(--hover)]',
  quiet: 'text-[var(--accent)] hover:bg-[var(--hover)]',
  danger: 'border border-[var(--danger)] text-[var(--danger)] hover:bg-[var(--hover)]',
};

export function Button({
  tone = 'secondary',
  onClick,
  children,
  disabled = false,
  pending = false,
  label,
  type = 'button',
  expanded,
  controls,
}: {
  tone?: Tone;
  onClick?: () => void;
  children: ReactNode;
  disabled?: boolean;
  /** Waiting for Mosaïk: the button cannot be pressed again meanwhile. */
  pending?: boolean;
  /** A fuller name for screen readers, starting with the visible text ("Open “Brand”"). */
  label?: string;
  type?: 'button' | 'submit';
  /** For a button that shows or hides something. */
  expanded?: boolean;
  controls?: string;
}) {
  return (
    <button
      type={type}
      disabled={disabled || pending}
      aria-busy={pending || undefined}
      aria-label={label}
      aria-expanded={expanded}
      aria-controls={controls}
      onClick={onClick}
      className={`inline-flex min-h-8 items-center justify-center gap-2 rounded-lg px-3 py-1.5 text-small font-semibold whitespace-nowrap transition-[background-color,filter,opacity] duration-150 disabled:opacity-50 forced-colors:disabled:border forced-colors:disabled:border-[GrayText] forced-colors:disabled:bg-[Canvas] forced-colors:disabled:text-[GrayText] forced-colors:disabled:opacity-100 ${pending ? 'cursor-progress' : 'disabled:cursor-not-allowed'} ${TONES[tone]}`}
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
  leading = false,
}: {
  label: string;
  description?: string;
  checked: boolean;
  onChange: (on: boolean) => void;
  /** The switch before its label (on its own, outside a list of settings). */
  leading?: boolean;
}) {
  const id = useId();
  return (
    <div
      className={`flex items-start py-3 ${leading ? 'flex-row-reverse justify-end gap-3' : 'justify-between gap-6'}`}
    >
      <div className="min-w-0">
        <label htmlFor={id} className="font-semibold">
          {label}
        </label>
        {description ? (
          <p id={`${id}-d`} className="mt-0.5 text-small text-[var(--muted)]">
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
        onClick={() => {
          onChange(!checked);
        }}
        className={`relative mt-0.5 h-6 w-11 shrink-0 rounded-full border transition-colors duration-150 ${
          checked ? 'border-[var(--accent)] bg-[var(--accent)]' : 'border-[var(--control)] bg-transparent'
        }`}
      >
        <span
          aria-hidden="true"
          className={`absolute top-[3px] h-4 w-4 rounded-full transition-[left,background-color] duration-150 forced-colors:bg-[CanvasText] ${
            checked ? 'left-[23px] bg-[var(--on-accent)]' : 'left-[3px] bg-[var(--control)]'
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
  options: readonly { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  const name = useId();
  return (
    <fieldset className="py-3">
      {/* Floated so it lays out like any other row, not on the fieldset's border. */}
      <legend className="float-left w-full font-semibold">{label}</legend>
      <div className="clear-both flex flex-wrap gap-2 pt-2">
        {options.map((o) => {
          const on = o.value === value;
          return (
            <label
              key={o.value}
              className={`flex cursor-pointer items-center gap-2 rounded-lg border px-3 py-1.5 text-small has-[:focus-visible]:outline has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-[var(--focus)] ${
                on ? 'border-[var(--accent)] font-semibold forced-colors:border-[Highlight]' : 'border-[var(--line)]'
              }`}
            >
              <input
                type="radio"
                name={name}
                value={o.value}
                checked={on}
                onChange={() => {
                  onChange(o.value);
                }}
                // The label shows the focus ring; the radio's own would draw a second one inside it.
                className="accent-[var(--accent)] focus-visible:outline-none"
              />
              {o.label}
            </label>
          );
        })}
      </div>
    </fieldset>
  );
}

/** A shortcut as keys: "Win+Alt+Space" → [Win] [Alt] [Space], read as "Win plus Alt plus Space". */
export function Keys({ shortcut }: { shortcut: string }) {
  const keys = shortcut.split('+');
  return (
    <span className="inline-flex items-center gap-1 align-baseline">
      {keys.map((k, i) => (
        <span key={`${k}-${String(i)}`} className="inline-flex items-center">
          <kbd className="rounded-md border border-b-2 border-[var(--line)] bg-[var(--surface)] px-1.5 font-mono text-caption leading-5 text-[var(--text)]">
            {k}
          </kbd>
          {i < keys.length - 1 ? <span className="sr-only"> plus </span> : null}
        </span>
      ))}
    </span>
  );
}

/** A short message next to the control it is about, read out by screen readers. Errors are
 * alerts; the element is always there, so a message that appears is announced. */
export function Status({ text, error = false }: { text: string | null; error?: boolean }) {
  return (
    <p
      role={error ? 'alert' : 'status'}
      className={`text-small ${error ? 'text-[var(--danger)]' : 'text-[var(--muted)]'}`}
    >
      {text}
    </p>
  );
}

export function Section({ title, children, intro }: { title: string; intro?: ReactNode; children: ReactNode }) {
  const id = useId();
  return (
    <section aria-labelledby={id} className="mt-8 first:mt-0">
      <h2 id={id} className="font-display text-heading font-semibold tracking-tight">
        {title}
      </h2>
      {intro ? <div className="mt-1 text-small text-[var(--muted)]">{intro}</div> : null}
      <div className="mt-3 divide-y divide-[var(--line)] rounded-xl border border-[var(--line)] bg-[var(--surface)] px-4">
        {children}
      </div>
    </section>
  );
}

/** A page's title. It takes focus when the page opens, so keyboard and screen-reader users
 * start at the top of the new page. */
export function PageTitle({ children, eyebrow, id }: { children: ReactNode; eyebrow?: string; id?: string }) {
  const ref = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    ref.current?.focus({ preventScroll: true });
  }, []);
  return (
    <div>
      {eyebrow ? <p className="text-small font-semibold text-[var(--muted)]">{eyebrow}</p> : null}
      <h1
        ref={ref}
        id={id}
        tabIndex={-1}
        className="font-display text-title font-bold tracking-tight focus-visible:outline-none"
      >
        {children}
      </h1>
    </div>
  );
}

/** A modal question with the keyboard trapped inside (the browser's own dialog, which gives
 * focus back to what had it when it closes). */
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
      <h2 id={titleId} className="font-display text-heading font-semibold">
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

/** A button that shows a short list of actions under it. Esc or a click elsewhere closes it,
 * and focus goes back to the button. */
export function MenuButton({
  label,
  children,
  items,
}: {
  /** The button's full name for screen readers ("More for “Brand”"). */
  label: string;
  children: ReactNode;
  items: readonly { label: string; onSelect: () => void; danger?: boolean }[];
}) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const box = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    box.current?.querySelector<HTMLButtonElement>('[data-item]')?.focus();
    const away = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', away);
    return () => {
      document.removeEventListener('mousedown', away);
    };
  }, [open]);

  const close = () => {
    setOpen(false);
    trigger.current?.focus();
  };

  return (
    <div
      ref={box}
      className="relative"
      onKeyDown={(e) => {
        if (!open) return;
        const all = Array.from(box.current?.querySelectorAll<HTMLButtonElement>('[data-item]') ?? []);
        const at = all.indexOf(document.activeElement as HTMLButtonElement);
        if (e.key === 'Escape') {
          e.preventDefault();
          close();
        } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
          e.preventDefault();
          const next = all[(at + (e.key === 'ArrowDown' ? 1 : all.length - 1)) % all.length];
          next?.focus();
        }
      }}
      onBlur={(e) => {
        if (!box.current?.contains(e.relatedTarget)) setOpen(false);
      }}
    >
      <button
        ref={trigger}
        type="button"
        aria-label={label}
        aria-expanded={open}
        aria-controls={id}
        onClick={() => {
          setOpen((o) => !o);
        }}
        className="inline-flex min-h-8 min-w-8 items-center justify-center rounded-lg px-2 text-small font-semibold text-[var(--muted)] hover:bg-[var(--hover)]"
      >
        {children}
      </button>
      {open ? (
        <div
          id={id}
          className="absolute right-0 z-20 mt-1 flex min-w-44 flex-col rounded-xl border border-[var(--line)] bg-[var(--surface)] p-1 shadow-lg"
        >
          {items.map((item) => (
            <button
              key={item.label}
              type="button"
              data-item
              onClick={() => {
                setOpen(false);
                item.onSelect();
              }}
              className={`rounded-lg px-3 py-1.5 text-left text-small font-semibold hover:bg-[var(--hover)] ${
                item.danger ? 'text-[var(--danger)]' : ''
              }`}
            >
              {item.label}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

/** What happened after an action: a short message at the bottom of the window, whichever page
 * is showing (it survives a page change). A success fades after a few seconds; an error stays
 * until dismissed or replaced. */
type Say = (text: string, error?: boolean) => void;

const FeedbackContext = createContext<Say>(() => undefined);

export function useSay(): Say {
  return useContext(FeedbackContext);
}

export function Feedback({ children }: { children: ReactNode }) {
  const [message, setMessage] = useState<{ text: string; error: boolean; n: number } | null>(null);
  const counter = useRef(0);
  const say = useCallback<Say>((text, error = false) => {
    counter.current += 1;
    setMessage({ text, error, n: counter.current });
  }, []);
  useEffect(() => {
    if (!message || message.error) return;
    const t = window.setTimeout(() => {
      setMessage((m) => (m?.n === message.n ? null : m));
    }, 4000);
    return () => {
      window.clearTimeout(t);
    };
  }, [message]);
  return (
    <FeedbackContext.Provider value={say}>
      {children}
      <div className="pointer-events-none fixed inset-x-0 bottom-5 z-30 flex justify-center px-6">
        {/* Both regions are always there so what appears in them is announced. */}
        <div role="status" className="contents">
          {message && !message.error ? (
            <p className="pointer-events-auto rounded-xl bg-[var(--text)] px-4 py-2 text-small font-semibold text-[var(--bg)] shadow-lg">
              {message.text}
            </p>
          ) : null}
        </div>
        <div role="alert" className="contents">
          {message?.error ? (
            <div className="pointer-events-auto flex max-w-xl items-start gap-3 rounded-xl border border-[var(--danger)] bg-[var(--surface)] px-4 py-2 shadow-lg">
              <p className="text-small text-[var(--danger)]">{message.text}</p>
              <button
                type="button"
                onClick={() => {
                  setMessage(null);
                }}
                className="shrink-0 rounded-md px-2 text-small font-semibold text-[var(--accent)] hover:bg-[var(--hover)]"
              >
                Dismiss
              </button>
            </div>
          ) : null}
        </div>
      </div>
    </FeedbackContext.Provider>
  );
}
