// The workspaces on Home, in a column beside the screen: each a row with its layout in small and
// its apps' icons, opened in one click, with the rest in its ⋯ menu. Below them, workspaces Mosaïk
// suggests from the apps installed that go together (coding: an editor, a terminal, a browser...),
// opened the same way: their apps start and are laid out for the task. "Save layout" keeps what is
// on screen as a new one.

import { type SyntheticEvent, useCallback, useEffect, useId, useState } from 'react';

import { onWorkspacesChanged, request } from './engine';
import { AppIcon, AppIcons, Glyph, Spinner } from './icons';
import { MiniLayout } from './Layouts';
import { type AppSet, MAX_NAME, type WorkspaceSummary, errorMessage } from './protocol';
import { Button, MenuButton, Status, useAnnounce } from './ui';

/** A free name for a new workspace: "Workspace 1", "Workspace 2"… */
function freeName(taken: string[]): string {
  const used = new Set(taken.map((t) => t.trim().toLowerCase()));
  for (let n = 1; ; n += 1) {
    const name = `Workspace ${String(n)}`;
    if (!used.has(name.toLowerCase())) return name;
  }
}

function SaveForm({
  suggested,
  taken,
  onSaved,
  onCancel,
}: {
  suggested: string;
  /** The names in use: saving under one would replace that workspace. */
  taken: string[];
  onSaved: (message: string) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(suggested);
  const [problem, setProblem] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const id = useId();
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    const wanted = name.trim();
    const same = taken.find((t) => t.trim().toLowerCase() === wanted.toLowerCase());
    if (same) {
      setProblem(`“${same}” already exists. Pick another name, or use Save current layout here in its menu.`);
      return;
    }
    setPending(true);
    void request({ cmd: 'save', workspace: wanted }).then((reply) => {
      setPending(false);
      if (reply.reply === 'done') onSaved(reply.message);
      else setProblem(errorMessage(reply) ?? 'The layout could not be saved.');
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
      className="flex flex-col gap-2 rounded-[14px] border border-[var(--accent)] bg-[var(--raised)] p-3"
    >
      <label htmlFor={id} className="text-small font-semibold">
        Name the new workspace
      </label>
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
        className="field w-full"
      />
      <Status text={problem} error />
      <div className="flex gap-2">
        <Button tone="primary" type="submit" pending={pending} disabled={!name.trim()}>
          Save
        </Button>
        <Button onClick={onCancel}>Cancel</Button>
      </div>
    </form>
  );
}

/** A suggested workspace's picture: a dashed outline, as it has no zones yet, with the icons of
 * up to four of its apps. */
function IconCluster({ icons }: { icons: (string | null)[] }) {
  return (
    <span className="grid h-[34px] w-[56px] shrink-0 grid-cols-2 place-items-center gap-px rounded-[9px] border border-dashed border-[var(--control)] p-0.5">
      {icons
        .filter((i) => i !== null)
        .slice(0, 4)
        .map((icon) => (
          <AppIcon key={icon} icon={icon} size={13} />
        ))}
    </span>
  );
}

const ROW =
  'flex w-full items-center gap-3 rounded-[14px] py-2 pr-11 pl-2 text-left transition-colors duration-150 disabled:cursor-progress';

/** A spinner over a row's picture while it opens. */
const Opening = () => (
  <span className="absolute inset-0 grid place-items-center rounded-[9px] bg-black/50 text-white">
    <Spinner size={14} />
  </span>
);

export function Workspaces({
  onEdit,
  onEditZones,
  onOpened,
}: {
  onEdit: (workspace: string) => void;
  onEditZones: (workspace: string) => void;
  /** A workspace was opened: Home reads the screen again. */
  onOpened?: () => void;
}) {
  const [list, setList] = useState<WorkspaceSummary[] | null>(null);
  const [sets, setSets] = useState<AppSet[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [outcome, setOutcome] = useState<{ text: string; error: boolean } | null>(null);
  const [opening, setOpening] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const announce = useAnnounce();

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
    void request({ cmd: 'sets' }).then((reply) => {
      if (reply.reply === 'sets') setSets(reply.sets);
    });
  }, []);

  // Read now, and again whenever they change: arranged, saved from the switcher, opened with a
  // shortcut…
  useEffect(() => {
    load();
    return onWorkspacesChanged(load);
  }, [load]);

  const finish = (key: string, reply: Awaited<ReturnType<typeof request>>, failed: string) => {
    setOpening((o) => (o === key ? null : o));
    if (reply.reply === 'done') {
      setOutcome({ text: reply.message, error: false });
      announce(reply.message);
    } else {
      setOutcome({ text: errorMessage(reply) ?? failed, error: true });
    }
    load();
    onOpened?.();
  };

  const open = (w: WorkspaceSummary) => {
    setOpening(w.id);
    setOutcome(null);
    void request({ cmd: 'switch', workspace: w.id }).then((reply) => {
      finish(w.id, reply, `“${w.name}” could not be opened.`);
    });
  };

  const openSet = (s: AppSet) => {
    const key = `set:${s.id}`;
    setOpening(key);
    setOutcome(null);
    void request({ cmd: 'open_set', set: s.id }).then((reply) => {
      finish(key, reply, `The apps for ${s.name} could not be opened.`);
    });
  };

  const saveHere = (w: WorkspaceSummary) => {
    setOutcome(null);
    void request({ cmd: 'save', workspace: null }).then((reply) => {
      if (reply.reply === 'done') {
        setOutcome({ text: reply.message, error: false });
        announce(reply.message);
      } else {
        setOutcome({ text: errorMessage(reply) ?? `“${w.name}” could not be saved.`, error: true });
      }
      load();
    });
  };

  // Sets whose workspace exists are that workspace's row already.
  const suggested = sets.filter((s) => s.workspace === null);
  const names = (list ?? []).map((w) => w.name);

  return (
    <section
      aria-labelledby="workspaces-title"
      className="flex min-w-0 flex-col self-start rounded-[22px] bg-[var(--panel)] p-3 pt-4"
    >
      <div className="flex items-center justify-between gap-3 px-2">
        <h2 id="workspaces-title" className="font-display text-heading font-bold tracking-tight">
          Workspaces
        </h2>
        {saving ? null : (
          <Button
            tone="quiet"
            size="sm"
            onClick={() => {
              setSaving(true);
              setOutcome(null);
            }}
          >
            <Glyph name="plus" size={14} />
            Save layout
          </Button>
        )}
      </div>

      {problem ? (
        <div className="mt-2 flex flex-wrap items-center gap-3 px-2">
          <Status text={problem} error />
          <Button size="sm" onClick={load}>
            Try again
          </Button>
        </div>
      ) : null}

      {saving ? (
        <div className="mt-3">
          <SaveForm
            suggested={freeName(names)}
            taken={names}
            onCancel={() => {
              setSaving(false);
            }}
            onSaved={(message) => {
              setSaving(false);
              setOutcome({ text: message, error: false });
              announce(message);
              load();
            }}
          />
        </div>
      ) : null}

      <ul aria-label="Workspaces" className="mt-2 flex flex-col gap-0.5">
        {(list ?? []).map((w) => {
          const isOpen = active === w.id;
          return (
            <li key={w.id} className="relative" data-workspace={w.id}>
              <button
                type="button"
                aria-label={`Open “${w.name}”${isOpen ? ' (open now)' : ''}`}
                aria-busy={opening === w.id || undefined}
                disabled={opening !== null}
                onClick={() => {
                  open(w);
                }}
                className={`${ROW} ${isOpen ? 'bg-[var(--raised)]' : 'hover:bg-[var(--raised)]'}`}
              >
                <span className="relative">
                  <MiniLayout layout={w.layout} width={56} height={34} />
                  {opening === w.id ? <Opening /> : null}
                </span>
                <span className="flex min-w-0 flex-1 flex-col gap-1">
                  <span className="flex min-w-0 items-center gap-2">
                    <span className="truncate font-semibold">{w.name}</span>
                    {isOpen ? (
                      <span className="shrink-0 text-caption font-semibold text-[var(--accent)]">Open</span>
                    ) : null}
                  </span>
                  <span className="flex min-w-0 items-center gap-2">
                    {w.apps.length ? (
                      <AppIcons names={w.apps} icons={w.icons} size={15} most={4} />
                    ) : (
                      <span className="text-caption text-[var(--muted)]">No apps yet</span>
                    )}
                    {w.hotkey ? (
                      <span className="shrink-0 text-caption text-[var(--muted)]">
                        {w.hotkey.replaceAll('+', ' + ')}
                      </span>
                    ) : null}
                  </span>
                </span>
              </button>
              {/* Centred without a transform: one would hold the open menu under the next row. */}
              <div className="absolute inset-y-0 right-1.5 flex items-center">
                <MenuButton
                  label={`More for “${w.name}”`}
                  items={[
                    ...(isOpen
                      ? [
                          {
                            label: 'Save current layout here',
                            onSelect: () => {
                              saveHere(w);
                            },
                          },
                        ]
                      : []),
                    {
                      label: 'Edit',
                      onSelect: () => {
                        onEdit(w.id);
                      },
                    },
                    {
                      label: 'Edit zones',
                      onSelect: () => {
                        onEditZones(w.id);
                      },
                    },
                  ]}
                >
                  <Glyph name="more" />
                </MenuButton>
              </div>
            </li>
          );
        })}
      </ul>

      {list !== null && list.length === 0 && !saving ? (
        <p className="px-2 py-2 text-small text-[var(--muted)]">
          Arrange your windows, then save the layout: it opens again in one click.
        </p>
      ) : null}

      {suggested.length ? (
        <>
          <h3 className="mt-4 px-2 text-small font-semibold text-[var(--muted)]">Suggested</h3>
          <ul aria-label="Suggested workspaces" className="mt-1 flex flex-col gap-0.5">
            {suggested.map((s) => {
              const key = `set:${s.id}`;
              return (
                <li key={key} data-set={s.id}>
                  <button
                    type="button"
                    aria-label={`Open the suggested workspace “${s.name}”: ${s.apps.join(', ')}`}
                    title={s.apps.join(', ')}
                    aria-busy={opening === key || undefined}
                    disabled={opening !== null}
                    onClick={() => {
                      openSet(s);
                    }}
                    className={`${ROW} pr-2 hover:bg-[var(--raised)]`}
                  >
                    <span className="relative">
                      <IconCluster icons={s.icons} />
                      {opening === key ? <Opening /> : null}
                    </span>
                    <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                      <span className="truncate font-semibold">{s.name}</span>
                      <span className="truncate text-caption text-[var(--muted)]">
                        {s.apps.length} {s.apps.length === 1 ? 'app' : 'apps'} that go together
                      </span>
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        </>
      ) : null}

      <div className="px-2 pt-1">
        <Status text={outcome?.text ?? null} error={outcome?.error ?? false} />
      </div>
    </section>
  );
}
