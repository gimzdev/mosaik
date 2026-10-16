// One workspace, to change: its name, its shortcut, its apps (each in the zone it goes in), its
// zones, or to delete it. Opened from its card's ⋯ menu on Home. At the top, the picture of the
// main screen as opening it would leave it, as on Home (the owner: "show visually the zones same
// as homepage for that workspace specifically with arrangements aswell"), where its apps are
// dragged to other zones and its zones resized (ADR 0031), saved at once.

import { type SyntheticEvent, useId, useState } from 'react';

import { AddApp } from './AddApp';
import { request } from './engine';
import { AppIcon, Glyph } from './icons';
import {
  type AppZone,
  type ArrangePreview,
  type Command,
  MAX_NAME,
  type WorkspaceApp,
  type WorkspaceApps,
  type WorkspaceSummary,
  errorMessage,
} from './protocol';
import { useReads } from './reads';
import { type PictureEdits, Screen, dropped, stretched } from './Screen';
import { ShortcutRecorder } from './shortcut';
import {
  Button,
  Confirm,
  MenuButton,
  OnScreen,
  PageTitle,
  Section,
  Status,
  useAnnounce,
  zoneChip,
  zoneTint,
} from './ui';
import { type Change, moveApp, resizeZones } from './workspacePicture';

function Rename({
  workspace,
  onDone,
  onCancel,
}: {
  workspace: WorkspaceSummary;
  onDone: (message: string) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(workspace.name);
  const [problem, setProblem] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const id = useId();
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    setPending(true);
    void request({ cmd: 'rename_workspace', workspace: workspace.id, name }).then((reply) => {
      setPending(false);
      if (reply.reply === 'done') onDone(reply.message);
      else setProblem(errorMessage(reply) ?? 'It could not be renamed.');
    });
  };
  return (
    <form
      onSubmit={submit}
      onKeyDown={(e) => {
        if (e.key === 'Escape') {
          e.preventDefault();
          onCancel();
        }
      }}
      className="flex flex-col gap-2"
    >
      <label htmlFor={id} className="sr-only">
        Name
      </label>
      <div className="flex flex-wrap gap-2">
        <input
          id={id}
          value={name}
          maxLength={MAX_NAME}
          // Focus goes where the user is about to type.
          autoFocus
          onFocus={(e) => {
            e.currentTarget.select();
          }}
          onChange={(e) => {
            setName(e.target.value);
            setProblem(null);
          }}
          aria-invalid={problem !== null}
          className="field min-w-56 flex-1 font-display text-heading font-semibold"
        />
        <Button tone="primary" type="submit" pending={pending} disabled={!name.trim()}>
          Save name
        </Button>
        <Button onClick={onCancel}>Cancel</Button>
      </div>
      <Status text={problem} error />
    </form>
  );
}

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

/** The workspace's apps, zone by zone as its picture shows them: each zone's number in its colour,
 * a line of its colour down the apps it takes, a zone left empty said so (the owner, of the row of
 * zone dots each app had: "the options look like absolute garbage"). Move puts an app in another
 * zone (as dragging it on the picture does), and × takes it out. Then the windows on screen to
 * add. In the open workspace a change moves the window at once. Each change answers with the
 * apps as they are now. */
function Apps({
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

  const row = (a: WorkspaceApp) => {
    const elsewhere = zones.filter((z) => z.id !== a.zone);
    return (
      <li key={a.id} className="flex min-h-11 items-center gap-2.5">
        <AppIcon icon={a.icon} size={22} keepSpace />
        <span className="min-w-0 flex-1 truncate font-semibold">{a.label}</span>
        {a.tab ? (
          <span className="shrink-0 rounded-md bg-[var(--hover)] px-1.5 py-0.5 text-caption font-semibold text-[var(--muted)]">
            Tab
          </span>
        ) : null}
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

  /** One zone and its apps: its number, then a line of its colour down to its last app. */
  const group = (zone: AppZone | null, apps: WorkspaceApp[]) => (
    <div
      key={zone?.id ?? 'none'}
      role="group"
      aria-label={zone ? `Zone ${zone.name}` : 'Not in a zone'}
      className="flex gap-3 py-2"
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
          Empty. Drag an app here on the picture, or use Move.
        </p>
      )}
    </div>
  );

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

export function WorkspacePage({
  workspace,
  onBack,
  onEditZones,
}: {
  workspace: string;
  onBack: () => void;
  onEditZones: (workspace: string) => void;
}) {
  const [summary, setSummary] = useState<WorkspaceSummary | null>(null);
  const [active, setActive] = useState(false);
  const [gone, setGone] = useState(false);
  const [apps, setApps] = useState<WorkspaceApps | null>(null);
  const [appsProblem, setAppsProblem] = useState<string | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [opening, setOpening] = useState(false);
  const [outcome, setOutcome] = useState<{ text: string; error: boolean } | null>(null);
  /** The picture of it opened, and whether Mosaïk could not say. */
  const [preview, setPreview] = useState<ArrangePreview | null>(null);
  const [unread, setUnread] = useState(false);
  /** The picture right after a change made on it, until Mosaïk's answer comes. */
  const [early, setEarly] = useState<ArrangePreview | null>(null);
  const announce = useAnnounce();

  // What it is (its name, shortcut, whether it is on screen) changes only by Mosaïk's say-so;
  // its apps also as windows open and close, which the window coming back into focus catches;
  // its picture with both. A change of its apps answers with them: they are not read again.
  const poke = useReads(async (why, current) => {
    if (why.has('start') || why.has('changed') || why.has('own')) {
      const reply = await request({ cmd: 'workspaces' });
      if (!current()) return;
      if (reply.reply === 'workspaces') {
        const w = reply.workspaces.find((x) => x.id === workspace) ?? null;
        setSummary(w);
        setGone(w === null);
        setActive(reply.active === workspace);
        if (!w) return;
      }
    }
    if (why.has('start') || why.has('focus') || (why.has('changed') && !why.has('own'))) {
      const reply = await request({ cmd: 'apps', workspace });
      if (!current()) return;
      if (reply.reply === 'apps') {
        setApps(reply);
        setAppsProblem(null);
      } else {
        setAppsProblem(errorMessage(reply) ?? 'Its apps could not be read.');
      }
    }
    const reply = await request({ cmd: 'preview_workspace', workspace });
    if (!current()) return;
    setPreview(reply.reply === 'preview' ? reply : null);
    setUnread(reply.reply !== 'preview');
    setEarly(null);
  }, workspace);

  const say = (text: string, error = false) => {
    setOutcome({ text, error });
    if (!error) announce(text);
  };

  /** A change made on the picture, saved at once: said once Mosaïk answers. A move answers with
   * the apps; a resize may number the zones anew, so everything is read again. */
  const edited = (change: Promise<Change>) => {
    setOutcome(null);
    void change.then(({ reply, text, done }) => {
      say(text, !done);
      if (reply.reply === 'apps') {
        setApps(reply);
        poke('own');
      } else {
        if (!done) setEarly(null);
        poke('changed');
      }
    });
  };
  const picture = early ?? preview;
  const pictureEdits: PictureEdits | undefined = picture
    ? {
        kind: 'workspace',
        onDrop: (d) => {
          setEarly(dropped(picture, d));
          edited(moveApp(workspace, null, picture, d));
        },
        onStretch: (areas) => {
          setEarly(stretched(picture, areas));
          edited(resizeZones(workspace, null, picture, areas));
        },
      }
    : undefined;

  if (gone) {
    return (
      <article aria-labelledby="workspace-title">
        <PageTitle id="workspace-title">This workspace is gone</PageTitle>
        <p className="mt-2 text-[var(--muted)]">It was deleted.</p>
        <div className="mt-4">
          <Button onClick={onBack}>Back to Home</Button>
        </div>
      </article>
    );
  }
  if (!summary) {
    return (
      <p className="text-[var(--muted)]" aria-busy="true">
        Reading the workspace…
      </p>
    );
  }

  return (
    <article aria-labelledby="workspace-title" className="mx-auto w-full max-w-[46rem]">
      <button
        type="button"
        onClick={onBack}
        aria-label="Back to Home"
        className="-ml-2 inline-flex min-h-10 items-center gap-1 rounded-lg px-2 text-small font-semibold text-[var(--muted)] hover:bg-[var(--hover)] hover:text-[var(--text)]"
      >
        <Glyph name="back" size={14} />
        Home
      </button>

      <div className="mt-2 flex flex-wrap items-center gap-4">
        <div className="min-w-0 flex-1">
          {renaming ? (
            <Rename
              workspace={summary}
              onCancel={() => {
                setRenaming(false);
              }}
              onDone={(m) => {
                setRenaming(false);
                say(m);
              }}
            />
          ) : (
            <div className="flex flex-wrap items-center gap-2">
              <PageTitle id="workspace-title" className="min-w-0 break-words">
                {summary.name}
              </PageTitle>
              <Button
                tone="quiet"
                size="sm"
                label={`Rename “${summary.name}”`}
                onClick={() => {
                  setRenaming(true);
                }}
              >
                <Glyph name="edit" size={14} />
                Rename
              </Button>
            </div>
          )}
          <p className="mt-0.5 flex flex-wrap items-center gap-x-1.5 text-small text-[var(--muted)]">
            {active ? (
              <>
                <OnScreen />
                <span aria-hidden="true">·</span>
              </>
            ) : null}
            <span>
              {summary.apps.length} {summary.apps.length === 1 ? 'app' : 'apps'} · {summary.zones}{' '}
              {summary.zones === 1 ? 'zone' : 'zones'}
            </span>
          </p>
        </div>
        <Button
          tone="primary"
          pending={opening}
          onClick={() => {
            setOpening(true);
            void request({ cmd: 'switch', workspace: summary.id }).then((reply) => {
              setOpening(false);
              if (reply.reply === 'done') {
                say(reply.message);
              } else {
                say(errorMessage(reply) ?? 'It could not be opened.', true);
                poke('changed');
              }
            });
          }}
        >
          <Glyph name="play" size={12} />
          Open
        </Button>
      </div>
      <div className="mt-2">
        <Status text={outcome?.text ?? null} error={outcome?.error ?? false} />
      </div>

      <div className="mt-4">
        <Screen preview={picture} unread={unread} tallest={38} edit={pictureEdits} />
      </div>

      <Section title="Apps">
        <Apps
          name={summary.name}
          data={apps}
          problem={appsProblem}
          onApps={(next) => {
            setApps(next);
            poke('own');
          }}
          onStale={() => {
            poke('changed');
          }}
        />
      </Section>

      <Section title="Shortcut">
        <ShortcutRecorder
          label={`Open “${summary.name}”`}
          description="Opens it from any app."
          value={summary.hotkey}
          workspace={summary.id}
          onSave={async (hotkey) => {
            const reply = await request({ cmd: 'set_workspace_hotkey', workspace: summary.id, hotkey });
            if (reply.reply !== 'done') return errorMessage(reply) ?? 'The shortcut could not be saved.';
            announce(reply.message);
            return null;
          }}
          onRemove={async () => {
            const reply = await request({ cmd: 'set_workspace_hotkey', workspace: summary.id, hotkey: null });
            if (reply.reply !== 'done') return errorMessage(reply) ?? 'The shortcut could not be removed.';
            announce(reply.message);
            return null;
          }}
        />
      </Section>

      <Section title="Zones">
        <div className="flex flex-wrap items-center justify-between gap-3 py-3.5">
          <p className="text-small text-[var(--muted)]">
            Where its windows go, numbered by place. Resize, split or recolour them.
          </p>
          <Button
            onClick={() => {
              onEditZones(summary.id);
            }}
          >
            Edit zones
          </Button>
        </div>
      </Section>

      <div className="mt-7 flex justify-end">
        <Button
          tone="danger"
          size="sm"
          onClick={() => {
            setDeleting(true);
          }}
        >
          <Glyph name="trash" size={14} />
          Delete workspace
        </Button>
      </div>

      <Confirm
        open={deleting}
        title={`Delete “${summary.name}”?`}
        body="Its windows stay where they are; only the workspace goes. This cannot be undone."
        confirm="Delete workspace"
        onCancel={() => {
          setDeleting(false);
        }}
        onConfirm={() => {
          setDeleting(false);
          void request({ cmd: 'delete_workspace', workspace: summary.id }).then((reply) => {
            if (reply.reply === 'done') {
              announce(reply.message);
              onBack();
            } else {
              say(errorMessage(reply) ?? 'It could not be deleted.', true);
            }
          });
        }}
      />
    </article>
  );
}
