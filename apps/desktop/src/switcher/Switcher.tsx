import { useCallback, useEffect, useId, useMemo, useRef, useState } from 'react';

import type { WorkspaceSummary } from '../protocol';

export interface SwitcherProps {
  workspaces: WorkspaceSummary[];
  active: string | null;
  /** The shortcut that opens the switcher, as Windows names it ("Win+Alt+Space"). */
  shortcut: string;
  onOpen: (workspace: WorkspaceSummary) => void;
  onSave: () => void;
  onClose: () => void;
}

/** Keys that pick the n-th workspace: 1 to 9, then F1 and up (F1 is the 10th). */
export function indexForKey(key: string): number | null {
  if (/^[1-9]$/.test(key)) return Number(key) - 1;
  const f = /^F([1-9]|1[0-2])$/.exec(key);
  if (f?.[1] !== undefined) return 9 + Number(f[1]) - 1;
  return null;
}

export function keyLabelFor(index: number): string | null {
  if (index < 9) return String(index + 1);
  if (index < 21) return `F${index - 8}`;
  return null;
}

/**
 * The workspace switcher: a filterable list with a key for each workspace. It has the keyboard
 * while open, so bare keys are safe: 1 to 9 and F1 up pick, arrows move, Enter opens, S saves,
 * Esc closes, and typing filters.
 */
export function Switcher({ workspaces, active, shortcut, onOpen, onSave, onClose }: SwitcherProps) {
  const [filter, setFilter] = useState('');
  const [selected, setSelected] = useState(0);
  const listId = useId();
  const inputRef = useRef<HTMLInputElement>(null);

  const shown = useMemo(() => {
    const q = filter.trim().toLocaleLowerCase();
    return q ? workspaces.filter((w) => w.name.toLocaleLowerCase().includes(q)) : workspaces;
  }, [filter, workspaces]);
  // Kept in range here rather than in an effect: the list can shrink under the selection.
  const current = Math.min(selected, Math.max(shown.length - 1, 0));

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      const pick = (i: number) => {
        const w = shown[i];
        if (w) onOpen(w);
      };
      if (e.key === 'Escape') {
        e.preventDefault();
        if (filter) setFilter('');
        else onClose();
        return;
      }
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        const step = e.key === 'ArrowDown' ? 1 : -1;
        setSelected(shown.length ? (current + step + shown.length) % shown.length : 0);
        return;
      }
      if (e.key === 'Enter') {
        e.preventDefault();
        pick(current);
        return;
      }
      // While filtering, digits and S are text; otherwise they are shortcuts.
      if (filter === '' && !e.ctrlKey && !e.altKey && !e.metaKey) {
        const index = indexForKey(e.key);
        if (index !== null) {
          e.preventDefault();
          pick(index);
          return;
        }
        if (e.key === 's' || e.key === 'S') {
          e.preventDefault();
          onSave();
        }
      }
    },
    [current, filter, onClose, onOpen, onSave, shown],
  );

  const optionId = (i: number) => `${listId}-option-${i}`;

  return (
    <div
      className="flex h-full flex-col overflow-hidden rounded-2xl border border-white/10 bg-[#1b1e26] text-[#e6e9f0] shadow-2xl"
      onKeyDown={onKeyDown}
    >
      <div className="flex h-13 items-center gap-3 border-b border-white/10 px-4">
        <input
          ref={inputRef}
          value={filter}
          onChange={(e) => {
            setFilter(e.target.value);
            setSelected(0);
          }}
          className="min-w-0 flex-1 bg-transparent text-[15px] text-[#e6e9f0] placeholder:text-[#8e97aa] focus:outline-none"
          placeholder="Switch workspace"
          aria-label="Filter workspaces"
          role="combobox"
          aria-expanded="true"
          aria-controls={listId}
          aria-activedescendant={shown.length ? optionId(current) : undefined}
          spellCheck={false}
        />
        <Keys keys={shortcut.split('+')} />
      </div>
      {shown.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 px-8 text-center">
          <p className="font-display text-lg font-semibold">
            {workspaces.length === 0 ? 'No workspaces yet' : 'No workspace matches'}
          </p>
          <p className="text-[13px] text-[#8e97aa]">
            {workspaces.length === 0
              ? 'Arrange your windows the way you like them, then press S to save this layout as a workspace.'
              : 'Try another name, or press Esc to clear the filter.'}
          </p>
        </div>
      ) : (
        <ul id={listId} role="listbox" aria-label="Workspaces" className="flex-1 overflow-y-auto p-2">
          {shown.map((w, i) => {
            const key = filter === '' ? keyLabelFor(i) : null;
            const isSelected = i === current;
            return (
              <li
                key={w.id}
                id={optionId(i)}
                role="option"
                aria-selected={isSelected}
                className={`relative flex cursor-default items-center gap-4 rounded-[10px] px-3 py-2.5 ${isSelected ? 'bg-[#272b37]' : ''}`}
                onMouseMove={() => {
                  setSelected(i);
                }}
                onClick={() => {
                  onOpen(w);
                }}
              >
                {isSelected && <span className="absolute inset-y-2.5 left-0 w-[3px] rounded-full bg-[#7c93ff]" />}
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[15px] font-semibold">
                    {w.name}
                    {w.id === active && <span className="ml-2 text-[12px] font-normal text-[#8e97aa]">current</span>}
                  </p>
                  <p className="text-[12.5px] text-[#8e97aa]">
                    {w.windows} {w.windows === 1 ? 'window' : 'windows'} · {w.zones} {w.zones === 1 ? 'zone' : 'zones'}
                  </p>
                </div>
                {key && <Keys keys={[key]} dim />}
              </li>
            );
          })}
        </ul>
      )}
      <div className="flex items-center gap-5 border-t border-white/10 px-4 py-2.5 text-[12px] text-[#8e97aa]">
        <span>
          <b className="text-[#c9ceda]">Enter</b> open
        </span>
        <span>
          <b className="text-[#c9ceda]">S</b> save current layout
        </span>
        <span>
          <b className="text-[#c9ceda]">Esc</b> close
        </span>
      </div>
    </div>
  );
}

function Keys({ keys, dim = false }: { keys: string[]; dim?: boolean }) {
  return (
    <span className="flex gap-1.5" aria-hidden="true">
      {keys.map((k) => (
        <span
          key={k}
          className={`rounded-md border border-white/15 px-1.5 py-0.5 text-[11.5px] ${dim ? 'text-[#8e97aa]' : 'text-[#e6e9f0]'}`}
        >
          {k}
        </span>
      ))}
    </span>
  );
}
