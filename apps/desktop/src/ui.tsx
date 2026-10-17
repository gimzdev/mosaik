// The window's building blocks: buttons, switches, choices, keys, menus, dialogs, and a voice for
// screen readers. Every one works with the keyboard alone and says what it is to screen readers,
// and every one that is clicked is at least 40 pixels high. Nothing here floats over the page to
// say something: what happened is said where it happened.

import {
  type CSSProperties,
  type ReactNode,
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';

import { Spinner } from './icons';
import type { ZoneColor } from './protocol';

/** A zone's colour, as the theme draws it. */
export const zoneTint = (color: ZoneColor) => `var(--zone-${color})`;

/** The style of a zone's number on its colour (`.zone-chip`): saffron, light, takes dark text. */
export function zoneChip(color: ZoneColor): CSSProperties {
  return {
    '--chip': zoneTint(color),
    '--on-chip': color === 'saffron' ? 'var(--on-zone-saffron)' : 'var(--on-zone)',
  } as CSSProperties;
}

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
    'bg-[var(--accent-fill)] text-[var(--on-accent)] hover:bg-[var(--accent-fill-hover)] forced-colors:border forced-colors:border-[ButtonText]',
  secondary: 'border border-[var(--line)] bg-[var(--raised)] text-[var(--text)] hover:bg-[var(--hover)]',
  quiet: 'text-[var(--accent)] hover:bg-[var(--hover)]',
  danger: 'border border-[var(--danger)] text-[var(--danger)] hover:bg-[var(--hover)]',
};

const SIZES = {
  sm: 'min-h-10 gap-1.5 rounded-lg px-3 text-small',
  md: 'min-h-10 gap-2 rounded-[10px] px-4 text-small',
  lg: 'min-h-12 gap-2 rounded-xl px-5 text-body',
} as const;

export function Button({
  tone = 'secondary',
  size = 'md',
  onClick,
  children,
  disabled = false,
  pending = false,
  label,
  hint,
  type = 'button',
  expanded,
  controls,
  wide = false,
}: {
  tone?: Tone;
  size?: keyof typeof SIZES;
  onClick?: () => void;
  children: ReactNode;
  disabled?: boolean;
  /** Waiting for Mosaïk: a ring turns, and the button cannot be pressed again meanwhile. */
  pending?: boolean;
  /** A fuller name for screen readers, starting with the visible text ("Open “Brand”"). */
  label?: string;
  /** More about it for the pointer, shown on hover. */
  hint?: string;
  type?: 'button' | 'submit';
  /** For a button that shows or hides something. */
  expanded?: boolean;
  controls?: string;
  /** As wide as its box. */
  wide?: boolean;
}) {
  return (
    <button
      type={type}
      disabled={disabled || pending}
      aria-busy={pending || undefined}
      aria-label={label}
      title={hint}
      aria-expanded={expanded}
      aria-controls={controls}
      onClick={onClick}
      className={`inline-flex items-center justify-center font-semibold whitespace-nowrap transition-[background-color,opacity] duration-150 disabled:opacity-50 forced-colors:disabled:border forced-colors:disabled:border-[GrayText] forced-colors:disabled:text-[GrayText] forced-colors:disabled:opacity-100 ${
        pending ? 'cursor-progress' : 'disabled:cursor-not-allowed'
      } ${SIZES[size]} ${TONES[tone]} ${wide ? 'w-full' : ''}`}
    >
      {pending ? <Spinner /> : null}
      {children}
    </button>
  );
}

/** A setting's row: what it is on the left, its control on the right. */
export function Row({
  label,
  description,
  htmlFor,
  children,
  id,
}: {
  label: ReactNode;
  description?: ReactNode;
  /** The control the label names, when it is a single field. */
  htmlFor?: string;
  children: ReactNode;
  id?: string;
}) {
  const own = useId();
  const labelId = id ?? own;
  return (
    <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 py-3.5">
      <div className="min-w-0 flex-1 basis-56">
        {htmlFor ? (
          <label id={labelId} htmlFor={htmlFor} className="font-semibold">
            {label}
          </label>
        ) : (
          <p id={labelId} className="font-semibold">
            {label}
          </p>
        )}
        {description ? <div className="mt-0.5 text-small text-[var(--muted)]">{description}</div> : null}
      </div>
      <div className="flex shrink-0 flex-wrap items-center gap-2">{children}</div>
    </div>
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
    <Row label={label} description={description ? <span id={`${id}-d`}>{description}</span> : undefined} htmlFor={id}>
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
        // Drawn small, as switches are, but 60 by 40 pixels to the pointer.
        className={`relative h-6 w-11 shrink-0 rounded-full border transition-colors duration-150 after:absolute after:-inset-x-2 after:-inset-y-2 disabled:opacity-50 ${
          checked ? 'border-[var(--accent-fill)] bg-[var(--accent-fill)]' : 'border-[var(--control)] bg-transparent'
        }`}
      >
        <span
          aria-hidden="true"
          className={`absolute top-[3px] h-4 w-4 rounded-full transition-[left,background-color] duration-150 forced-colors:bg-[CanvasText] ${
            checked ? 'left-[23px] bg-[var(--on-accent)]' : 'left-[3px] bg-[var(--control)]'
          }`}
        />
      </button>
    </Row>
  );
}

/** One choice among a few, as joined segments (radio buttons to screen readers). */
export function Segmented<T extends string>({
  label,
  value,
  options,
  onChange,
  disabled = false,
}: {
  /** Names the group for screen readers (the row shows it). */
  label: string;
  value: T;
  options: readonly { value: T; label: string }[];
  onChange: (value: T) => void;
  disabled?: boolean;
}) {
  const name = useId();
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className="inline-flex rounded-[10px] border border-[var(--line)] bg-[var(--bg)] p-0.5"
    >
      {options.map((o) => {
        const on = o.value === value;
        return (
          <label
            key={o.value}
            className={`relative flex min-h-10 cursor-pointer items-center rounded-lg px-3 text-small font-semibold transition-colors duration-150 has-[:focus-visible]:outline has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-1 has-[:focus-visible]:outline-[var(--focus)] ${
              on
                ? 'bg-[var(--raised)] text-[var(--text)] shadow-[0_1px_0_rgb(255_255_255/0.04),0_1px_6px_-2px_rgb(0_0_0/0.5)] forced-colors:border forced-colors:border-[Highlight]'
                : 'text-[var(--muted)] hover:text-[var(--text)]'
            } ${disabled ? 'pointer-events-none opacity-50' : ''}`}
          >
            <input
              type="radio"
              name={name}
              value={o.value}
              checked={on}
              disabled={disabled}
              onChange={() => {
                onChange(o.value);
              }}
              // Over the whole segment, unseen: a click anywhere on it is on the radio itself, and
              // a screen reader's focus box is the segment, not a dot.
              className="absolute inset-0 m-0 cursor-pointer appearance-none opacity-0 disabled:cursor-default"
            />
            {o.label}
          </label>
        );
      })}
    </div>
  );
}

/** A shortcut as keys: "Win+Alt+Space" → [Win] [Alt] [Space], read as "Win plus Alt plus Space". */
export function Keys({ shortcut, quiet = false }: { shortcut: string; quiet?: boolean }) {
  const keys = shortcut.split('+');
  return (
    <span className="inline-flex items-center gap-1 align-baseline">
      {keys.map((k, i) => (
        <span key={`${k}-${String(i)}`} className="inline-flex items-center">
          <kbd
            className={`rounded-md border border-b-2 px-1.5 font-mono text-caption leading-5 ${
              quiet
                ? 'border-[var(--line)] bg-transparent text-[var(--muted)]'
                : 'border-[var(--line)] bg-[var(--raised)] text-[var(--text)]'
            }`}
          >
            {k}
          </kbd>
          {i < keys.length - 1 ? <span className="sr-only"> plus </span> : null}
        </span>
      ))}
    </span>
  );
}

/** Says the workspace is the one on screen: quiet words with a small dot, not the accent colour of
 * what can be clicked. */
export function OnScreen() {
  return (
    <span className="inline-flex items-center gap-1.5">
      <span aria-hidden="true" className="size-2 shrink-0 rounded-full bg-[var(--ok)] forced-colors:bg-[CanvasText]" />
      On screen
    </span>
  );
}

/** A short message next to the control it is about. Errors are alerts, read out as they appear;
 * other messages are said to screen readers by the page's announcer (useAnnounce), so the same
 * words are not read twice. */
export function Status({ text, error = false }: { text: string | null; error?: boolean }) {
  return (
    <p
      role={error ? 'alert' : undefined}
      className={`text-small empty:hidden ${error ? 'text-[var(--danger)]' : 'text-[var(--muted)]'}`}
    >
      {text}
    </p>
  );
}

/** A titled group of settings rows on one tile. */
export function Section({ title, children, aside }: { title: string; aside?: ReactNode; children: ReactNode }) {
  const id = useId();
  return (
    <section aria-labelledby={id} className="mt-7 first:mt-0">
      <div className="mb-2 flex items-baseline justify-between gap-3 px-1">
        <h2 id={id} className="font-display text-heading font-semibold tracking-tight">
          {title}
        </h2>
        {aside}
      </div>
      <div className="tile divide-y divide-[var(--line)] px-5">{children}</div>
    </section>
  );
}

/** A page's title. It takes focus when the page opens, so keyboard and screen-reader users
 * start at the top of the new page: as the page is drawn, not once it is on screen, when it would
 * take the focus from whatever had it by then (job 0177: a key pressed as the page appeared).
 * Once: React's checks in development run the effect again later, by when the focus may have
 * moved on. */
export function PageTitle({ children, id, className = '' }: { children: ReactNode; id?: string; className?: string }) {
  const ref = useRef<HTMLHeadingElement>(null);
  const took = useRef(false);
  useLayoutEffect(() => {
    if (took.current) return;
    took.current = true;
    ref.current?.focus({ preventScroll: true });
  }, []);
  return (
    <h1
      ref={ref}
      id={id}
      tabIndex={-1}
      className={`font-display text-title font-bold tracking-tight focus-visible:outline-none ${className}`}
    >
      {children}
    </h1>
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
      className="m-auto max-w-md rounded-2xl border border-[var(--line)] bg-[var(--panel)] p-6 text-[var(--text)] shadow-[var(--shadow)] backdrop:bg-black/55"
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

/** The part of the window where `el` can be seen: the window, less what the boxes around it that
 * scroll or clip leave out. */
function visibleArea(el: HTMLElement): { top: number; bottom: number } {
  let top = 0;
  let bottom = window.innerHeight;
  for (let p = el.parentElement; p; p = p.parentElement) {
    if (['auto', 'scroll', 'hidden', 'clip'].includes(getComputedStyle(p).overflowY)) {
      const r = p.getBoundingClientRect();
      top = Math.max(top, r.top);
      bottom = Math.min(bottom, r.bottom);
    }
  }
  return { top, bottom };
}

export interface MenuItem {
  label: string;
  /** A second line, quieter ("Release notes - Google Chrome"). */
  detail?: string | undefined;
  /** Tells items with the same label apart. */
  key?: string;
  onSelect: () => void;
  danger?: boolean;
  icon?: ReactNode;
}

/**
 * A list that opens from a button: under it, or above it near the bottom of the window, so the
 * list is always in view. When it opens, focus goes to the first element `first` finds in it;
 * Esc, a click elsewhere, or focus leaving it closes it, and `close` puts focus back on the
 * button.
 */
export function usePopup(first: string) {
  const [open, setOpen] = useState(false);
  /** Where the list opens: under the button, or above it when only there is it in view. */
  const [side, setSide] = useState<'bottom' | 'top'>('bottom');
  const box = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);

  // Measured before the list is painted, so it never shows out of view first.
  useLayoutEffect(() => {
    const button = trigger.current;
    const menu = list.current;
    if (!open || !button || !menu || !box.current) return;
    const room = visibleArea(box.current);
    const at = button.getBoundingClientRect();
    const height = menu.getBoundingClientRect().height + 4;
    const below = room.bottom - at.bottom;
    const above = at.top - room.top;
    if (height <= below) setSide('bottom');
    else if (height <= above) setSide('top');
    else {
      // Room neither way (a very small window): under the button, and the page scrolls to it.
      setSide('bottom');
      menu.scrollIntoView({ block: 'nearest' });
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;
    box.current?.querySelector<HTMLElement>(first)?.focus();
    const away = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', away);
    return () => {
      document.removeEventListener('mousedown', away);
    };
  }, [open, first]);

  const close = useCallback(() => {
    setOpen(false);
    trigger.current?.focus();
  }, []);

  return { open, setOpen, side, box, trigger, list, close };
}

/** The box of an open popup's list, by the edge of its button it lines up with. */
export function popupClass(side: 'bottom' | 'top', align: 'left' | 'right') {
  return `absolute z-30 flex max-h-80 w-max max-w-80 min-w-48 flex-col overflow-y-auto rounded-xl border border-[var(--line)] bg-[var(--raised)] p-1 shadow-[var(--shadow)] ${
    align === 'left' ? 'left-0' : 'right-0'
  } ${side === 'top' ? 'bottom-full mb-1.5' : 'top-full mt-1.5'}`;
}

/** A button that shows a short list of actions (see `usePopup`). */
export function MenuButton({
  label,
  children,
  items,
  align = 'right',
  className = '',
}: {
  /** The button's full name for screen readers ("More for “Brand”"). */
  label: string;
  children: ReactNode;
  items: readonly MenuItem[];
  /** Which edge of the button the list lines up with. */
  align?: 'left' | 'right';
  className?: string;
}) {
  const id = useId();
  const { open, setOpen, side, box, trigger, list, close } = usePopup('[data-item]');

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
          e.stopPropagation();
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
        onClick={(e) => {
          e.stopPropagation();
          setOpen((o) => !o);
        }}
        className={
          className ||
          'inline-flex min-h-10 min-w-10 items-center justify-center gap-1.5 rounded-lg px-2 text-small font-semibold text-[var(--muted)] hover:bg-[var(--hover)] hover:text-[var(--text)]'
        }
      >
        {children}
      </button>
      {open ? (
        <div ref={list} id={id} data-side={side} className={popupClass(side, align)}>
          {items.map((item) => (
            <button
              key={item.key ?? item.label}
              type="button"
              data-item
              onClick={(e) => {
                e.stopPropagation();
                setOpen(false);
                item.onSelect();
              }}
              className={`flex min-h-10 items-center gap-2.5 rounded-lg px-3 py-2 text-left text-small font-semibold hover:bg-[var(--hover)] focus-visible:-outline-offset-2 ${
                item.danger ? 'text-[var(--danger)]' : ''
              }`}
            >
              {item.icon}
              {item.detail ? (
                <span className="flex min-w-0 flex-col">
                  <span>{item.label}</span>
                  <span className="truncate text-caption font-normal text-[var(--muted)]">{item.detail}</span>
                </span>
              ) : (
                item.label
              )}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

/** Says something to screen readers only: what happened is shown where it happened, and this
 * makes sure it is heard too (a workspace renamed, a shortcut saved). */
type Announce = (text: string) => void;

const AnnounceContext = createContext<Announce>(() => undefined);

export function useAnnounce(): Announce {
  return useContext(AnnounceContext);
}

export function Announcer({ children }: { children: ReactNode }) {
  const [text, setText] = useState('');
  const n = useRef(0);
  const announce = useCallback<Announce>((t) => {
    n.current += 1;
    // A repeat of the same words is still announced: the region changes each time.
    setText(`${t}${n.current % 2 ? '' : '​'}`);
  }, []);
  return (
    <AnnounceContext.Provider value={announce}>
      {children}
      <p role="status" className="sr-only">
        {text}
      </p>
    </AnnounceContext.Provider>
  );
}
