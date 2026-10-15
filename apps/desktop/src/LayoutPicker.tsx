// The layouts Arrange can use, under the picture of the screen: Automatic (Mosaïk lays the
// windows out for what they are for), then the five most used on a landscape screen, by name, as
// the side menu's strip has them, and the other five under More layouts. The one chosen always
// shows with its name: one of the other five takes the place of More layouts' words. Radio
// buttons to the keyboard and screen readers: the arrow keys go from one to the next, and the
// picture follows.

import { useId } from 'react';

import { PRESETS, type Preset, presetShape } from './editor/model';
import { Glyph } from './icons';
import { LayoutGlyph } from './Layouts';
import { popupClass, usePopup } from './ui';

/** What Arrange will do: lay the windows out by itself (`auto`), or in a layout. */
export type Choice = Preset | 'auto';

/** The side menu's strip (crates/mosaik-engine/src/quick.rs, `LAYOUTS`). */
const SHOWN: readonly Choice[] = ['auto', 'halves', 'thirds', 'main-side', 'half-stack', 'quarters'];
const MORE: readonly Preset[] = ['side-main', 'stack-half', 'rows', 'three-rows', 'full'];

/** A choice's name: "Automatic", "Halves"… */
export function choiceName(choice: Choice): string {
  return choice === 'auto' ? 'Automatic' : (PRESETS.find((p) => p.id === choice)?.label ?? choice);
}

const CHOSEN = 'bg-[var(--raised)] text-[var(--text)] ring-1 ring-[var(--accent)] forced-colors:ring-[Highlight]';
const OTHER = 'text-[var(--muted)] hover:bg-[var(--panel)] hover:text-[var(--text)]';

/** A layout drawn small, in its colours when it is the one chosen; Automatic is a spark. */
function Picture({ choice, on }: { choice: Choice; on: boolean }) {
  return choice === 'auto' ? (
    <Glyph name="spark" size={15} className={on ? 'text-[var(--accent)]' : ''} />
  ) : (
    <LayoutGlyph shape={presetShape(choice)} width={20} height={13} colored={on} />
  );
}

/** One layout: a radio button over the whole of it, unseen, so a click anywhere on it chooses it
 * and a screen reader's focus box is the layout, not a dot. */
function Option({
  group,
  choice,
  on,
  disabled = false,
  first = false,
  row = false,
  onChoose,
  onClick,
}: {
  group: string;
  choice: Choice;
  on: boolean;
  disabled?: boolean;
  /** Takes the focus when its list opens. */
  first?: boolean;
  /** A line of a list, rather than a chip. */
  row?: boolean;
  onChoose: (choice: Choice) => void;
  onClick?: (pointer: boolean) => void;
}) {
  const shape = row ? 'rounded-lg' : 'rounded-full';
  return (
    <label
      className={`relative inline-flex h-10 min-w-11 shrink-0 items-center gap-1.5 px-2.5 text-small font-semibold transition-colors duration-150 has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-[var(--focus)] has-[:disabled]:opacity-50 ${shape} ${
        on ? CHOSEN : OTHER
      }`}
    >
      <input
        type="radio"
        name={group}
        value={choice}
        checked={on}
        disabled={disabled}
        data-first={first || undefined}
        onChange={() => {
          onChoose(choice);
        }}
        onClick={(e) => {
          // A click of the pointer, rather than the arrow keys or Space.
          onClick?.(e.detail > 0);
        }}
        className={`absolute inset-0 m-0 cursor-pointer appearance-none opacity-0 disabled:cursor-default ${shape}`}
      />
      <Picture choice={choice} on={on} />
      <span className="whitespace-nowrap">{choiceName(choice)}</span>
    </label>
  );
}

/** The other five, in a list under a button; the one chosen among them in the button's place.
 * The arrow keys choose in the list, as in the row; a click, Enter or Esc closes it. */
function MoreLayouts({
  value,
  disabled,
  onChoose,
}: {
  value: Choice | null;
  disabled: boolean;
  onChoose: (choice: Choice) => void;
}) {
  const id = useId();
  const group = useId();
  const { open, setOpen, side, box, trigger, list, close } = usePopup('[data-first]');
  const chosen = MORE.find((p) => p === value) ?? null;
  return (
    <div
      ref={box}
      className="relative"
      onKeyDown={(e) => {
        if (open && (e.key === 'Escape' || e.key === 'Enter')) {
          e.preventDefault();
          e.stopPropagation();
          close();
        }
      }}
      onBlur={(e) => {
        if (!box.current?.contains(e.relatedTarget)) setOpen(false);
      }}
    >
      <button
        ref={trigger}
        type="button"
        disabled={disabled}
        aria-expanded={open}
        aria-controls={id}
        aria-label={chosen ? `More layouts: ${choiceName(chosen)}` : undefined}
        onClick={() => {
          setOpen((o) => !o);
        }}
        className={`inline-flex h-10 shrink-0 items-center gap-1.5 rounded-full px-2.5 text-small font-semibold whitespace-nowrap transition-colors duration-150 disabled:opacity-50 ${
          chosen ? CHOSEN : OTHER
        }`}
      >
        {chosen ? (
          <>
            <Picture choice={chosen} on />
            {choiceName(chosen)}
          </>
        ) : (
          'More layouts'
        )}
        <Glyph name="down" size={14} />
      </button>
      {open ? (
        <div ref={list} id={id} data-side={side} className={popupClass(side, 'left')}>
          <fieldset className="flex flex-col gap-0.5">
            <legend className="sr-only">More layouts</legend>
            {MORE.map((p, i) => (
              <Option
                key={p}
                group={group}
                choice={p}
                on={chosen === p}
                first={chosen ? chosen === p : i === 0}
                row
                onChoose={onChoose}
                onClick={(pointer) => {
                  if (pointer) close();
                }}
              />
            ))}
          </fieldset>
        </div>
      ) : null}
    </div>
  );
}

export function LayoutPicker({
  value,
  disabled,
  onChoose,
}: {
  /** The layout chosen; none while a workspace is shown instead. */
  value: Choice | null;
  /** Nothing to arrange. */
  disabled: boolean;
  onChoose: (choice: Choice) => void;
}) {
  const group = useId();
  return (
    <fieldset className="min-w-0">
      <legend className="sr-only">Layout</legend>
      <div className="flex flex-wrap items-center gap-0.5">
        {SHOWN.map((choice) => (
          <Option
            key={choice}
            group={group}
            choice={choice}
            on={value === choice}
            disabled={disabled}
            onChoose={onChoose}
          />
        ))}
        <MoreLayouts value={value} disabled={disabled} onChoose={onChoose} />
      </div>
    </fieldset>
  );
}
