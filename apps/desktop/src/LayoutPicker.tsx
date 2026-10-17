// The layouts Arrange can use, under the picture of the screen, all in view: Automatic (Mosaïk
// lays the windows out for what they are for), as tall as the two rows beside it, then the eight
// layouts in two rows of four, each drawn above its name: the columns first, then the stacks,
// the quarters and full screen (the owner, on 2026-10-15: "we dont want dropdown menu we want
// shorter list", "9 left fitting in 2 lines of options"). Radio buttons to the keyboard and
// screen readers: the arrow keys go from one to the next, and the picture follows.

import { useId } from 'react';

import { PRESETS, type Preset, presetShape } from './editor/model';
import { Glyph } from './icons';
import { LayoutGlyph } from './Layouts';

/** What Arrange will do: lay the windows out by itself (`auto`), or in a layout. */
export type Choice = Preset | 'auto';

/** The eight layouts, in reading order. */
const LAYOUTS: readonly Preset[] = [
  'halves',
  'thirds',
  'main-side',
  'side-main',
  'half-stack',
  'stack-half',
  'quarters',
  'full',
];

/** A choice's name: "Automatic", "Halves"… */
export function choiceName(choice: Choice): string {
  return choice === 'auto' ? 'Automatic' : (PRESETS.find((p) => p.id === choice)?.label ?? choice);
}

const CHOSEN = 'bg-[var(--raised)] text-[var(--text)] ring-1 ring-[var(--accent)] forced-colors:ring-[Highlight]';
const OTHER = 'text-[var(--muted)] hover:bg-[var(--panel)] hover:text-[var(--text)]';

/** One layout: its picture above its name, with a radio button over the whole of it, unseen, so
 * a click anywhere on it chooses it and a screen reader's focus box is the layout, not a dot. Its
 * name has the room to stay on one line at the window's smallest size; zoomed in, it may take
 * two. */
function Option({
  group,
  choice,
  on,
  disabled,
  title,
  onChoose,
}: {
  group: string;
  choice: Choice;
  on: boolean;
  disabled: boolean;
  /** What it says when pointed at: its name, or why it cannot be chosen. */
  title?: string | undefined;
  onChoose: (choice: Choice) => void;
}) {
  const auto = choice === 'auto';
  return (
    <label
      title={title ?? choiceName(choice)}
      className={`relative flex min-h-[54px] min-w-0 flex-col items-center justify-center gap-1 rounded-xl px-1 py-1.5 text-caption font-semibold transition-colors duration-150 has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-[var(--focus)] has-[:disabled]:opacity-50 ${
        auto ? 'row-span-2 px-3' : ''
      } ${on ? CHOSEN : OTHER}`}
    >
      <input
        type="radio"
        name={group}
        value={choice}
        checked={on}
        disabled={disabled}
        onChange={() => {
          onChoose(choice);
        }}
        className="absolute inset-0 m-0 cursor-pointer appearance-none rounded-xl opacity-0 disabled:cursor-default"
      />
      {auto ? (
        <Glyph name="spark" size={20} className={on ? 'text-[var(--accent)]' : ''} />
      ) : (
        <LayoutGlyph shape={presetShape(choice)} width={30} height={18} colored={on} />
      )}
      <span className="line-clamp-2 max-w-full text-center leading-tight">{choiceName(choice)}</span>
    </label>
  );
}

export function LayoutPicker({
  value,
  disabled,
  noAuto = false,
  onChoose,
}: {
  /** The layout chosen; none when the picture is in no layout. */
  value: Choice | null;
  /** Nothing to arrange. */
  disabled: boolean;
  /** For a workspace's layout: Automatic, which lays out the windows on screen, cannot be chosen
   * (ADR 0033). */
  noAuto?: boolean;
  onChoose: (choice: Choice) => void;
}) {
  const group = useId();
  return (
    <fieldset className="min-w-0">
      <legend className="sr-only">Layout</legend>
      <div className="grid grid-cols-[minmax(5.5rem,auto)_repeat(4,minmax(0,1fr))] gap-1">
        {(['auto', ...LAYOUTS] as const).map((choice) => (
          <Option
            key={choice}
            group={group}
            choice={choice}
            on={value === choice}
            disabled={disabled || (noAuto && choice === 'auto')}
            title={noAuto && choice === 'auto' ? 'Automatic lays out the windows on your screen' : undefined}
            onChoose={onChoose}
          />
        ))}
      </div>
    </fieldset>
  );
}
