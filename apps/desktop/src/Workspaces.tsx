// The saved workspaces: open one, edit its zones, give it a shortcut, rename or delete it.
// (Arranging the screen, with or without a layout, is the home page's.)

import { type SyntheticEvent, useCallback, useEffect, useId, useState } from 'react';

import { onWorkspacesChanged, request } from './engine';
import { MAX_NAME, errorMessage, type WorkspaceSummary } from './protocol';
import { ShortcutRecorder } from './shortcut';
import { Button, Confirm, Keys, MenuButton, PageTitle, Status, useSay } from './ui';

type Panel = { kind: 'rename' | 'shortcut'; id: string } | null;

function plural(n: number, one: string, many: string) {
  return `${String(n)} ${n === 1 ? one : many}`;
}

/** Focus for after a row's panel closes or a row goes: its Open button, else the title. */
function focusRow(id: string | null) {
  requestAnimationFrame(() => {
    const row = id ? document.querySelector<HTMLElement>(`[data-open="${CSS.escape(id)}"] button`) : null;
    (row ?? document.querySelector<HTMLElement>('h1[tabindex]'))?.focus();
  });
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
          className="field min-w-64 flex-1"
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

/** A small picture of zones, for the empty page. */
function EmptyPicture() {
  const tile = (color: string, area: string) => (
    <div
      className="rounded-[5px] border-2 forced-colors:border-[CanvasText]"
      style={{
        gridArea: area,
        borderColor: `var(--zone-${color})`,
        background: `color-mix(in srgb, var(--zone-${color}) 14%, transparent)`,
      }}
    />
  );
  return (
    <div
      aria-hidden="true"
      className="mx-auto grid h-20 w-32 gap-1.5 rounded-lg border border-[var(--line)] bg-[var(--bg)] p-1.5"
      style={{ gridTemplateAreas: '"a b" "a c"', gridTemplateColumns: '3fr 2fr' }}
    >
      {tile('lapis', 'a')}
      {tile('verdigris', 'b')}
      {tile('saffron', 'c')}
    </div>
  );
}

export function Workspaces({
  shortcut,
  onEditZones,
  onHome,
}: {
  shortcut: string;
  onEditZones: (workspace: string) => void;
  /** Goes to the home page, where the screen is arranged. */
  onHome: () => void;
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

  // Read now, and again whenever they change: saved from the switcher, opened with a shortcut…
  useEffect(() => {
    load();
    return onWorkspacesChanged(load);
  }, [load]);

  const openSwitcher = () => {
    void request({ cmd: 'show_switcher' }).then((reply) => {
      if (reply.reply === 'error') say(errorMessage(reply) ?? 'The switcher did not open.', true);
    });
  };

  const open = (w: WorkspaceSummary) => {
    setOpening(w.id);
    void request({ cmd: 'switch', workspace: w.id }).then((reply) => {
      setOpening(null);
      if (reply.reply === 'done') say(reply.message);
      else say(errorMessage(reply) ?? 'It could not be opened.', true);
      load();
    });
  };

  const closePanel = (id: string) => {
    setPanel(null);
    focusRow(id);
  };

  return (
    <article aria-labelledby="workspaces-title">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <PageTitle id="workspaces-title">Workspaces</PageTitle>
          <p className="mt-2 text-[var(--muted)]">
            Open one to bring its windows back where you left them. To save the windows on screen as a workspace, press{' '}
            <Keys shortcut={shortcut} /> then <Keys shortcut="S" />.
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-1.5">
          {list?.length ? <Button onClick={openSwitcher}>Open the switcher</Button> : null}
        </div>
      </div>

      {problem ? (
        <div className="mt-6 flex flex-wrap items-center gap-3">
          <p role="alert" className="text-[var(--danger)]">
            {problem}
          </p>
          <Button onClick={load}>Try again</Button>
        </div>
      ) : list === null ? (
        <p className="mt-6 text-[var(--muted)]" aria-busy="true">
          Reading your workspaces…
        </p>
      ) : list.length === 0 ? (
        <div className="mt-8 rounded-xl border border-dashed border-[var(--control)] px-8 py-10 text-center">
          <EmptyPicture />
          <h2 className="mt-5 font-display text-heading font-semibold">No workspaces yet</h2>
          <p className="mx-auto mt-1 max-w-md text-small text-[var(--muted)]">
            Open the apps of one project and arrange them: Mosaïk keeps them as a workspace.
          </p>
          <div className="mt-5">
            <Button tone="primary" onClick={onHome}>
              Arrange windows
            </Button>
          </div>
        </div>
      ) : (
        <ul className="mt-6 flex flex-col gap-3" aria-label="Saved workspaces">
          {list.map((w) => (
            <li key={w.id} className="rounded-xl border border-[var(--line)] bg-[var(--surface)] p-4">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="min-w-0">
                  <h2 className="flex items-center gap-2 font-display text-heading font-semibold">
                    <span className="truncate">{w.name}</span>
                    {active === w.id ? (
                      <span className="rounded-full border border-[var(--ok)] px-2 font-sans text-caption font-semibold leading-5 text-[var(--ok)]">
                        Open now
                      </span>
                    ) : null}
                  </h2>
                  <p className="mt-0.5 text-small text-[var(--muted)]">
                    {plural(w.windows, 'window', 'windows')} · {plural(w.zones, 'zone', 'zones')}
                    {w.hotkey ? (
                      <>
                        {' · '}
                        <Keys shortcut={w.hotkey} />
                      </>
                    ) : null}
                  </p>
                </div>
                <div className="flex items-center gap-1.5">
                  <span data-open={w.id} className="contents">
                    <Button
                      tone="primary"
                      label={`Open “${w.name}”`}
                      pending={opening === w.id}
                      onClick={() => {
                        open(w);
                      }}
                    >
                      Open
                    </Button>
                  </span>
                  <Button
                    label={`Edit zones of “${w.name}”`}
                    onClick={() => {
                      onEditZones(w.id);
                    }}
                  >
                    Edit zones
                  </Button>
                  <MenuButton
                    label={`More for “${w.name}”`}
                    items={[
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
                </div>
              </div>
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
    </article>
  );
}
