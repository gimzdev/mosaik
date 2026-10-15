// One monitor's work area with its zones: pick a zone, drag the bars between zones (or focus a
// bar and use the arrow keys). Bars snap to a grid of 48ths. They show at rest, a little faded,
// so they can be found without pointing at them first.

import { type KeyboardEvent, type PointerEvent, type RefObject, useRef } from 'react';

import type { MonitorState } from '../protocol';
import { zoneChip, zoneTint } from '../ui';
import { type Draft, GRID, type Line, lineLabel, lineRange, lines, moveLine, percent, snap } from './model';

function LineHandle({
  line,
  drafts,
  monitor,
  area,
  onChange,
}: {
  line: Line;
  drafts: Draft[];
  monitor: string;
  area: RefObject<HTMLDivElement | null>;
  onChange: (next: Draft[]) => void;
}) {
  /** The drafts and the line as they were when the drag started: every move starts from them,
   * and Esc puts them back. */
  const dragging = useRef<{ drafts: Draft[]; line: Line } | null>(null);
  const vertical = line.axis === 'x';
  const range = lineRange(drafts, line, monitor);

  const at = (e: PointerEvent<HTMLDivElement>) => {
    const box = area.current?.getBoundingClientRect();
    if (!box) return null;
    return vertical ? (e.clientX - box.left) / box.width : (e.clientY - box.top) / box.height;
  };

  const cancel = () => {
    if (!dragging.current) return;
    onChange(dragging.current.drafts);
    dragging.current = null;
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === 'Escape' && dragging.current) {
      e.preventDefault();
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
    onChange(moveLine(drafts, line, monitor, e.key === 'Home' || e.key === 'End' ? to : snap(to)));
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
        dragging.current = { drafts, line };
      }}
      onPointerMove={(e) => {
        const start = dragging.current;
        const v = at(e);
        if (!start || v === null) return;
        onChange(moveLine(start.drafts, start.line, monitor, snap(v)));
      }}
      onPointerUp={() => {
        dragging.current = null;
      }}
      onPointerCancel={cancel}
      onKeyDown={onKeyDown}
      className={`group absolute z-10 flex touch-none items-center justify-center rounded-full ${
        vertical ? 'cursor-col-resize' : 'cursor-row-resize'
      }`}
      style={style}
    >
      <span
        aria-hidden="true"
        className={`rounded-full bg-[var(--desk-label)] opacity-60 transition-opacity duration-150 group-hover:opacity-100 group-focus-visible:bg-[var(--focus)] group-focus-visible:opacity-100 forced-colors:bg-[CanvasText] forced-colors:opacity-100 ${
          vertical ? 'h-full max-h-20 w-1.5' : 'h-1.5 w-full max-w-20'
        }`}
      />
    </div>
  );
}

export function Canvas({
  monitor,
  drafts,
  selected,
  onSelect,
  onChange,
}: {
  monitor: MonitorState;
  drafts: Draft[];
  selected: string | null;
  onSelect: (key: string) => void;
  onChange: (next: Draft[]) => void;
}) {
  const area = useRef<HTMLDivElement>(null);
  const here = drafts.filter((d) => d.monitor === monitor.id);
  const w = monitor.work_area.right - monitor.work_area.left;
  const h = monitor.work_area.bottom - monitor.work_area.top;

  return (
    <div className="flex justify-center" data-canvas>
      <div
        ref={area}
        className="sky relative overflow-hidden rounded-[18px] border border-[var(--line)] select-none"
        // As wide as the page, or less for a tall monitor, so the zone's details stay in view.
        style={{ aspectRatio: `${String(w)} / ${String(h)}`, width: `min(100%, calc(46vh * ${String(w / h)}))` }}
      >
        {here.length === 0 ? (
          <p className="absolute inset-0 flex items-center justify-center p-6 text-center text-small text-[var(--desk-label)]">
            No zones on this monitor. Start from a layout.
          </p>
        ) : null}
        {here.map((z) => {
          const a = z.area;
          const on = z.key === selected;
          return (
            <button
              key={z.key}
              type="button"
              aria-pressed={on}
              aria-label={`Zone ${z.name}, ${percent(a.right - a.left)} wide, ${percent(a.bottom - a.top)} high`}
              onClick={() => {
                onSelect(z.key);
              }}
              className={`absolute flex items-start justify-start overflow-hidden rounded-lg p-2 text-left transition-[background-color] duration-150 ${
                on
                  ? 'forced-colors:outline forced-colors:outline-4 forced-colors:outline-offset-[-6px] forced-colors:outline-[Highlight]'
                  : ''
              }`}
              style={{
                left: `calc(${String(a.left * 100)}% + 3px)`,
                top: `calc(${String(a.top * 100)}% + 3px)`,
                width: `calc(${String((a.right - a.left) * 100)}% - 6px)`,
                height: `calc(${String((a.bottom - a.top) * 100)}% - 6px)`,
                border: `${on ? '3px' : '2px'} solid ${zoneTint(z.color)}`,
                background: `color-mix(in srgb, ${zoneTint(z.color)} ${on ? '24%' : '12%'}, transparent)`,
              }}
            >
              <span
                className="zone-chip grid min-w-6 place-items-center rounded-full px-2 py-0.5 text-caption font-bold tabular-nums"
                style={zoneChip(z.color)}
              >
                {z.name}
              </span>
            </button>
          );
        })}
        {lines(drafts, monitor.id).map((line) => (
          <LineHandle key={line.id} line={line} drafts={drafts} monitor={monitor.id} area={area} onChange={onChange} />
        ))}
      </div>
    </div>
  );
}
