// "Add an app", on a workspace's page (ADR 0033; the owner: "add an open window" becomes "add an
// app" and proposes even apps that are not open): the windows open now that the workspace does
// not have, then the apps of the Start menu that are not open, with a box to find one by its
// name. An open window's app joins with its window; an app not open starts when the workspace
// opens. The arrow keys go from the box to the apps and back; Esc closes the list.

import { type ReactNode, useId, useState } from 'react';

import { AppIcon, Glyph } from './icons';
import type { InstalledApp, OpenWindow } from './protocol';
import { usePopup } from './ui';

/** Whether a window's title says no more than its app's name ("Battle.net" and "Battle․net"). */
export const sameName = (title: string, label: string) =>
  !title.trim() || title.normalize('NFKC').trim().toLowerCase() === label.normalize('NFKC').trim().toLowerCase();

const ITEM =
  'flex min-h-10 w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-small font-semibold hover:bg-[var(--hover)] focus-visible:-outline-offset-2';

/** One group of the list: its heading, then its apps. */
function Group({ title, children }: { title: string; children: ReactNode }) {
  const id = useId();
  return (
    <div role="group" aria-labelledby={id} className="flex flex-col">
      <p id={id} className="px-3 pt-2 pb-1 text-caption font-semibold text-[var(--muted)]">
        {title}
      </p>
      {children}
    </div>
  );
}

export function AddApp({
  name,
  windows,
  installed,
  onWindow,
  onInstalled,
}: {
  /** The workspace's name. */
  name: string;
  windows: readonly OpenWindow[];
  installed: readonly InstalledApp[];
  onWindow: (window: OpenWindow) => void;
  onInstalled: (app: InstalledApp) => void;
}) {
  const id = useId();
  const [query, setQuery] = useState('');
  const { open, setOpen, side, box, trigger, list, close } = usePopup('input');
  const q = query.trim().toLowerCase();
  const hits = (...words: string[]) => !q || words.some((w) => w.toLowerCase().includes(q));
  const openNow = windows.filter((w) => hits(w.label, w.title));
  const notOpen = installed.filter((a) => hits(a.label));

  const shut = () => {
    setQuery('');
    close();
  };
  const pick = (then: () => void) => {
    shut();
    then();
  };

  return (
    <div
      ref={box}
      className="relative"
      onKeyDown={(e) => {
        if (!open) return;
        if (e.key === 'Escape') {
          e.preventDefault();
          e.stopPropagation();
          shut();
          return;
        }
        if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
        e.preventDefault();
        const all = Array.from(list.current?.querySelectorAll<HTMLElement>('input, [data-item]') ?? []);
        const at = all.indexOf(document.activeElement as HTMLElement);
        const next = all[(at + (e.key === 'ArrowDown' ? 1 : all.length - 1)) % all.length];
        next?.focus();
      }}
      onBlur={(e) => {
        if (!box.current?.contains(e.relatedTarget)) {
          setOpen(false);
          setQuery('');
        }
      }}
    >
      <button
        ref={trigger}
        type="button"
        aria-label={`Add an app to “${name}”`}
        aria-expanded={open}
        aria-controls={id}
        onClick={(e) => {
          e.stopPropagation();
          setQuery('');
          setOpen((o) => !o);
        }}
        className="inline-flex min-h-10 items-center gap-1.5 rounded-lg px-3 text-small font-semibold text-[var(--accent)] hover:bg-[var(--hover)]"
      >
        <Glyph name="plus" size={14} />
        Add an app
      </button>
      {open ? (
        <div
          ref={list}
          id={id}
          data-side={side}
          className={`absolute left-0 z-30 flex max-h-96 w-80 max-w-[calc(100vw-2rem)] flex-col rounded-xl border border-[var(--line)] bg-[var(--raised)] p-1 shadow-[var(--shadow)] ${
            side === 'top' ? 'bottom-full mb-1.5' : 'top-full mt-1.5'
          }`}
        >
          <label className="flex min-h-10 shrink-0 items-center gap-2 rounded-lg border border-[var(--line)] px-2.5 focus-within:outline-2 focus-within:outline-offset-1 focus-within:outline-[var(--focus)]">
            <Glyph name="search" size={14} className="text-[var(--muted)]" />
            <input
              type="search"
              aria-label="Find an app"
              placeholder="Find an app"
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
              }}
              className="min-w-0 flex-1 bg-transparent py-2 text-small outline-none placeholder:text-[var(--muted)]"
            />
          </label>
          <div className="mt-1 flex min-h-0 flex-col overflow-y-auto">
            {openNow.length ? (
              <Group title="Open now">
                {openNow.map((w) => (
                  <button
                    key={`w${String(w.window)}`}
                    type="button"
                    data-item
                    className={ITEM}
                    onClick={() => {
                      pick(() => {
                        onWindow(w);
                      });
                    }}
                  >
                    <AppIcon icon={w.icon} size={18} keepSpace />
                    {sameName(w.title, w.label) ? (
                      w.label
                    ) : (
                      <span className="flex min-w-0 flex-col">
                        <span>{w.label}</span>
                        <span className="truncate text-caption font-normal text-[var(--muted)]">{w.title}</span>
                      </span>
                    )}
                  </button>
                ))}
              </Group>
            ) : null}
            {notOpen.length ? (
              <Group title="Not open">
                {notOpen.map((a) => (
                  <button
                    key={`a${a.app}`}
                    type="button"
                    data-item
                    className={ITEM}
                    onClick={() => {
                      pick(() => {
                        onInstalled(a);
                      });
                    }}
                  >
                    <AppIcon icon={a.icon} size={18} keepSpace />
                    {a.label}
                  </button>
                ))}
              </Group>
            ) : null}
            {!openNow.length && !notOpen.length ? (
              <p className="px-3 py-3 text-small text-[var(--muted)]">
                {q ? `No app called “${query.trim()}”.` : 'Every app is in it already.'}
              </p>
            ) : null}
          </div>
        </div>
      ) : null}
    </div>
  );
}
