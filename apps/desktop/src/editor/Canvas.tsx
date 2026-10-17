// One monitor's work area with its zones: pick a zone, drag the lines between zones (or focus a
// line and use the arrow keys; `LineHandle`).

import { useRef } from 'react';

import type { MonitorState } from '../protocol';
import { zoneChip, zoneTint } from '../ui';
import { LineHandle } from './LineHandle';
import { type Draft, lines, percent } from './model';

/** The canvas at most this many percent of the window's height (the zone editor's page takes its
 * width). */
export const CANVAS_TALLEST = 46;

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
        style={{
          aspectRatio: `${String(w)} / ${String(h)}`,
          width: `min(100%, calc(${String(CANVAS_TALLEST)}vh * ${String(w / h)}))`,
        }}
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
