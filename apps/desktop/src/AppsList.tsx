// A workspace's apps on its page, zone by zone as its picture shows them: each zone's number in
// its colour, a line of its colour down the apps it takes, a zone left empty said so (the owner,
// of the row of zone dots each app had: "the options look like absolute garbage"). An app is
// dragged by its row into a zone, which it joins, or onto another app (its icon and name, as the
// picture's title bars), the two trading places (ADR 0033; the owner: "the apps in image 1 must
// be able to be dragged and droped from zones", then "permute when specifically dropped on
// another app otherwise just add", ADR 0034); with the keyboard, Move puts it in another zone,
// and on the picture above Enter picks it and Enter on another app trades them. × takes it out.
// Then Add an app. In the open workspace a change moves the window at once. Each change answers
// with the apps as they are now.

import { type PointerEvent as ReactPointerEvent, useEffect, useRef, useState } from 'react';

import { AddApp } from './AddApp';
import { request } from './engine';
import { AppIcon, Glyph } from './icons';
import { type AppZone, type Command, type WorkspaceApp, type WorkspaceApps, errorMessage } from './protocol';
import { MenuButton, Status, useAnnounce, zoneChip, zoneTint } from './ui';

/** A zone's number on its colour, as on the picture. */
function ZoneNumber({ zone, small = false }: { zone: AppZone; small?: boolean }) {
  return (
    <span
      aria-hidden="true"
      className={`zone-chip grid shrink-0 place-items-center rounded-full font-bold tabular-nums ${
        small ? 'size-5 text-[11px]' : 'size-7 text-caption'
      }`}
      style={zoneChip(zone.color)}
    >
      {zone.name}
    </span>
  );
}

/** Where an app dragged in the list is let go of: onto another app, or onto a zone. */
export type ListTarget = { app: WorkspaceApp } | { zone: AppZone };

/** What letting `app` go onto `target` asks Mosaïk, with the words that say it once done: onto
 * another app of a zone, the two trade places (zones, or in one zone their places front to back,
 * as on the picture); an app in no zone goes in that app's zone instead; onto a zone, the app
 * joins it. Null when nothing would change (onto itself, an app in no zone, or its own zone). */
export function listDrop(
  data: WorkspaceApps,
  app: WorkspaceApp,
  target: ListTarget,
): { command: Command; text: string } | null {
  const zone = (id: string | null) => data.zones.find((z) => z.id === id);
  const into = (z: AppZone) =>
    z.id === app.zone
      ? null
      : {
          command: { cmd: 'move_app', workspace: data.workspace, app: app.id, zone: z.id } satisfies Command,
          text: `${app.label} goes in zone ${z.name}.`,
        };
  if ('zone' in target) return into(target.zone);
  const other = target.app;
  const there = zone(other.zone);
  if (other.id === app.id || !there) return null;
  if (!zone(app.zone)) return into(there);
  return {
    command: { cmd: 'move_app', workspace: data.workspace, app: app.id, zone: there.id, swap_with: other.id },
    text:
      other.zone === app.zone
        ? `${app.label} and ${other.label} traded places in zone ${there.name}.`
        : `${app.label} and ${other.label} traded zones.`,
  };
}

/** An app being dragged: where the pointer holds its row, the row's width, the pointer, and
 * what it is over. */
interface Drag {
  app: WorkspaceApp;
  dx: number;
  dy: number;
  width: number;
  x: number;
  y: number;
  over: ListTarget | null;
}

export function AppsList({
  name,
  data,
  problem,
  onApps,
  onStale,
}: {
  /** The workspace's name, as its page says it. */
  name: string;
  data: WorkspaceApps | null;
  problem: string | null;
  /** A change made, with the apps as they are now. */
  onApps: (apps: WorkspaceApps) => void;
  /** A change refused: the apps may have changed meanwhile. */
  onStale: () => void;
}) {
  const [busy, setBusy] = useState<string | null>(null);
  const [outcome, setOutcome] = useState<{ text: string; error: boolean } | null>(null);
  const announce = useAnnounce();
  const [drag, setDragState] = useState<Drag | null>(null);
  const dragging = useRef<Drag | null>(null);
  /** Where the pointer went down on a row, until it moves far enough to drag it. */
  const press = useRef<{ app: WorkspaceApp; pointer: number; x: number; y: number } | null>(null);
  /** Each app's icon and name: a drop there is onto the app. */
  const chips = useRef(new Map<string, HTMLElement>());
  const groups = useRef(new Map<string, HTMLElement>());

  const setDrag = (d: Drag | null) => {
    dragging.current = d;
    setDragState(d);
  };

  // Esc lets a drag go, and so does a release anywhere (the row may have gone meanwhile).
  const dragOn = drag !== null;
  useEffect(() => {
    if (!dragOn) return;
    const key = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      e.preventDefault();
      e.stopPropagation();
      press.current = null;
      dragging.current = null;
      setDragState(null);
    };
    const end = () => {
      press.current = null;
      dragging.current = null;
      setDragState(null);
    };
    document.addEventListener('keydown', key, true);
    window.addEventListener('pointercancel', end);
    return () => {
      document.removeEventListener('keydown', key, true);
      window.removeEventListener('pointercancel', end);
    };
  }, [dragOn]);

  const change = (key: string, command: Command, done: string) => {
    if (busy !== null) return;
    setBusy(key);
    setOutcome(null);
    void request(command).then((reply) => {
      setBusy(null);
      if (reply.reply === 'apps') {
        onApps(reply);
        setOutcome({ text: done, error: false });
        announce(done);
      } else {
        setOutcome({ text: errorMessage(reply) ?? 'The workspace could not be changed.', error: true });
        onStale();
      }
    });
  };

  if (problem) return <Status text={problem} error />;
  if (data === null) {
    return (
      <p className="py-3 text-small text-[var(--muted)]" aria-busy="true">
        Reading its apps…
      </p>
    );
  }
  // The zones in their order on screen: 1, 2, 3…
  const zones = [...data.zones].sort((a, b) => Number(a.name) - Number(b.name) || a.name.localeCompare(b.name));
  const installed = data.installed ?? [];
  const known = new Set(zones.map((z) => z.id));
  const loose = data.apps.filter((a) => a.zone === null || !known.has(a.zone));

  /** What is under the pointer: the icon and name of another app of a zone, else a zone (the rest
   * of its rows too, as the picture's zones beside their title bars). */
  const targetAt = (x: number, y: number, self: string): ListTarget | null => {
    const hit = (el: HTMLElement) => {
      const r = el.getBoundingClientRect();
      return x >= r.left && x < r.right && y >= r.top && y < r.bottom;
    };
    for (const [id, el] of chips.current) {
      const a = data.apps.find((x) => x.id === id);
      if (id !== self && a && a.zone !== null && known.has(a.zone) && hit(el)) return { app: a };
    }
    for (const [id, el] of groups.current) {
      const z = zones.find((x) => x.id === id);
      if (z && hit(el)) return { zone: z };
    }
    return null;
  };

  /** A press that moves 5 pixels drags the row (a ghost of it follows the pointer); one on its
   * buttons is theirs. A mouse drags it from anywhere, a finger or pen from its grip. */
  const handle = (a: WorkspaceApp) => ({
    onPointerDown: (e: ReactPointerEvent<HTMLElement>) => {
      const t = e.target as HTMLElement;
      if (e.button !== 0 || busy !== null || t.closest('button')) return;
      if (e.pointerType !== 'mouse' && !t.closest('[data-grip]')) return;
      press.current = { app: a, pointer: e.pointerId, x: e.clientX, y: e.clientY };
      if (typeof e.currentTarget.setPointerCapture === 'function') e.currentTarget.setPointerCapture(e.pointerId);
    },
    onPointerMove: (e: ReactPointerEvent<HTMLElement>) => {
      const p = press.current;
      if (p?.pointer !== e.pointerId || p.app.id !== a.id) return;
      let d = dragging.current;
      if (!d) {
        if (Math.hypot(e.clientX - p.x, e.clientY - p.y) < 5) return;
        const r = e.currentTarget.getBoundingClientRect();
        d = { app: a, dx: p.x - r.left, dy: p.y - r.top, width: r.width, x: 0, y: 0, over: null };
      }
      setDrag({ ...d, x: e.clientX, y: e.clientY, over: targetAt(e.clientX, e.clientY, a.id) });
    },
    onPointerUp: (e: ReactPointerEvent<HTMLElement>) => {
      if (press.current?.pointer !== e.pointerId) return;
      press.current = null;
      const d = dragging.current;
      setDrag(null);
      if (!d?.over) return;
      const drop = listDrop(data, d.app, d.over);
      if (drop) change(d.app.id, drop.command, drop.text);
    },
    onLostPointerCapture: () => {
      if (!press.current && dragging.current?.app.id === a.id) setDrag(null);
    },
  });

  const overApp = (a: WorkspaceApp) =>
    drag?.over !== undefined && drag.over !== null && 'app' in drag.over && drag.over.app.id === a.id;
  const overZone = (z: AppZone) =>
    drag?.over !== undefined && drag.over !== null && 'zone' in drag.over && drag.over.zone.id === z.id;

  const row = (a: WorkspaceApp) => {
    const elsewhere = zones.filter((z) => z.id !== a.zone);
    return (
      <li
        key={a.id}
        className={`group/row -mx-1.5 flex min-h-11 cursor-grab items-center gap-2.5 rounded-lg px-1.5 select-none ${
          drag?.app.id === a.id ? 'opacity-40' : ''
        }`}
        {...handle(a)}
      >
        {/* Its icon and name: dropped there, an app trades places with it. */}
        <span
          ref={(el) => {
            if (!el) return;
            chips.current.set(a.id, el);
            return () => {
              if (chips.current.get(a.id) === el) chips.current.delete(a.id);
            };
          }}
          className={`-my-1 flex min-w-0 items-center gap-2.5 rounded-md py-1 pr-1.5 ${
            overApp(a) ? 'outline-2 outline-offset-1 outline-[var(--text)]' : ''
          }`}
        >
          <span
            data-grip
            aria-hidden="true"
            title="Drag it into another zone, or onto another app to swap them"
            className="-mr-1 grid w-4 shrink-0 touch-none place-items-center text-[var(--muted)] opacity-60 group-hover/row:opacity-100"
          >
            <Glyph name="grip" size={14} />
          </span>
          <AppIcon icon={a.icon} size={22} keepSpace />
          <span className="min-w-0 truncate font-semibold">{a.label}</span>
          {a.tab ? (
            <span className="shrink-0 rounded-md bg-[var(--hover)] px-1.5 py-0.5 text-caption font-semibold text-[var(--muted)]">
              Tab
            </span>
          ) : null}
        </span>
        <span aria-hidden="true" className="flex-1" />
        {elsewhere.length ? (
          <MenuButton
            label={`Move ${a.label} to another zone`}
            items={elsewhere.map((z) => ({
              key: z.id,
              label: `Zone ${z.name}`,
              icon: <ZoneNumber zone={z} small />,
              onSelect: () => {
                change(
                  a.id,
                  { cmd: 'move_app', workspace: data.workspace, app: a.id, zone: z.id },
                  `${a.label} goes in zone ${z.name}.`,
                );
              },
            }))}
          >
            Move
            <Glyph name="down" size={14} />
          </MenuButton>
        ) : null}
        <button
          type="button"
          aria-label={`Remove ${a.label}`}
          title={`Remove ${a.label}`}
          aria-busy={busy === a.id || undefined}
          disabled={busy !== null}
          onClick={() => {
            change(a.id, { cmd: 'remove_app', workspace: data.workspace, app: a.id }, `${a.label} removed.`);
          }}
          className="grid size-10 shrink-0 place-items-center rounded-full text-[var(--muted)] hover:bg-[var(--hover)] hover:text-[var(--text)] focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[var(--focus)] disabled:cursor-progress disabled:opacity-50"
        >
          <Glyph name="close" size={16} />
        </button>
      </li>
    );
  };

  /** One zone and its apps: its number, then a line of its colour down to its last app. Lit while
   * an app dragged is over it, a little past the line between zones, which stays as wide as the
   * list (the lines went 8 px past it on each side, the last one did not). */
  const group = (zone: AppZone | null, apps: WorkspaceApp[]) => {
    const lit = zone !== null && overZone(zone);
    return (
      <div key={zone?.id ?? 'none'}>
        <div
          ref={(el) => {
            if (!el || !zone) return;
            groups.current.set(zone.id, el);
            return () => {
              if (groups.current.get(zone.id) === el) groups.current.delete(zone.id);
            };
          }}
          role="group"
          aria-label={zone ? `Zone ${zone.name}` : 'Not in a zone'}
          className={`-mx-2 flex gap-3 rounded-xl px-2 py-2 transition-colors duration-150 ${
            lit ? 'outline-2 outline-offset-[-2px] outline-[var(--text)]' : ''
          }`}
          style={lit ? { background: `color-mix(in srgb, ${zoneTint(zone.color)} 14%, transparent)` } : undefined}
        >
          <div aria-hidden="true" className="flex w-7 shrink-0 flex-col items-center pt-2">
            {zone ? (
              <ZoneNumber zone={zone} />
            ) : (
              <span className="size-7 rounded-full border-2 border-dashed border-[var(--control)]" />
            )}
            {apps.length > 1 ? (
              <span
                className="mt-1.5 mb-3 w-0.5 flex-1 rounded-full forced-colors:bg-[CanvasText]"
                style={{
                  background: zone ? `color-mix(in srgb, ${zoneTint(zone.color)} 55%, transparent)` : 'var(--line)',
                }}
              />
            ) : null}
          </div>
          {apps.length ? (
            <ul className="min-w-0 flex-1">{apps.map(row)}</ul>
          ) : (
            <p className="flex min-h-11 flex-1 items-center text-small text-[var(--muted)]">
              {/* How to fill it, only while there is an app to drag or move. */}
              {data.apps.length ? 'Empty. Drag an app here, or use Move.' : 'Empty'}
            </p>
          )}
        </div>
      </div>
    );
  };

  return (
    <div>
      {data.apps.length === 0 && zones.length === 0 ? (
        <p className="py-3.5 text-small text-[var(--muted)]">No apps yet. Add one below.</p>
      ) : (
        <div aria-label={`Apps of “${name}”`} role="group" className="divide-y divide-[var(--line)]">
          {zones.map((z) =>
            group(
              z,
              data.apps.filter((a) => a.zone === z.id),
            ),
          )}
          {loose.length ? group(null, loose) : null}
        </div>
      )}
      {drag ? (
        <div
          aria-hidden="true"
          className="pointer-events-none fixed z-40 flex min-h-11 items-center gap-2.5 rounded-lg bg-[var(--raised)] px-3 shadow-[var(--shadow)] outline-2 outline-[var(--text)]"
          style={{ left: drag.x - drag.dx, top: drag.y - drag.dy, width: drag.width }}
        >
          <AppIcon icon={drag.app.icon} size={22} keepSpace />
          <span className="min-w-0 flex-1 truncate font-semibold">{drag.app.label}</span>
        </div>
      ) : null}
      <div className="flex flex-wrap items-center justify-between gap-2 border-t border-[var(--line)] py-3">
        {data.windows.length || installed.length ? (
          <AddApp
            name={name}
            windows={data.windows}
            installed={installed}
            onWindow={(w) => {
              change(
                'add',
                { cmd: 'add_app', workspace: data.workspace, window: w.window, zone: null },
                `${w.label} added.`,
              );
            }}
            onInstalled={(a) => {
              change(
                'add',
                { cmd: 'add_installed_app', workspace: data.workspace, app: a.app, zone: null },
                `${a.label} added.`,
              );
            }}
          />
        ) : (
          <p className="text-small text-[var(--muted)]">Open a window to add it here.</p>
        )}
        <Status text={outcome?.text ?? null} error={outcome?.error ?? false} />
      </div>
    </div>
  );
}
