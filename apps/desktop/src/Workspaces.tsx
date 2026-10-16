// The workspaces on Home, in a column beside the screen: each a row with its layout in small and
// its apps' icons. A click shows it in Home's picture as opening it would leave the screen, with
// Open; a double-click opens it at once; its ⋯ menu has the rest: Open, Replace with what's on
// screen (the one on screen), Rename (in its row), and Edit, its page, where its zones are edited
// too (the owner: "we should have rename, and not sure we should keep edit zones it feels like it
// should be on workspace edit screen", ADR 0034). Below them, workspaces Mosaïk
// suggests from the apps installed that go together (coding: an editor, a terminal, a
// browser...): a click shows what starting them does, and Home's button starts them. "Save as
// workspace" keeps what is on screen as a new one. What all this did is said under Home's
// picture, in one place.

import { type SyntheticEvent, useEffect, useId, useRef, useState } from 'react';

import { request } from './engine';
import { AppIcon, AppIcons, Glyph, Spinner } from './icons';
import { MiniLayout } from './Layouts';
import { type AppSet, MAX_NAME, type WorkspaceSummary, errorMessage } from './protocol';
import { useReads } from './reads';
import { Button, MenuButton, OnScreen, Status } from './ui';

/** A workspace, or a suggested set of apps (by its id). */
export interface Target {
  kind: 'workspace' | 'set';
  id: string;
}

/** What Home shows in its picture instead of the screen: a workspace, or a suggested set. */
export type Shown = { kind: 'workspace'; workspace: WorkspaceSummary } | { kind: 'set'; set: AppSet };

/** The suggested sets change only as apps are installed: read again on coming back to the window
 * at most this often (and whenever the workspaces change, as a set becomes one). */
const SETS_EVERY_MS = 30_000;

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
  active,
  onSaved,
  onCancel,
}: {
  suggested: string;
  /** The names in use: saving under one would replace that workspace. */
  taken: string[];
  /** The name of the workspace on screen, whose menu can take what is on screen. */
  active: string | null;
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
      // Only the workspace on screen offers to take what is on screen.
      setProblem(
        same === active
          ? `“${same}” already exists. Pick another name, or open its ⋯ menu and choose Replace with what’s on screen.`
          : `“${same}” already exists. Pick another name.`,
      );
      return;
    }
    setPending(true);
    void request({ cmd: 'save', workspace: wanted }).then((reply) => {
      setPending(false);
      if (reply.reply === 'done') onSaved(reply.message);
      else setProblem(errorMessage(reply) ?? 'The workspace could not be saved.');
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

/** A workspace renamed in its row: its name, chosen to be typed over. Enter saves it, Esc leaves it
 * as it was; a name Mosaïk refuses (taken, empty) is said in place. */
function RenameForm({
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
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    const wanted = name.trim();
    if (wanted === workspace.name) {
      onCancel();
      return;
    }
    setPending(true);
    void request({ cmd: 'rename_workspace', workspace: workspace.id, name: wanted }).then((reply) => {
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
          e.stopPropagation();
          onCancel();
        }
      }}
      className="flex flex-col gap-2 rounded-[14px] border border-[var(--accent)] bg-[var(--raised)] p-2"
    >
      <input
        aria-label={`Rename “${workspace.name}”`}
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
        className="field w-full font-semibold"
      />
      <Status text={problem} error />
      <div className="flex gap-2">
        <Button tone="primary" size="sm" type="submit" pending={pending} disabled={!name.trim()}>
          Save name
        </Button>
        <Button size="sm" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </form>
  );
}

/** A suggested workspace's picture: a dashed outline, as it has no zones yet, with the icons of
 * up to four of its apps. */
function IconCluster({ icons }: { icons: (string | null)[] }) {
  return (
    <span className="grid h-[30px] w-[48px] shrink-0 grid-cols-2 place-items-center gap-px rounded-[9px] border border-dashed border-[var(--control)] p-0.5">
      {icons
        .filter((i) => i !== null)
        .slice(0, 4)
        .map((icon) => (
          <AppIcon key={icon} icon={icon} size={12} />
        ))}
    </span>
  );
}

const ROW =
  'flex w-full items-center gap-2.5 rounded-[14px] py-2 pl-2 text-left transition-colors duration-150 disabled:cursor-progress';

/** A row's look: ringed when Home shows it, raised when it is on screen or pointed at. */
const look = (shown: boolean, raised: boolean) =>
  shown
    ? 'bg-[var(--raised)] ring-1 ring-[var(--accent)] forced-colors:ring-[Highlight]'
    : raised
      ? 'bg-[var(--raised)]'
      : 'hover:bg-[var(--raised)]';

/** A spinner over a row's picture while it opens. */
const Opening = () => (
  <span className="absolute inset-0 grid place-items-center rounded-[9px] bg-black/50 text-white">
    <Spinner size={14} />
  </span>
);

const same = (a: Target | null, kind: Target['kind'], id: string) => a?.kind === kind && a.id === id;

export function Workspaces({
  shown,
  launching,
  onShow,
  onOpen,
  onSay,
  onEdit,
}: {
  /** What Home's picture shows. */
  shown: Target | null;
  /** What is being opened. */
  launching: Target | null;
  /** A row clicked: Home shows it (it stays shown when clicked again). */
  onShow: (shown: Shown) => void;
  /** Opens a workspace: a double-click on its row, or Open in its menu. */
  onOpen: (workspace: WorkspaceSummary) => void;
  /** Says what an action did, under Home's picture. */
  onSay: (text: string, error?: boolean) => void;
  onEdit: (workspace: string) => void;
}) {
  const [list, setList] = useState<WorkspaceSummary[] | null>(null);
  const [sets, setSets] = useState<AppSet[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  /** The workspace being renamed in its row. */
  const [renaming, setRenaming] = useState<string | null>(null);
  /** The workspace whose menu button takes the focus back, once its row is back. */
  const refocus = useRef<string | null>(null);
  const column = useRef<HTMLElement>(null);
  /** When the suggested sets were last read. */
  const setsRead = useRef(0);

  useEffect(() => {
    const id = refocus.current;
    if (id === null) return;
    refocus.current = null;
    // Its ⋯ button, the one control of the row that says whether it is expanded.
    column.current?.querySelector<HTMLElement>(`li[data-workspace="${CSS.escape(id)}"] [aria-expanded]`)?.focus();
  });

  // Read now, and again whenever Mosaïk says they changed: arranged, saved from the switcher,
  // opened with a shortcut… Its own changes need no read of their own: Mosaïk says so for them
  // too.
  const poke = useReads(async (why, current) => {
    const changed = why.has('start') || why.has('changed') || why.has('own');
    if (changed) {
      const reply = await request({ cmd: 'workspaces' });
      if (!current()) return;
      if (reply.reply === 'workspaces') {
        setList(reply.workspaces);
        setActive(reply.active);
        setProblem(null);
      } else {
        setProblem(errorMessage(reply) ?? 'Your workspaces could not be read.');
      }
    }
    if (changed || Date.now() - setsRead.current >= SETS_EVERY_MS) {
      setsRead.current = Date.now();
      const reply = await request({ cmd: 'sets' });
      if (!current()) return;
      if (reply.reply === 'sets') setSets(reply.sets);
    }
  }, 'workspaces');

  const saveHere = (w: WorkspaceSummary) => {
    void request({ cmd: 'save', workspace: null }).then((reply) => {
      if (reply.reply === 'done') onSay(reply.message);
      else onSay(errorMessage(reply) ?? `“${w.name}” could not be saved.`, true);
    });
  };

  // Sets whose workspace exists are that workspace's row already.
  const suggested = sets.filter((s) => s.workspace === null);
  const names = (list ?? []).map((w) => w.name);
  const activeName = list?.find((w) => w.id === active)?.name ?? null;

  return (
    <section
      ref={column}
      aria-labelledby="workspaces-title"
      className="flex min-w-0 flex-col self-start rounded-[22px] bg-[var(--panel)] p-2 pt-3.5"
    >
      <div className="flex flex-wrap items-center justify-between gap-x-2 gap-y-1 pl-2">
        <h2 id="workspaces-title" className="font-display text-heading font-bold tracking-tight">
          Workspaces
        </h2>
        {saving ? null : (
          <Button
            tone="quiet"
            size="sm"
            onClick={() => {
              setSaving(true);
            }}
          >
            <Glyph name="plus" size={14} />
            Save as workspace
          </Button>
        )}
      </div>

      {problem ? (
        <div className="mt-2 flex flex-wrap items-center gap-3 px-2">
          <Status text={problem} error />
          <Button
            size="sm"
            onClick={() => {
              poke('changed');
            }}
          >
            Try again
          </Button>
        </div>
      ) : null}

      {saving ? (
        <div className="mt-3">
          <SaveForm
            suggested={freeName(names)}
            taken={names}
            active={activeName}
            onCancel={() => {
              setSaving(false);
            }}
            onSaved={(message) => {
              setSaving(false);
              onSay(message);
            }}
          />
        </div>
      ) : null}

      <ul aria-label="Workspaces" className="mt-2 flex flex-col gap-0.5">
        {(list ?? []).map((w) => {
          const isOpen = active === w.id;
          const opening = same(launching, 'workspace', w.id);
          if (renaming === w.id) {
            const done = () => {
              refocus.current = w.id;
              setRenaming(null);
            };
            return (
              <li key={w.id} data-workspace={w.id}>
                <RenameForm
                  workspace={w}
                  onCancel={done}
                  onDone={(message) => {
                    done();
                    onSay(message);
                  }}
                />
              </li>
            );
          }
          return (
            <li key={w.id} className="relative" data-workspace={w.id}>
              <button
                type="button"
                aria-label={`${w.name}${isOpen ? ' (on screen)' : ''}`}
                aria-pressed={same(shown, 'workspace', w.id)}
                aria-busy={opening || undefined}
                disabled={launching !== null}
                onClick={() => {
                  onShow({ kind: 'workspace', workspace: w });
                }}
                onDoubleClick={() => {
                  onOpen(w);
                }}
                className={`${ROW} pr-12 ${look(same(shown, 'workspace', w.id), isOpen)}`}
              >
                <span className="relative">
                  <MiniLayout layout={w.layout} width={48} height={30} />
                  {opening ? <Opening /> : null}
                </span>
                <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span className="line-clamp-2 font-semibold break-words">{w.name}</span>
                  <span className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5 text-caption text-[var(--muted)]">
                    {isOpen ? <OnScreen /> : null}
                    {w.apps.length ? (
                      <AppIcons names={w.apps} icons={w.icons} size={15} most={4} />
                    ) : (
                      <span>No apps yet</span>
                    )}
                  </span>
                </span>
              </button>
              {/* Centred without a transform: one would hold the open menu under the next row. */}
              <div className="absolute inset-y-0 right-1 flex items-center">
                <MenuButton
                  label={`More for “${w.name}”`}
                  items={[
                    {
                      label: isOpen ? 'Open again' : 'Open',
                      onSelect: () => {
                        onOpen(w);
                      },
                    },
                    ...(isOpen
                      ? [
                          {
                            label: 'Replace with what’s on screen',
                            onSelect: () => {
                              saveHere(w);
                            },
                          },
                        ]
                      : []),
                    {
                      label: 'Rename',
                      onSelect: () => {
                        setRenaming(w.id);
                      },
                    },
                    {
                      label: 'Edit',
                      onSelect: () => {
                        onEdit(w.id);
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
          Arrange your windows, then save them as a workspace to bring them back later.
        </p>
      ) : null}

      {suggested.length ? (
        <>
          <h3 className="mt-4 px-2 text-small font-semibold text-[var(--muted)]">Suggested</h3>
          <ul aria-label="Suggested workspaces" className="mt-1 flex flex-col gap-0.5">
            {suggested.map((s) => (
              <li key={s.id} data-set={s.id}>
                <button
                  type="button"
                  aria-label={`${s.name} (suggested): ${s.apps.join(', ')}`}
                  aria-pressed={same(shown, 'set', s.id)}
                  aria-busy={same(launching, 'set', s.id) || undefined}
                  title={s.apps.join(', ')}
                  disabled={launching !== null}
                  onClick={() => {
                    onShow({ kind: 'set', set: s });
                  }}
                  className={`${ROW} pr-2 ${look(same(shown, 'set', s.id), false)}`}
                >
                  <span className="relative">
                    <IconCluster icons={s.icons} />
                    {same(launching, 'set', s.id) ? <Opening /> : null}
                  </span>
                  <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                    <span className="line-clamp-2 font-semibold break-words">{s.name}</span>
                    <span className="text-caption text-[var(--muted)]">
                      {s.apps.length} {s.apps.length === 1 ? 'app' : 'apps'} that go together
                    </span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </>
      ) : null}
    </section>
  );
}
