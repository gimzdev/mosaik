// "Add an app", on a workspace's page (ADR 0033; the owner: "add an open window" becomes "add an
// app" and proposes even apps that are not open; "why even specify "not open"": one list): the
// apps it could take, by name, with a box to find one. An app open now joins with its window
// (two windows of one app are told apart by their titles); one that is not starts when the
// workspace opens. The arrow keys go from the box to the apps and back; Esc closes the list.

import { useId, useState } from 'react';

import { AppIcon, Glyph } from './icons';
import type { InstalledApp, OpenWindow } from './protocol';
import { usePopup } from './ui';

const ITEM =
  'flex min-h-10 w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-small font-semibold hover:bg-[var(--hover)] focus-visible:-outline-offset-2';

/** An app of the list: a window open now, or an app of the Start menu. */
type Choice = { key: string; label: string; detail: string | null; icon: string | null } & (
  { window: OpenWindow } | { app: InstalledApp }
);

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
  const twice = (label: string) => windows.filter((w) => w.label === label).length > 1;
  const choices: Choice[] = [
    ...windows.map((w) => ({
      key: `w${String(w.window)}`,
      label: w.label,
      detail: twice(w.label) && w.title ? w.title : null,
      icon: w.icon,
      window: w,
    })),
    ...installed.map((a) => ({ key: `a${a.app}`, label: a.label, detail: null, icon: a.icon, app: a })),
  ]
    .filter((c) => !q || c.label.toLowerCase().includes(q) || (c.detail ?? '').toLowerCase().includes(q))
    .sort((a, b) => a.label.localeCompare(b.label, undefined, { sensitivity: 'base' }));

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
          <div role="group" aria-label="Apps to add" className="mt-1 flex min-h-0 flex-col overflow-y-auto">
            {choices.map((c) => (
              <button
                key={c.key}
                type="button"
                data-item
                className={ITEM}
                onClick={() => {
                  pick(() => {
                    if ('window' in c) onWindow(c.window);
                    else onInstalled(c.app);
                  });
                }}
              >
                <AppIcon icon={c.icon} size={18} keepSpace />
                {c.detail === null ? (
                  c.label
                ) : (
                  <span className="flex min-w-0 flex-col">
                    <span>{c.label}</span>
                    <span className="truncate text-caption font-normal text-[var(--muted)]">{c.detail}</span>
                  </span>
                )}
              </button>
            ))}
            {choices.length === 0 ? (
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
