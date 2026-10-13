// The zone editor: one workspace's zones on each monitor connected now. Drag the lines between
// zones (or focus one and use the arrow keys) to resize; pick a zone to rename, recolour, split
// or remove it; start from a preset. Nothing changes until "Save zones".

import { useEffect, useMemo, useState } from 'react';

import { Canvas } from './editor/Canvas';
import {
  type Draft,
  PRESETS,
  type Preset,
  applyPreset,
  fromZones,
  presetShape,
  problem as draftProblem,
  remove,
  split,
  toZones,
  update,
} from './editor/model';
import { ZonePanel } from './editor/ZonePanel';
import { request } from './engine';
import { errorMessage, type MonitorState } from './protocol';
import { Button, Confirm, MenuButton, PageTitle, Status, useSay } from './ui';

interface Loaded {
  name: string;
  monitors: MonitorState[];
  revision: string;
}

/** The key, in `next`, of the zone that is `key` in `before`: the same id, or for a zone saved
 * for the first time, the same place. */
function sameZone(before: Draft[], next: Draft[], key: string | null): string | null {
  const z = before.find((d) => d.key === key);
  if (!z) return null;
  const same = next.find(
    (d) =>
      (z.id !== null && d.id === z.id) ||
      (d.monitor === z.monitor && JSON.stringify(d.area) === JSON.stringify(z.area)),
  );
  return same?.key ?? null;
}

/** A small drawing of a preset's zones, beside its name in the menu. */
function PresetPicture({ preset }: { preset: Preset }) {
  return (
    <svg aria-hidden="true" width="30" height="20" viewBox="0 0 30 20" className="shrink-0">
      {presetShape(preset).map((a, i) => (
        <rect
          key={i}
          x={1 + a.left * 28 + 0.75}
          y={1 + a.top * 18 + 0.75}
          width={Math.max(1, (a.right - a.left) * 28 - 1.5)}
          height={Math.max(1, (a.bottom - a.top) * 18 - 1.5)}
          rx="1.5"
          className="fill-[var(--hover)] stroke-[var(--muted)] forced-colors:fill-[Canvas] forced-colors:stroke-[CanvasText]"
          strokeWidth="1"
        />
      ))}
    </svg>
  );
}

export function ZoneEditor({
  workspace,
  onClose,
  onDirtyChange,
}: {
  workspace: string;
  onClose: () => void;
  /** Told whether there are unsaved changes, so leaving can ask first. */
  onDirtyChange: (dirty: boolean) => void;
}) {
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [loadProblem, setLoadProblem] = useState<string | null>(null);
  const [monitor, setMonitor] = useState<string | null>(null);
  const [saved, setSaved] = useState<Draft[]>([]);
  const [drafts, setDrafts] = useState<Draft[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const say = useSay();

  useEffect(() => {
    let live = true;
    void request({ cmd: 'zones', workspace }).then((reply) => {
      if (!live) return;
      if (reply.reply !== 'zones') {
        setLoadProblem(errorMessage(reply) ?? 'The zones could not be read.');
        return;
      }
      const d = fromZones(reply.zones);
      setLoaded({ name: reply.name, monitors: reply.monitors, revision: reply.revision });
      setMonitor((reply.monitors.find((m) => m.primary) ?? reply.monitors[0])?.id ?? null);
      setSaved(d);
      setDrafts(d);
    });
    return () => {
      live = false;
    };
  }, [workspace]);

  const dirty = useMemo(() => JSON.stringify(toZones(drafts)) !== JSON.stringify(toZones(saved)), [drafts, saved]);
  useEffect(() => {
    onDirtyChange(dirty);
  }, [dirty, onDirtyChange]);

  const problem = draftProblem(drafts);
  const zone = drafts.find((d) => d.key === selected && d.monitor === monitor) ?? null;
  const current = loaded?.monitors.find((m) => m.id === monitor) ?? null;

  const save = () => {
    if (!loaded) return;
    setSaving(true);
    void request({ cmd: 'set_zones', workspace, zones: toZones(drafts), revision: loaded.revision }).then((reply) => {
      setSaving(false);
      if (reply.reply !== 'zones') {
        say(errorMessage(reply) ?? 'The zones could not be saved.', true);
        return;
      }
      // The zones as saved: new ones have their ids now, so saving again keeps them.
      const next = fromZones(reply.zones);
      setSelected((key) => sameZone(drafts, next, key));
      setLoaded({ name: reply.name, monitors: reply.monitors, revision: reply.revision });
      setSaved(next);
      setDrafts(next);
      say('Zones saved.');
    });
  };

  if (!loaded) {
    return (
      <article aria-labelledby="zones-title" aria-busy={loadProblem === null}>
        <PageTitle id="zones-title" eyebrow="Zones">
          {loadProblem ? 'The zones could not be read' : 'Reading the zones…'}
        </PageTitle>
        <div className="mt-3">
          <Status text={loadProblem} error />
        </div>
        <div className="mt-3">
          <Button onClick={onClose}>Back</Button>
        </div>
      </article>
    );
  }

  return (
    <article aria-labelledby="zones-title">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <PageTitle id="zones-title" eyebrow="Zones of">
          {loaded.name}
        </PageTitle>
        <div className="flex gap-2">
          <Button
            onClick={() => {
              if (dirty) setLeaving(true);
              else onClose();
            }}
          >
            {dirty ? 'Cancel' : 'Back'}
          </Button>
          <Button tone="primary" pending={saving} disabled={!dirty || problem !== null} onClick={save}>
            Save zones
          </Button>
        </div>
      </div>
      <p className="mt-2 text-small text-[var(--muted)]">
        Drag a line between zones to resize them, or focus it and use the arrow keys (with Shift for bigger steps).
        Windows in a zone you remove stay where they are.
      </p>

      <div className="mt-5 flex flex-wrap items-center justify-between gap-3">
        {loaded.monitors.length > 1 ? (
          <div role="group" aria-label="Monitor" className="flex flex-wrap gap-1.5">
            {loaded.monitors.map((m) => (
              <button
                key={m.id}
                type="button"
                aria-pressed={m.id === monitor}
                onClick={() => {
                  setMonitor(m.id);
                  setSelected(null);
                }}
                className={`rounded-lg border px-3 py-1.5 text-small font-semibold ${
                  m.id === monitor
                    ? 'border-[var(--accent)] bg-[var(--surface)] text-[var(--text)] forced-colors:border-[Highlight] forced-colors:underline'
                    : 'border-[var(--line)] text-[var(--muted)] hover:bg-[var(--hover)]'
                }`}
              >
                {m.name}
                {m.primary ? ' (main)' : ''} · {m.scale_percent}%
              </button>
            ))}
          </div>
        ) : (
          <span />
        )}
        {current ? (
          <MenuButton
            label="Start from a preset"
            items={PRESETS.map((p) => ({
              label: p.label,
              icon: <PresetPicture preset={p.id} />,
              onSelect: () => {
                setDrafts(applyPreset(drafts, current.id, p.id));
                setSelected(null);
              },
            }))}
          >
            <span className="text-[var(--accent)]">Start from a preset ▾</span>
          </MenuButton>
        ) : null}
      </div>

      {current ? (
        <div className="mt-3">
          <Canvas monitor={current} drafts={drafts} selected={selected} onSelect={setSelected} onChange={setDrafts} />
        </div>
      ) : null}

      <div className="mt-4">
        {zone ? (
          <ZonePanel
            zone={zone}
            onRename={(name) => {
              setDrafts(update(drafts, zone.key, { name }));
            }}
            onColor={(color) => {
              setDrafts(update(drafts, zone.key, { color }));
            }}
            onSplit={(direction) => {
              setDrafts(split(drafts, zone.key, direction));
            }}
            onRemove={() => {
              setDrafts(remove(drafts, zone.key));
              setSelected(null);
              // Focus goes to a zone left on this monitor rather than being lost with the panel.
              requestAnimationFrame(() => {
                document.querySelector<HTMLElement>('[data-canvas] button[aria-pressed]')?.focus();
              });
            }}
          />
        ) : (
          <p className="text-small text-[var(--muted)]">Pick a zone to rename, recolour, split or remove it.</p>
        )}
        <div className="mt-2">
          <Status text={problem} error />
        </div>
      </div>

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
