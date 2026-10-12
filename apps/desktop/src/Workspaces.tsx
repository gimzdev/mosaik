// The saved workspaces: open one, edit its zones, rename it, give it a shortcut, delete it.

import { type SyntheticEvent, useCallback, useEffect, useId, useState } from 'react';

import { request } from './engine';
import { errorMessage, type Reply, type WorkspaceSummary } from './protocol';
import { ShortcutRecorder } from './shortcut';
import { Button, Confirm, Keys, Status } from './ui';

type Editing = { kind: 'rename' | 'shortcut'; id: string } | null;

function plural(n: number, one: string, many: string) {
  return `${String(n)} ${n === 1 ? one : many}`;
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
  const id = useId();
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    void request({ cmd: 'rename_workspace', workspace: workspace.id, name }).then((reply) => {
      if (reply.reply === 'done') onDone(reply.message);
      else setProblem(errorMessage(reply) ?? 'It could not be renamed.');
    });
  };
  return (
    <form onSubmit={submit} className="flex flex-col gap-2 pt-3">
      <label htmlFor={id} className="text-[13px] font-semibold">
        New name
      </label>
      <div className="flex flex-wrap gap-2">
        <input
          id={id}
          value={name}
          maxLength={60}
          // Focus goes where the user is about to type.
          autoFocus
          onChange={(e) => {
            setName(e.target.value);
            setProblem(null);
          }}
          aria-invalid={problem !== null}
          className="min-w-64 flex-1 rounded-lg border border-[var(--line)] bg-[var(--bg)] px-3 py-1.5 select-text"
        />
        <Button tone="primary" type="submit" disabled={!name.trim()}>
          Save name
        </Button>
        <Button onClick={onCancel}>Cancel</Button>
      </div>
      <Status text={problem} error />
    </form>
  );
}

export function Workspaces({
  switcherShortcut,
  onEditZones,
}: {
  switcherShortcut: string;
  onEditZones: (workspace: string) => void;
}) {
  const [list, setList] = useState<WorkspaceSummary[] | null>(null);
  const [active, setActive] = useState<string | null>(null);
  const [editing, setEditing] = useState<Editing>(null);
  const [deleting, setDeleting] = useState<WorkspaceSummary | null>(null);
  const [status, setStatus] = useState<{ text: string; error: boolean } | null>(null);

  const apply = useCallback((reply: Reply) => {
    if (reply.reply === 'workspaces') {
      setList(reply.workspaces);
      setActive(reply.active);
    } else {
      setStatus({ text: errorMessage(reply) ?? 'Your workspaces could not be read.', error: true });
    }
  }, []);
  const load = useCallback(() => {
    void request({ cmd: 'workspaces' }).then(apply);
  }, [apply]);

  useEffect(() => {
    let live = true;
    void request({ cmd: 'workspaces' }).then((reply) => {
      if (live) apply(reply);
    });
    return () => {
      live = false;
    };
  }, [apply]);

  const after = (text: string, error = false) => {
    setStatus({ text, error });
    setEditing(null);
    load();
  };

  const open = (w: WorkspaceSummary) => {
    void request({ cmd: 'switch', workspace: w.id }).then((reply) => {
      after(reply.reply === 'done' ? reply.message : (errorMessage(reply) ?? 'It could not be opened.'), reply.reply === 'error');
    });
  };

  return (
    <article aria-labelledby="workspaces-title">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 id="workspaces-title" className="font-display text-3xl font-bold tracking-tight">
            Workspaces
          </h1>
          <p className="mt-2 text-[var(--muted)]">
            To save the windows on screen as a workspace, press <Keys shortcut={switcherShortcut} />, then{' '}
            <Keys shortcut="S" />.
          </p>
        </div>
        <Button
          onClick={() => {
            void request({ cmd: 'show_switcher' });
          }}
        >
          Open the switcher
        </Button>
      </div>
      <div className="mt-4">
        <Status text={status?.text ?? null} error={status?.error ?? false} />
      </div>

      {list === null ? (
        <p className="mt-6 text-[var(--muted)]" aria-busy="true">
          Reading your workspaces…
        </p>
      ) : list.length === 0 ? (
        <div className="mt-6 rounded-xl border border-dashed border-[var(--line)] p-8 text-center">
          <h2 className="font-display text-lg font-semibold">No workspaces yet</h2>
          <p className="mx-auto mt-2 max-w-md text-[var(--muted)]">
            Arrange the windows you use for a project, press <Keys shortcut={switcherShortcut} />, then{' '}
            <Keys shortcut="S" /> and name it.
          </p>
        </div>
      ) : (
        <ul className="mt-4 flex flex-col gap-3" aria-label="Saved workspaces">
          {list.map((w) => (
            <li key={w.id} className="rounded-xl border border-[var(--line)] bg-[var(--surface)] p-4">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="min-w-0">
                  <h2 className="flex items-center gap-2 font-display text-lg font-semibold">
                    <span className="truncate">{w.name}</span>
                    {active === w.id ? (
                      <span className="rounded-full border border-[var(--ok)] px-2 py-0.5 font-sans text-[11px] font-semibold text-[var(--ok)]">
                        Open now
                      </span>
                    ) : null}
                  </h2>
                  <p className="mt-0.5 text-[13px] text-[var(--muted)]">
                    {plural(w.windows, 'window', 'windows')} · {plural(w.zones, 'zone', 'zones')}
                    {w.hotkey ? (
                      <>
                        {' · '}
                        <Keys shortcut={w.hotkey} />
                      </>
                    ) : null}
                  </p>
                </div>
                <div className="flex flex-wrap gap-1">
                  <Button
                    tone="primary"
                    label={`Open “${w.name}”`}
                    onClick={() => {
                      open(w);
                    }}
                  >
                    Open
                  </Button>
                  <Button
                    label={`Edit the zones of “${w.name}”`}
                    onClick={() => {
                      onEditZones(w.id);
                    }}
                  >
                    Zones
                  </Button>
                  <Button
                    tone="quiet"
                    label={`Shortcut for “${w.name}”`}
                    onClick={() => {
                      setEditing({ kind: 'shortcut', id: w.id });
                    }}
                  >
                    Shortcut
                  </Button>
                  <Button
                    tone="quiet"
                    label={`Rename “${w.name}”`}
                    onClick={() => {
                      setEditing({ kind: 'rename', id: w.id });
                    }}
                  >
                    Rename
                  </Button>
                  <Button
                    tone="quiet"
                    label={`Delete “${w.name}”`}
                    onClick={() => {
                      setDeleting(w);
                    }}
                  >
                    Delete
                  </Button>
                </div>
              </div>
              {editing?.id === w.id && editing.kind === 'rename' ? (
                <Rename
                  workspace={w}
                  onDone={(m) => {
                    after(m);
                  }}
                  onCancel={() => {
                    setEditing(null);
                  }}
                />
              ) : null}
              {editing?.id === w.id && editing.kind === 'shortcut' ? (
                <div className="mt-2 border-t border-[var(--line)]">
                  <ShortcutRecorder
                    label={`Switch to “${w.name}”`}
                    value={w.hotkey}
                    workspace={w.id}
                    onSave={async (hotkey) => {
                      const reply = await request({ cmd: 'set_workspace_hotkey', workspace: w.id, hotkey });
                      if (reply.reply === 'done') {
                        after(reply.message);
                        return null;
                      }
                      return errorMessage(reply) ?? 'The shortcut could not be saved.';
                    }}
                    onRemove={async () => {
                      const reply = await request({ cmd: 'set_workspace_hotkey', workspace: w.id, hotkey: null });
                      if (reply.reply === 'done') {
                        after(reply.message);
                        return null;
                      }
                      return errorMessage(reply) ?? 'The shortcut could not be removed.';
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
          setDeleting(null);
        }}
        onConfirm={() => {
          const w = deleting;
          setDeleting(null);
          if (!w) return;
          void request({ cmd: 'delete_workspace', workspace: w.id }).then((reply) => {
            after(reply.reply === 'done' ? reply.message : (errorMessage(reply) ?? 'It could not be deleted.'), reply.reply === 'error');
          });
        }}
      />
    </article>
  );
}
