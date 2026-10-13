// The saved workspaces, on the home page: each with its apps at a glance, Open, and the rest in
// its ⋯ menu (its apps in full, to change them; its zones; a shortcut of its own; rename;
// delete). Opening one brings its apps back where they were left.

import { type SyntheticEvent, useCallback, useEffect, useId, useState } from 'react';

import { onWorkspacesChanged, request } from './engine';
import { MAX_NAME, errorMessage, type WorkspaceSummary } from './protocol';
import { ShortcutRecorder } from './shortcut';
import { Button, Confirm, Keys, MenuButton, Status, useSay } from './ui';
import { AppsPanel } from './WorkspaceApps';

type Panel = { kind: 'rename' | 'shortcut' | 'apps'; id: string } | null;

/** Focus for after a row's panel closes or a row goes: its Open button, else the list's title. */
function focusRow(id: string | null) {
  requestAnimationFrame(() => {
    const row = id ? document.querySelector<HTMLElement>(`[data-open="${CSS.escape(id)}"] button`) : null;
    (row ?? document.querySelector<HTMLElement>('#home-workspaces-title'))?.focus();
  });
}

/** A workspace's apps at a glance: the first few by name, then how many more. */
function AppNames({ names }: { names: string[] }) {
  if (names.length === 0) return <p className="mt-1 text-small text-[var(--muted)]">No apps yet.</p>;
  const shown = names.slice(0, 5);
  const more = names.length - shown.length;
  return (
    <ul className="mt-1.5 flex flex-wrap gap-1.5" aria-label="Its apps">
      {shown.map((n, i) => (
        <li
          key={`${n}-${String(i)}`}
          className="max-w-48 truncate rounded-full border border-[var(--line)] px-2 text-caption leading-5 text-[var(--muted)]"
        >
          {n}
        </li>
      ))}
      {more > 0 ? <li className="px-1 text-caption leading-5 text-[var(--muted)]">+{String(more)}</li> : null}
    </ul>
  );
}

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
      className="mt-3 flex flex-col gap-2 border-t border-[var(--line)] pt-3"
    >
      <label htmlFor={id} className="text-small font-semibold">
        New name
      </label>
      <div className="flex flex-wrap gap-2">
        <input
          id={id}
          value={name}
          maxLength={MAX_NAME}
          // Focus goes where the user is about to type.
          autoFocus
          onChange={(e) => {
            setName(e.target.value);
            setProblem(null);
          }}
          aria-invalid={problem !== null}
          className="field min-w-56 flex-1"
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

export function WorkspaceList({
  onEditZones,
  onOpened,
}: {
  onEditZones: (workspace: string) => void;
  /** A workspace was opened: the home page reads the screen again. */
  onOpened?: () => void;
}) {
  const [list, setList] = useState<WorkspaceSummary[] | null>(null);
  const [active, setActive] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [panel, setPanel] = useState<Panel>(null);
  const [deleting, setDeleting] = useState<WorkspaceSummary | null>(null);
  const [opening, setOpening] = useState<string | null>(null);
  const say = useSay();

  const load = useCallback(() => {
    void request({ cmd: 'workspaces' }).then((reply) => {
      if (reply.reply === 'workspaces') {
        setList(reply.workspaces);
        setActive(reply.active);
        setProblem(null);
      } else {
        setProblem(errorMessage(reply) ?? 'Your workspaces could not be read.');
      }
    });
  }, []);

  // Read now, and again whenever they change: arranged, saved from the switcher, opened with a
  // shortcut…
  useEffect(() => {
    load();
    return onWorkspacesChanged(load);
  }, [load]);

  const open = (w: WorkspaceSummary) => {
    setOpening(w.id);
    void request({ cmd: 'switch', workspace: w.id }).then((reply) => {
      setOpening(null);
      if (reply.reply === 'done') say(reply.message);
      else say(errorMessage(reply) ?? 'It could not be opened.', true);
      load();
      onOpened?.();
    });
  };

  const closePanel = (id: string) => {
    setPanel(null);
    focusRow(id);
  };

  return (
    <section aria-labelledby="home-workspaces-title" className="below-picture mt-8">
      <h2
        id="home-workspaces-title"
        tabIndex={-1}
        className="font-display text-heading font-semibold tracking-tight outline-none"
      >
        Workspaces
      </h2>
      {problem ? (
        <div className="mt-2 flex flex-wrap items-center gap-3">
          <p role="alert" className="text-small text-[var(--danger)]">
            {problem}
          </p>
          <Button onClick={load}>Try again</Button>
        </div>
      ) : list === null ? null : list.length === 0 ? (
        <p className="mt-1 text-small text-[var(--muted)]">Arranged windows are kept here, one click away.</p>
      ) : (
        <ul
          aria-label="Saved workspaces"
          className="mt-2 flex flex-col divide-y divide-[var(--line)] rounded-xl border border-[var(--line)] bg-[var(--surface)]"
        >
          {list.map((w) => (
            <li key={w.id} className="px-4 py-2.5">
              <div className="flex items-center justify-between gap-3">
                <span className="flex min-w-0 items-center gap-2">
                  <span className="truncate font-semibold">{w.name}</span>
                  {active === w.id ? (
                    <span className="shrink-0 rounded-full border border-[var(--ok)] px-2 text-caption leading-5 font-semibold text-[var(--ok)]">
                      Open now
                    </span>
                  ) : null}
                  {w.hotkey ? (
                    <span className="shrink-0 text-caption text-[var(--muted)]">
                      <Keys shortcut={w.hotkey} />
                    </span>
                  ) : null}
                </span>
                <span className="flex shrink-0 items-center gap-1">
                  <span data-open={w.id} className="contents">
                    <Button
                      label={`Open “${w.name}”`}
                      pending={opening === w.id}
                      onClick={() => {
                        open(w);
                      }}
                    >
                      Open
                    </Button>
                  </span>
                  <MenuButton
                    label={`More for “${w.name}”`}
                    items={[
                      {
                        label: panel?.id === w.id && panel.kind === 'apps' ? 'Hide its apps' : 'Change its apps…',
                        onSelect: () => {
                          setPanel((p) => (p?.id === w.id && p.kind === 'apps' ? null : { kind: 'apps', id: w.id }));
                        },
                      },
                      {
                        label: 'Edit zones',
                        onSelect: () => {
                          onEditZones(w.id);
                        },
                      },
                      {
                        label: w.hotkey ? 'Change shortcut…' : 'Add shortcut…',
                        onSelect: () => {
                          setPanel({ kind: 'shortcut', id: w.id });
                        },
                      },
                      {
                        label: 'Rename…',
                        onSelect: () => {
                          setPanel({ kind: 'rename', id: w.id });
                        },
                      },
                      {
                        label: 'Delete…',
                        danger: true,
                        onSelect: () => {
                          setDeleting(w);
                        },
                      },
                    ]}
                  >
                    <span aria-hidden="true" className="text-lg leading-none">
                      ⋯
                    </span>
                  </MenuButton>
                </span>
              </div>
              {panel?.id === w.id && panel.kind === 'apps' ? (
                <AppsPanel workspace={w.id} id={`apps-${w.id}`} />
              ) : (
                <AppNames names={w.apps} />
              )}
              {panel?.id === w.id && panel.kind === 'rename' ? (
                <Rename
                  workspace={w}
                  onDone={(m) => {
                    say(m);
                    load();
                    closePanel(w.id);
                  }}
                  onCancel={() => {
                    closePanel(w.id);
                  }}
                />
              ) : null}
              {panel?.id === w.id && panel.kind === 'shortcut' ? (
                <div className="mt-3 border-t border-[var(--line)]">
                  <ShortcutRecorder
                    label={`Switch to “${w.name}”`}
                    value={w.hotkey}
                    workspace={w.id}
                    autoStart
                    onClose={() => {
                      closePanel(w.id);
                    }}
                    onSave={async (hotkey) => {
                      const reply = await request({ cmd: 'set_workspace_hotkey', workspace: w.id, hotkey });
                      load();
                      if (reply.reply !== 'done') return errorMessage(reply) ?? 'The shortcut could not be saved.';
                      say(reply.message);
                      closePanel(w.id);
                      return null;
                    }}
                    onRemove={async () => {
                      const reply = await request({ cmd: 'set_workspace_hotkey', workspace: w.id, hotkey: null });
                      load();
                      if (reply.reply !== 'done') return errorMessage(reply) ?? 'The shortcut could not be removed.';
                      say(reply.message);
                      closePanel(w.id);
                      return null;
                    }}
                  />
                </div>
              ) : null}
            </li>
          ))}
        </ul>
      )}

      <Confirm
        open={deleting !== null}
        title={deleting ? `Delete “${deleting.name}”?` : ''}
        body="Its windows stay where they are; only the workspace goes. This cannot be undone."
        confirm="Delete workspace"
        onCancel={() => {
          const w = deleting;
          setDeleting(null);
          focusRow(w?.id ?? null);
        }}
        onConfirm={() => {
          const w = deleting;
          setDeleting(null);
          if (!w || !list) return;
          // Focus goes to the next workspace (or the one before) once this one is gone.
          const at = list.findIndex((x) => x.id === w.id);
          const neighbour = list[at + 1] ?? list[at - 1] ?? null;
          void request({ cmd: 'delete_workspace', workspace: w.id }).then((reply) => {
            if (reply.reply === 'done') say(reply.message);
            else say(errorMessage(reply) ?? 'It could not be deleted.', true);
            load();
            focusRow(reply.reply === 'done' ? (neighbour?.id ?? null) : w.id);
          });
        }}
      />
    </section>
  );
}
