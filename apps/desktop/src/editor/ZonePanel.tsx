// The picked zone: its name and colour, splitting it in two, removing it.

import { useId } from 'react';

import { MAX_NAME, type ZoneColor } from '../protocol';
import { Button } from '../ui';
import { COLORS, type Draft } from './model';

const COLOR_NAMES: Record<ZoneColor, string> = {
  lapis: 'Lapis',
  verdigris: 'Verdigris',
  saffron: 'Saffron',
  madder: 'Madder',
  slate: 'Slate',
};

export function ZonePanel({
  zone,
  onRename,
  onColor,
  onSplit,
  onRemove,
}: {
  zone: Draft;
  onRename: (name: string) => void;
  onColor: (color: ZoneColor) => void;
  onSplit: (direction: 'columns' | 'rows') => void;
  onRemove: () => void;
}) {
  const nameId = useId();
  return (
    <section
      aria-label={`Zone ${zone.name.trim() || 'without a name'}`}
      className="flex flex-wrap items-end gap-x-6 gap-y-3 rounded-xl border border-[var(--line)] bg-[var(--panel)] p-4"
    >
      <div className="flex flex-col gap-1">
        <label htmlFor={nameId} className="text-small font-semibold">
          Name
        </label>
        <input
          id={nameId}
          value={zone.name}
          maxLength={MAX_NAME}
          aria-invalid={zone.name.trim() === ''}
          onChange={(e) => {
            onRename(e.target.value);
          }}
          className="field w-52"
        />
      </div>
      <fieldset>
        <legend className="text-small font-semibold">Colour</legend>
        <div className="mt-1 flex gap-1.5">
          {COLORS.map((c) => (
            <label key={c} className="cursor-pointer rounded-full">
              <input
                type="radio"
                name={`color-${zone.key}`}
                value={c}
                checked={zone.color === c}
                onChange={() => {
                  onColor(c);
                }}
                className="peer sr-only"
              />
              <span
                aria-hidden="true"
                title={COLOR_NAMES[c]}
                // The swatch keeps its colour in high contrast too, and the chosen one has a
                // check mark, so colour is never the only sign.
                className="flex h-8 w-8 items-center justify-center rounded-full border-2 border-transparent text-small font-bold forced-color-adjust-none peer-checked:border-[var(--text)] peer-focus-visible:outline peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-[var(--focus)]"
                style={{
                  background: `var(--swatch-${c})`,
                  color: c === 'saffron' ? 'var(--color-ink)' : 'var(--on-swatch)',
                }}
              >
                {zone.color === c ? '✓' : ''}
              </span>
              <span className="sr-only">{COLOR_NAMES[c]}</span>
            </label>
          ))}
        </div>
      </fieldset>
      <div className="flex flex-col gap-1">
        <span className="text-small font-semibold" id={`${nameId}-split`}>
          Split
        </span>
        <div role="group" aria-labelledby={`${nameId}-split`} className="flex gap-1.5">
          <Button
            label="Split side by side"
            onClick={() => {
              onSplit('columns');
            }}
          >
            Side by side
          </Button>
          <Button
            label="Split top and bottom"
            onClick={() => {
              onSplit('rows');
            }}
          >
            Top and bottom
          </Button>
        </div>
      </div>
      <div className="ml-auto">
        <Button tone="danger" onClick={onRemove}>
          Remove zone
        </Button>
      </div>
    </section>
  );
}
