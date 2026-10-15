// One workspace, to change: its name, its shortcut, its apps (each in the zone it goes in), its
// zones, or to delete it. Opened from its card's ⋯ menu on Home. At the top, the picture of the
// main screen as opening it would leave it, as on Home (the owner: "show visually the zones same
// as homepage for that workspace specifically with arrangements aswell").

import { type CSSProperties, type SyntheticEvent, useCallback, useEffect, useId, useState } from 'react';

import { onWorkspacesChanged, request } from './engine';
import { AppIcon, Glyph } from './icons';
import {
  type AppZone,
  type ArrangePreview,
  type Command,
  MAX_NAME,
  type WorkspaceApps,
  type WorkspaceSummary,
  errorMessage,
} from './protocol';
import { Screen } from './Screen';
import { ShortcutRecorder } from './shortcut';
import { Button, Confirm, MenuButton, PageTitle, Section, Status, useAnnounce } from './ui';

/** Whether a window's title says no more than its app's name ("Battle.net" and "Battle․net"). */
const same = (title: string, label: string) =>
  !title.trim() || title.normalize('NFKC').trim().toLowerCase() === label.normalize('NFKC').trim().toLowerCase();

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

/** A zone's dot: its number in its colour. The one an app is in is solid and ringed; the others
 * are rings of their colour, and a click puts the app there. */
function ZoneDot({
  zone,
  on,
  label,
  disabled,
  onPick,
}: {
  zone: AppZone;
  on: boolean;
  label: string;
  disabled: boolean;
  onPick: () => void;
}) {
  const tint = `var(--zone-${zone.color})`;
  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={on}
      disabled={disabled}
      onClick={() => {
        if (!on) onPick();
      }}
      className={`grid size-6 shrink-0 place-items-center rounded-full border-2 text-caption font-bold tabular-nums transition-[background-color,color] duration-150 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus)] disabled:cursor-progress forced-colors:border-[CanvasText] ${
        on
          ? 'zone-chip ring-2 ring-[var(--text)] ring-offset-2 ring-offset-[var(--bg)] forced-colors:ring-[Highlight]'
          : 'cursor-pointer text-[var(--muted)] hover:text-[var(--text)]'
      }`}
      style={
        on
          ? ({
              '--chip': tint,
              '--on-chip': zone.color === 'saffron' ? 'var(--on-zone-saffron)' : 'var(--on-zone)',
              borderColor: tint,
            } as CSSProperties)
          : { borderColor: tint }
      }
    >
      {zone.name}
    </button>
  );
}

/** The workspace's apps, one compact row each: its icon and name, a dot for every zone with its
 * own zone's lit (a click on another moves it there), and × to take it out (the owner: "show all
 * dots and the selected one is highlighted", "the remove should be a cross"). Then the windows
 * on screen to add. In the open workspace a change moves the window at once. */
function Apps({ workspace, onChanged }: { workspace: string; onChanged: () => void }) {
  const [data, setData] = useState<WorkspaceApps | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [outcome, setOutcome] = useState<{ text: string; error: boolean } | null>(null);
  const announce = useAnnounce();

  const load = useCallback(() => {
    void request({ cmd: 'apps', workspace }).then((reply) => {
      if (reply.reply === 'apps') {
        setData(reply);
        setProblem(null);
      } else {
        setProblem(errorMessage(reply) ?? 'Its apps could not be read.');
      }
    });
  }, [workspace]);

  // Windows open and close meanwhile: the list follows when Mosaïk's window comes back.
  useEffect(() => {
    load();
    window.addEventListener('focus', load);
    const stop = onWorkspacesChanged(load);
    return () => {
      window.removeEventListener('focus', load);
      stop();
    };
  }, [load]);

  const change = (key: string, command: Command, done: string) => {
    setBusy(key);
    setOutcome(null);
    void request(command).then((reply) => {
      setBusy(null);
      if (reply.reply === 'apps') {
        setData(reply);
        setOutcome({ text: done, error: false });
        announce(done);
        onChanged();
      } else {
        setOutcome({ text: errorMessage(reply) ?? 'The workspace could not be changed.', error: true });
        load();
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
  // The dots in the zones' order on screen: 1, 2, 3…
  const zones = [...data.zones].sort((a, b) => Number(a.name) - Number(b.name) || a.name.localeCompare(b.name));
  return (
    <div>
      {data.apps.length === 0 ? (
        <p className="py-3.5 text-small text-[var(--muted)]">No apps yet. Add a window that is open now.</p>
      ) : (
        <ul aria-label={`Apps of “${data.name}”`} className="divide-y divide-[var(--line)]">
          {data.apps.map((a) => (
            <li key={a.id} className="flex items-center gap-3 py-2">
              <AppIcon icon={a.icon} size={22} keepSpace />
              <span className="min-w-0 flex-1 truncate font-semibold">{a.label}</span>
              {zones.length ? (
                <span role="group" aria-label={`Zone of ${a.label}`} className="flex shrink-0 flex-wrap gap-1.5">
                  {zones.map((z) => (
                    <ZoneDot
                      key={z.id}
                      zone={z}
                      on={a.zone === z.id}
                      label={a.zone === z.id ? `${a.label} is in zone ${z.name}` : `Put ${a.label} in zone ${z.name}`}
                      disabled={busy !== null}
                      onPick={() => {
                        change(
                          a.id,
                          { cmd: 'move_app', workspace: data.workspace, app: a.id, zone: z.id },
                          `${a.label} goes in zone ${z.name}.`,
                        );
                      }}
                    />
                  ))}
                </span>
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
                className="grid size-7 shrink-0 place-items-center rounded-full text-[var(--muted)] hover:bg-[var(--hover)] hover:text-[var(--text)] focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[var(--focus)] disabled:cursor-progress disabled:opacity-50"
              >
                <Glyph name="close" size={14} />
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="flex flex-wrap items-center justify-between gap-2 border-t border-[var(--line)] py-3">
        {data.windows.length ? (
          <MenuButton
            label={`Add a window to “${data.name}”`}
            align="left"
            className="inline-flex min-h-8 items-center gap-1.5 rounded-lg px-2.5 text-small font-semibold text-[var(--accent)] hover:bg-[var(--hover)]"
            items={data.windows.map((w) => ({
              key: String(w.window),
              label: w.label,
              detail: same(w.title, w.label) ? undefined : w.title,
              icon: <AppIcon icon={w.icon} size={18} keepSpace />,
              onSelect: () => {
                change(
                  'add',
                  { cmd: 'add_app', workspace: data.workspace, window: w.window, zone: null },
                  `${w.label} added.`,
                );
              },
            }))}
          >
            <Glyph name="plus" size={14} />
            Add an open window
          </MenuButton>
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
  const [renaming, setRenaming] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [opening, setOpening] = useState(false);
  const [outcome, setOutcome] = useState<{ text: string; error: boolean } | null>(null);
  /** The picture of it opened, and whether Mosaïk could not say. */
  const [preview, setPreview] = useState<ArrangePreview | null>(null);
  const [unread, setUnread] = useState(false);
  /** Counts the times the picture is asked again: an app moved, windows on screen changed… */
  const [reads, setReads] = useState(0);
  const readAgain = useCallback(() => {
    setReads((n) => n + 1);
  }, []);
  const announce = useAnnounce();

  const load = useCallback(() => {
    void request({ cmd: 'workspaces' }).then((reply) => {
      if (reply.reply !== 'workspaces') return;
      const w = reply.workspaces.find((x) => x.id === workspace) ?? null;
      setSummary(w);
      setGone(w === null);
      setActive(reply.active === workspace);
    });
  }, [workspace]);

  useEffect(() => {
    load();
    return onWorkspacesChanged(load);
  }, [load]);

  // Its picture follows its apps and the windows on screen.
  useEffect(() => {
    window.addEventListener('focus', readAgain);
    const stop = onWorkspacesChanged(readAgain);
    return () => {
      window.removeEventListener('focus', readAgain);
      stop();
    };
  }, [readAgain]);
  useEffect(() => {
    let live = true;
    void request({ cmd: 'preview_workspace', workspace }).then((reply) => {
      if (!live) return;
      setPreview(reply.reply === 'preview' ? reply : null);
      setUnread(reply.reply !== 'preview');
    });
    return () => {
      live = false;
    };
  }, [workspace, reads]);

  const say = (text: string, error = false) => {
    setOutcome({ text, error });
    if (!error) announce(text);
  };

  if (gone) {
    return (
      <article aria-labelledby="workspace-title">
        <PageTitle id="workspace-title">This workspace is gone</PageTitle>
        <p className="mt-2 text-[var(--muted)]">It was deleted or renamed meanwhile.</p>
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
        className="-ml-2 inline-flex items-center gap-1 rounded-lg px-2 py-1 text-small font-semibold text-[var(--muted)] hover:bg-[var(--hover)] hover:text-[var(--text)]"
      >
        <Glyph name="back" size={14} />
        Home
      </button>

      <div className="mt-3 flex flex-wrap items-center gap-4">
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
                load();
              }}
            />
          ) : (
            <div className="flex flex-wrap items-center gap-2">
              <PageTitle id="workspace-title" className="truncate">
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
          <p className="mt-0.5 text-small text-[var(--muted)]">
            {active ? 'Open now · ' : ''}
            {summary.apps.length} {summary.apps.length === 1 ? 'app' : 'apps'} · {summary.zones}{' '}
            {summary.zones === 1 ? 'zone' : 'zones'}
          </p>
        </div>
        <Button
          tone="primary"
          pending={opening}
          onClick={() => {
            setOpening(true);
            void request({ cmd: 'switch', workspace: summary.id }).then((reply) => {
              setOpening(false);
              if (reply.reply === 'done') say(reply.message);
              else say(errorMessage(reply) ?? 'It could not be opened.', true);
              load();
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
        <Screen preview={preview} unread={unread} tallest={38} />
      </div>

      <Section title="Apps">
        <Apps workspace={summary.id} onChanged={readAgain} />
      </Section>

      <Section title="Shortcut">
        <ShortcutRecorder
          label={`Open “${summary.name}”`}
          description="From anywhere, without the switcher."
          value={summary.hotkey}
          workspace={summary.id}
          onSave={async (hotkey) => {
            const reply = await request({ cmd: 'set_workspace_hotkey', workspace: summary.id, hotkey });
            load();
            if (reply.reply !== 'done') return errorMessage(reply) ?? 'The shortcut could not be saved.';
            announce(reply.message);
            return null;
          }}
          onRemove={async () => {
            const reply = await request({ cmd: 'set_workspace_hotkey', workspace: summary.id, hotkey: null });
            load();
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
