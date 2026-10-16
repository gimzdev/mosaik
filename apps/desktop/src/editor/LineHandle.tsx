// A line between zones, to drag (or to move with the arrow keys once focused): the zones on each
// side follow it, on a grid of 48ths, and never get smaller than Mosaïk allows. At rest it is the
// gap between the zones and no more; the pointer over it turns into a resize arrow and a thin line
// shows, as it does while it is dragged or has the keyboard's focus (the owner, of the bars drawn
// there before: "way too large", "either discreet or not shown"). The zone editor and the picture
// of the screen (Home, a workspace's page) both use it.

import { type KeyboardEvent, type PointerEvent, type RefObject, useRef, useState } from 'react';

import { type Draft, GRID, type Line, lineLabel, lineRange, moveLine, percent, snap } from './model';

const KEYS = new Set(['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End']);

export function LineHandle({
  line,
  drafts,
  monitor,
  area,
  onChange,
  onEnd,
}: {
  line: Line;
  drafts: Draft[];
  monitor: string;
  /** The element that is the monitor's work area: where the pointer is, as a fraction of it. */
  area: RefObject<HTMLDivElement | null>;
  onChange: (next: Draft[]) => void;
  /** The line was let go of, or the keys that moved it were: where it is now is meant. */
  onEnd?: (() => void) | undefined;
}) {
  /** The drafts and the line as they were when the drag started (every move starts from them,
   * and Esc puts them back), and whether it has moved since: a press that moved nothing, or a
   * drag put back with Esc, ends nothing. */
  const dragging = useRef<{ drafts: Draft[]; line: Line; moved: boolean } | null>(null);
  /** The keys moved it since they were last let go of. */
  const keyed = useRef(false);
  /** Held by the pointer: the line shows until it is let go of. */
  const [held, setHeld] = useState(false);
  const vertical = line.axis === 'x';
  const range = lineRange(drafts, line, monitor);

  const at = (e: PointerEvent<HTMLDivElement>) => {
    const box = area.current?.getBoundingClientRect();
    if (!box || box.width <= 0 || box.height <= 0) return null;
    return vertical ? (e.clientX - box.left) / box.width : (e.clientY - box.top) / box.height;
  };

  const cancel = () => {
    setHeld(false);
    if (!dragging.current) return;
    onChange(dragging.current.drafts);
    dragging.current = null;
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === 'Escape' && dragging.current) {
      e.preventDefault();
      e.stopPropagation();
      cancel();
      return;
    }
    const step = (e.shiftKey ? 4 : 1) * GRID;
    const less = vertical ? 'ArrowLeft' : 'ArrowUp';
    const more = vertical ? 'ArrowRight' : 'ArrowDown';
    const to =
      e.key === less
        ? line.at - step
        : e.key === more
          ? line.at + step
          : e.key === 'Home'
            ? range.min
            : e.key === 'End'
              ? range.max
              : null;
    if (to === null) return;
    e.preventDefault();
    const next = moveLine(drafts, line, monitor, e.key === 'Home' || e.key === 'End' ? to : snap(to));
    if (next === drafts) return;
    keyed.current = true;
    onChange(next);
  };

  // 16 pixels to grab, over the gap between the zones and a little of each.
  const style = vertical
    ? {
        left: `calc(${String(line.at * 100)}% - 8px)`,
        top: `${String(line.from * 100)}%`,
        width: '16px',
        height: `${String((line.to - line.from) * 100)}%`,
      }
    : {
        top: `calc(${String(line.at * 100)}% - 8px)`,
        left: `${String(line.from * 100)}%`,
        height: '16px',
        width: `${String((line.to - line.from) * 100)}%`,
      };

  return (
    <div
      role="separator"
      tabIndex={0}
      aria-orientation={vertical ? 'vertical' : 'horizontal'}
      aria-label={lineLabel(drafts, line)}
      aria-valuemin={Math.round(range.min * 100)}
      aria-valuemax={Math.round(range.max * 100)}
      aria-valuenow={Math.round(line.at * 100)}
      aria-valuetext={percent(line.at)}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        e.preventDefault();
        e.currentTarget.setPointerCapture(e.pointerId);
        e.currentTarget.focus();
        dragging.current = { drafts, line, moved: false };
        setHeld(true);
      }}
      onPointerMove={(e) => {
        const start = dragging.current;
        const v = at(e);
        if (!start || v === null) return;
        const next = moveLine(start.drafts, start.line, monitor, snap(v));
        if (next !== start.drafts) start.moved = true;
        onChange(next);
      }}
      onPointerUp={() => {
        setHeld(false);
        const start = dragging.current;
        dragging.current = null;
        if (start?.moved) onEnd?.();
      }}
      onPointerCancel={cancel}
      onKeyDown={onKeyDown}
      onKeyUp={(e) => {
        if (!KEYS.has(e.key) || !keyed.current || dragging.current) return;
        keyed.current = false;
        onEnd?.();
      }}
      className={`group absolute z-10 flex touch-none items-center justify-center rounded-full ${
        vertical ? 'cursor-col-resize' : 'cursor-row-resize'
      }`}
      style={style}
    >
      <span
        aria-hidden="true"
        className={`rounded-full bg-[var(--desk-label)] transition-opacity duration-150 group-hover:opacity-70 group-focus-visible:bg-[var(--focus)] group-focus-visible:opacity-100 forced-colors:bg-[CanvasText] ${
          held ? 'opacity-100' : 'opacity-0'
        } ${vertical ? 'h-full w-0.5 group-focus-visible:w-[3px]' : 'h-0.5 w-full group-focus-visible:h-[3px]'}`}
      />
    </div>
  );
}
