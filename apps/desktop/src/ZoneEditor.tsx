// The zone editor: one workspace's zones on each monitor connected now. Drag the lines between
// zones (or focus one and use the arrow keys) to resize; pick a zone to split, rename, recolour
// or remove it; start from a preset. Nothing changes until "Save zones".

import { type KeyboardEvent, type PointerEvent, useEffect, useId, useMemo, useRef, useState } from 'react';

import {
  COLORS,
  type Draft,
  type Edge,
  GRID,
  MAX_NAME,
  PRESETS,
  applyPreset,
  fromZones,
  percent,
  problem as draftProblem,
  remove,
  resize,
  snap,
  split,
  toZones,
  update,
} from './editor/model';
import { request } from './engine';
import { errorMessage, type MonitorState, type ZoneColor } from './protocol';
import { Button, Confirm, Status } from './ui';

const COLOR_NAMES: Record<ZoneColor, string> = {
  lapis: 'Lapis',
  verdigris: 'Verdigris',
  saffron: 'Saffron',
  madder: 'Madder',
  slate: 'Slate',
};

const EPS = 1e-6;

/** The edges with a handle: every edge not on the monitor's border, once per line (a zone's
 * left edge is skipped when another zone's right edge is on it, and so on). */
function handles(drafts: Draft[]): { zone: Draft; edge: Edge }[] {
  const out: { zone: Draft; edge: Edge }[] = [];
  for (const z of drafts) {
    const a = z.area;
    if (a.right < 1 - EPS) out.push({ zone: z, edge: 'right' });
    if (a.bottom < 1 - EPS) out.push({ zone: z, edge: 'bottom' });
    if (a.left > EPS && !drafts.some((o) => o.key !== z.key && Math.abs(o.area.right - a.left) < EPS)) {
      out.push({ zone: z, edge: 'left' });
    }
    if (a.top > EPS && !drafts.some((o) => o.key !== z.key && Math.abs(o.area.bottom - a.top) < EPS)) {
      out.push({ zone: z, edge: 'top' });
    }
  }
  return out;
}

function edgeLabel(zone: Draft, edge: Edge, drafts: Draft[]): string {
  const opposite: Record<Edge, Edge> = { left: 'right', right: 'left', top: 'bottom', bottom: 'top' };
  const other = drafts.find(
    (d) => d.key !== zone.key && d.monitor === zone.monitor && Math.abs(d.area[opposite[edge]] - zone.area[edge]) < EPS,
  );
  return other ? `Line between ${zone.name} and ${other.name}` : `${edge[0]?.toUpperCase() ?? ''}${edge.slice(1)} edge of ${zone.name}`;
}

function Canvas({
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

  const drag = (zone: Draft, edge: Edge) => (e: PointerEvent<HTMLDivElement>) => {
    const box = area.current?.getBoundingClientRect();
    if (!box || !(e.buttons & 1)) return;
    const horizontal = edge === 'left' || edge === 'right';
    const v = horizontal ? (e.clientX - box.left) / box.width : (e.clientY - box.top) / box.height;
    onChange(resize(drafts, zone.key, edge, snap(v)));
  };

  const keys = (zone: Draft, edge: Edge) => (e: KeyboardEvent<HTMLDivElement>) => {
    const horizontal = edge === 'left' || edge === 'right';
    const step = (e.shiftKey ? 4 : 1) * GRID;
    const now = zone.area[edge];
    const to =
      (horizontal && e.key === 'ArrowLeft') || (!horizontal && e.key === 'ArrowUp')
        ? now - step
        : (horizontal && e.key === 'ArrowRight') || (!horizontal && e.key === 'ArrowDown')
          ? now + step
          : e.key === 'Home'
            ? 0
            : e.key === 'End'
              ? 1
              : null;
    if (to === null) return;
    e.preventDefault();
    onChange(resize(drafts, zone.key, edge, snap(to)));
  };

  return (
    <div
      ref={area}
      className="relative w-full overflow-hidden rounded-xl border border-[var(--line)] bg-[var(--bg)] select-none"
      style={{ aspectRatio: `${String(w)} / ${String(h)}` }}
    >
      {here.map((z) => {
        const a = z.area;
        const on = z.key === selected;
        return (
          <button
            key={z.key}
            type="button"
            aria-pressed={on}
            aria-label={`${z.name}, ${percent(a.right - a.left)} wide, ${percent(a.bottom - a.top)} high`}
            onClick={() => {
              onSelect(z.key);
            }}
            className="absolute flex items-start justify-start overflow-hidden rounded-lg p-2 text-left"
            style={{
              left: `calc(${String(a.left * 100)}% + 3px)`,
              top: `calc(${String(a.top * 100)}% + 3px)`,
              width: `calc(${String((a.right - a.left) * 100)}% - 6px)`,
              height: `calc(${String((a.bottom - a.top) * 100)}% - 6px)`,
              border: `${on ? '3px' : '2px'} solid var(--zone-${z.color})`,
              background: `color-mix(in srgb, var(--zone-${z.color}) ${on ? '22%' : '12%'}, transparent)`,
            }}
          >
            <span
              className="truncate rounded-full px-2 py-0.5 text-[12px] font-semibold"
              style={{
                background: `var(--zone-${z.color})`,
                color: z.color === 'saffron' ? 'var(--zone-label-saffron)' : '#ffffff',
              }}
            >
              {z.name}
            </span>
          </button>
        );
      })}
      {handles(here).map(({ zone, edge }) => {
        const a = zone.area;
        const vertical = edge === 'left' || edge === 'right';
        const pos = a[edge];
        const style = vertical
          ? { left: `calc(${String(pos * 100)}% - 6px)`, top: `${String(a.top * 100)}%`, width: '12px', height: `${String((a.bottom - a.top) * 100)}%` }
          : { top: `calc(${String(pos * 100)}% - 6px)`, left: `${String(a.left * 100)}%`, height: '12px', width: `${String((a.right - a.left) * 100)}%` };
        return (
          <div
            key={`${zone.key}-${edge}`}
            role="separator"
            tabIndex={0}
            aria-orientation={vertical ? 'vertical' : 'horizontal'}
            aria-label={edgeLabel(zone, edge, here)}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(pos * 100)}
            onPointerDown={(e) => {
              e.currentTarget.setPointerCapture(e.pointerId);
            }}
            onPointerMove={drag(zone, edge)}
            onKeyDown={keys(zone, edge)}
            className={`group absolute z-10 flex items-center justify-center focus-visible:outline-none ${
              vertical ? 'cursor-col-resize' : 'cursor-row-resize'
            }`}
            style={style}
          >
            <span
              aria-hidden="true"
              className={`rounded-full bg-[var(--muted)] opacity-0 transition-opacity duration-150 group-hover:opacity-70 group-focus-visible:bg-[var(--focus)] group-focus-visible:opacity-100 ${
                vertical ? 'h-full max-h-16 w-1' : 'h-1 w-full max-w-16'
              }`}
            />
          </div>
        );
      })}
    </div>
  );
}

export function ZoneEditor({ workspace, onClose }: { workspace: string; onClose: () => void }) {
  const [name, setName] = useState<string | null>(null);
  const [monitors, setMonitors] = useState<MonitorState[]>([]);
  const [monitor, setMonitor] = useState<string | null>(null);
  const [saved, setSaved] = useState<Draft[]>([]);
  const [drafts, setDrafts] = useState<Draft[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [status, setStatus] = useState<{ text: string; error: boolean } | null>(null);
  const [leaving, setLeaving] = useState(false);
  const nameId = useId();

  useEffect(() => {
    void request({ cmd: 'zones', workspace }).then((reply) => {
      if (reply.reply !== 'zones') {
        setStatus({ text: errorMessage(reply) ?? 'The zones could not be read.', error: true });
        return;
      }
      const d = fromZones(reply.zones);
      setName(reply.name);
      setMonitors(reply.monitors);
      setMonitor((reply.monitors.find((m) => m.primary) ?? reply.monitors[0])?.id ?? null);
      setSaved(d);
      setDrafts(d);
    });
  }, [workspace]);

  const dirty = useMemo(() => JSON.stringify(toZones(drafts)) !== JSON.stringify(toZones(saved)), [drafts, saved]);
  const problem = draftProblem(drafts);
  const zone = drafts.find((d) => d.key === selected && d.monitor === monitor) ?? null;
  const current = monitors.find((m) => m.id === monitor) ?? null;

  const change = (next: Draft[]) => {
    setDrafts(next);
    setStatus(null);
  };

  const save = () => {
    void request({ cmd: 'set_zones', workspace, zones: toZones(drafts) }).then((reply) => {
      if (reply.reply === 'done') {
        setSaved(drafts);
        setStatus({ text: reply.message, error: false });
      } else {
        setStatus({ text: errorMessage(reply) ?? 'The zones could not be saved.', error: true });
      }
    });
  };

  if (name === null) {
    return (
      <article aria-busy={status === null}>
        <Status text={status?.text ?? 'Reading the zones…'} error={status?.error ?? false} />
        <Button onClick={onClose}>Back to workspaces</Button>
      </article>
    );
  }

  return (
    <article aria-labelledby="zones-title">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="text-[13px] font-semibold text-[var(--muted)]">Zones of</p>
          <h1 id="zones-title" className="font-display text-3xl font-bold tracking-tight">
            {name}
          </h1>
        </div>
        <div className="flex gap-2">
          <Button
            onClick={() => {
              if (dirty) setLeaving(true);
              else onClose();
            }}
          >
            {dirty ? 'Cancel' : 'Back'}
          </Button>
          <Button tone="primary" disabled={!dirty || problem !== null} onClick={save}>
            Save zones
          </Button>
        </div>
      </div>
      <p className="mt-2 text-[var(--muted)]">
        Drag the lines between zones to resize them, or focus a line and use the arrow keys (Shift for bigger steps). Pick
        a zone to split, rename or recolour it. Windows in a zone you remove stay where they are.
      </p>
      <div className="mt-3">
        <Status text={problem ?? status?.text ?? null} error={problem !== null || (status?.error ?? false)} />
      </div>

      {monitors.length > 1 ? (
        <div role="tablist" aria-label="Monitors" className="mt-4 flex flex-wrap gap-2">
          {monitors.map((m) => (
            <button
              key={m.id}
              role="tab"
              type="button"
              aria-selected={m.id === monitor}
              onClick={() => {
                setMonitor(m.id);
                setSelected(null);
              }}
              className={`rounded-lg border px-3 py-1.5 text-[13px] font-semibold ${
                m.id === monitor ? 'border-[var(--accent)]' : 'border-[var(--line)] text-[var(--muted)]'
              }`}
            >
              {m.name}
              {m.primary ? ' (main)' : ''} · {m.scale_percent}%
            </button>
          ))}
        </div>
      ) : null}

      {current ? (
        <div className="mt-4">
          <Canvas
            monitor={current}
            drafts={drafts}
            selected={selected}
            onSelect={setSelected}
            onChange={change}
          />
          <div className="mt-3 flex flex-wrap items-center gap-2" role="group" aria-label="Start from a preset">
            <span className="text-[13px] font-semibold text-[var(--muted)]">Presets:</span>
            {PRESETS.map((p) => (
              <Button
                key={p.id}
                tone="quiet"
                label={`Use the preset: ${p.label}`}
                onClick={() => {
                  change(applyPreset(drafts, current.id, p.id));
                  setSelected(null);
                }}
              >
                {p.label}
              </Button>
            ))}
          </div>
        </div>
      ) : null}

      {zone ? (
        <section aria-label={`Zone ${zone.name}`} className="mt-6 rounded-xl border border-[var(--line)] bg-[var(--surface)] p-4">
          <div className="flex flex-wrap items-end gap-4">
            <div className="flex flex-col gap-1">
              <label htmlFor={nameId} className="text-[13px] font-semibold">
                Name
              </label>
              <input
                id={nameId}
                value={zone.name}
                maxLength={MAX_NAME}
                onChange={(e) => {
                  change(update(drafts, zone.key, { name: e.target.value }));
                }}
                className="w-56 rounded-lg border border-[var(--line)] bg-[var(--bg)] px-3 py-1.5 select-text"
              />
            </div>
            <fieldset>
              <legend className="text-[13px] font-semibold">Colour</legend>
              <div className="mt-1 flex gap-2">
                {COLORS.map((c) => (
                  <label key={c} className="cursor-pointer">
                    <input
                      type="radio"
                      name={`color-${zone.key}`}
                      value={c}
                      checked={zone.color === c}
                      onChange={() => {
                        change(update(drafts, zone.key, { color: c }));
                      }}
                      className="peer sr-only"
                    />
                    <span
                      title={COLOR_NAMES[c]}
                      className="block h-7 w-7 rounded-full border-2 border-transparent peer-checked:border-[var(--text)] peer-focus-visible:outline peer-focus-visible:outline-2 peer-focus-visible:outline-[var(--focus)] forced-colors:border-[CanvasText]"
                      style={{ background: `var(--zone-${c})` }}
                    />
                    <span className="sr-only">{COLOR_NAMES[c]}</span>
                  </label>
                ))}
              </div>
            </fieldset>
          </div>
          <div className="mt-4 flex flex-wrap gap-2">
            <Button
              onClick={() => {
                change(split(drafts, zone.key, 'columns'));
              }}
            >
              Split side by side
            </Button>
            <Button
              onClick={() => {
                change(split(drafts, zone.key, 'rows'));
              }}
            >
              Split top and bottom
            </Button>
            <Button
              tone="danger"
              onClick={() => {
                change(remove(drafts, zone.key));
                setSelected(null);
              }}
            >
              Remove zone
            </Button>
          </div>
        </section>
      ) : (
        <p className="mt-6 text-[13px] text-[var(--muted)]">Pick a zone to split, rename, recolour or remove it.</p>
      )}

      <Confirm
        open={leaving}
        title="Leave without saving?"
        body="The changes to these zones will be lost."
        confirm="Discard changes"
        onCancel={() => {
          setLeaving(false);
        }}
        onConfirm={() => {
          setLeaving(false);
          onClose();
        }}
      />
    </article>
  );
}
