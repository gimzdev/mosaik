// Notes: what Mosaïk had to say while it worked (another window manager is running, an app it
// cannot move, a workspace shortcut taken...). They wait quietly behind a button in the window's
// bar, with how many there are; nothing pops up over the page.

import { useEffect, useId, useRef, useState } from 'react';

import { Glyph } from './icons';
import type { Notice } from './protocol';

export interface Note extends Notice {
  id: number;
}

export function Notes({
  notes,
  onDismiss,
  onClear,
}: {
  notes: Note[];
  onDismiss: (id: number) => void;
  onClear: () => void;
}) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const box = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', away);
    return () => {
      document.removeEventListener('mousedown', away);
    };
  }, [open]);

  // With the last note gone, the button goes too: the list closes, and focus goes to the page's
  // title rather than nowhere.
  const closeAll = () => {
    setOpen(false);
    document.querySelector<HTMLElement>('main h1[tabindex="-1"]')?.focus();
  };

  if (notes.length === 0) return null;
  return (
    <div
      ref={box}
      className="relative"
      onKeyDown={(e) => {
        if (open && e.key === 'Escape') {
          e.preventDefault();
          setOpen(false);
          trigger.current?.focus();
        }
      }}
    >
      <button
        ref={trigger}
        type="button"
        aria-expanded={open}
        aria-controls={id}
        aria-label={`Notes from Mosaïk: ${String(notes.length)}`}
        onClick={() => {
          setOpen((o) => !o);
        }}
        className="inline-flex min-h-10 items-center gap-1.5 rounded-lg px-2.5 text-small font-semibold text-[var(--muted)] hover:bg-[var(--hover)] hover:text-[var(--text)]"
      >
        <Glyph name="bell" size={15} />
        <span className="grid min-w-[1.15rem] place-items-center rounded-full bg-[var(--accent-fill)] px-1 text-caption leading-[1.15rem] text-[var(--on-accent)]">
          {notes.length}
        </span>
      </button>
      {open ? (
        <section
          id={id}
          aria-label="Notes"
          className="absolute top-full right-0 z-40 mt-2 flex max-h-[70vh] w-[22rem] flex-col overflow-hidden rounded-xl border border-[var(--line)] bg-[var(--raised)] shadow-[var(--shadow)]"
        >
          <div className="flex items-center justify-between border-b border-[var(--line)] py-1 pr-1 pl-4">
            <h2 className="font-display text-heading font-semibold">Notes</h2>
            <button
              type="button"
              onClick={() => {
                closeAll();
                onClear();
              }}
              className="min-h-10 rounded-md px-3 text-small font-semibold text-[var(--accent)] hover:bg-[var(--hover)]"
            >
              Clear all
            </button>
          </div>
          <ul className="flex flex-col divide-y divide-[var(--line)] overflow-y-auto">
            {notes.map((n) => (
              <li key={n.id} className="flex items-start gap-3 px-4 py-3">
                <div className="min-w-0 flex-1">
                  <p className="text-small font-semibold">{n.title}</p>
                  <p className="mt-0.5 text-small text-[var(--muted)]">{n.body}</p>
                </div>
                <button
                  type="button"
                  aria-label={`Dismiss: ${n.title}`}
                  onClick={() => {
                    if (notes.length === 1) closeAll();
                    onDismiss(n.id);
                  }}
                  className="-my-1.5 grid size-10 shrink-0 place-items-center rounded-md text-[var(--muted)] hover:bg-[var(--hover)] hover:text-[var(--text)]"
                >
                  <Glyph name="close" size={13} />
                </button>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </div>
  );
}
