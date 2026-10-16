// Home: the main screen as the chosen layout would leave it, with Move windows beside it, and
// the workspaces in a column of their own. The automatic layout is chosen first (Mosaïk reads
// what the windows are for); any of the eight layouts can be chosen instead, and the picture shows
// it before anything moves. On the picture, windows dragged onto each other trade places (in one
// zone, front to back: ADR 0032) and the lines between zones resize them (ADR 0031); Undo changes
// puts the layout's own back. Move windows does it; Undo puts everything back. A workspace picked
// in the column shows in the picture as opening it would leave the screen, with Open (the owner:
// "we shouldnt open the workspace when clicking on it it should show it in the anim as preview and
// show an open button"); its apps and zones can be changed there too, and are saved in it at
// once. A suggested one shows the apps it starts, with Start these apps. Back, or Esc, shows the
// screen again. Whatever an action did, here or in the column, is said in one line under the
// picture.

import { useCallback, useEffect, useState } from 'react';

import { request } from './engine';
import { Glyph } from './icons';
import { type Choice, LayoutPicker, choiceName } from './LayoutPicker';
import {
  type AppSet,
  type ArrangeMove,
  type ArrangePreview,
  type Command,
  type RatioRect,
  TASK_WORDS,
  type Task,
  type WorkspaceSummary,
  errorMessage,
} from './protocol';
import { useReads } from './reads';
import {
  AppsToStart,
  type PictureEdits,
  Screen,
  dropped,
  moved,
  pictureWidth,
  settled,
  stretched,
  zoneIndex,
} from './Screen';
import { Button, PageTitle, Status, useAnnounce } from './ui';
import { type Change, moveApp, resizeZones } from './workspacePicture';
import { type Shown, type Sorted, type Target, Workspaces } from './Workspaces';

/** No window put elsewhere: one list, so the pictures asked for do not change with each render. */
/** A window put elsewhere on the picture, and whether it was dropped onto another of its own zone
 * (to bring it in front: a change of order alone, made once, ADR 0032). */
interface Put {
  move: ArrangeMove;
  inZone: boolean;
}

const NO_PUTS: Put[] = [];
/** No zone resized on the picture: the layout's own. */
const NO_AREAS: RatioRect[] = [];

/** Areas in a few words, to tell one picture asked for from another. */
const areasKey = (areas: readonly RatioRect[]) =>
  areas.map((a) => [a.left, a.top, a.right, a.bottom].map((n) => n.toFixed(4)).join(',')).join(';');

/** "Slack", "Slack and Teams", "Slack, Teams and Mail"; the windows of one app counted once,
 * in the place of its first ("2 File Explorer windows and Google Chrome"). */
function names(list: string[]): string {
  const counts = new Map<string, number>();
  for (const name of list) counts.set(name, (counts.get(name) ?? 0) + 1);
  const said = [...counts].map(([name, n]) => (n > 1 ? `${String(n)} ${name} windows` : name));
  if (said.length <= 1) return said.join('');
  return `${said.slice(0, -1).join(', ')} and ${said[said.length - 1] ?? ''}`;
}

/** What opening a workspace does besides moving its windows, in a sentence under its picture:
 * the apps it starts, the windows it minimises, the windows that go to other screens. */
function openingNote(preview: ArrangePreview): string | null {
  const starting = preview.windows.filter((w) => w.starting).map((w) => w.label);
  const gone = preview.windows.filter((w) => w.minimized).map((w) => w.label);
  const parts: string[] = [];
  if (starting.length) parts.push(`${names(starting)} will start.`);
  if (gone.length) parts.push(`${names(gone)} will be minimized.`);
  if (preview.elsewhere) {
    parts.push(
      preview.elsewhere === 1
        ? 'One of its windows goes to your other screen.'
        : `${String(preview.elsewhere)} of its windows go to your other screens.`,
    );
  }
  if (!parts.length && settled(preview)) return 'It is open like this already.';
  return parts.length ? parts.join(' ') : null;
}

/** What starting a suggested set does, from what Mosaïk says of it: which apps, and how many of
 * them are open already (it does not say which). Without "Start a workspace’s closed apps", only
 * the ones open are put in their zones. */
function setNote(s: AppSet, startsApps: boolean): string {
  const all = names(s.apps);
  if (!startsApps) return `${s.name} puts ${all} in their zones, if they are open.`;
  if (s.open >= s.apps.length) return `${s.name} puts ${all} in their zones.`;
  if (s.open === 0) return `${s.name} starts ${all}, and puts them in their zones.`;
  const closed = s.apps.length - s.open;
  return `${s.name} puts ${all} in their zones, starting the ${closed === 1 ? 'one' : String(closed)} not open yet.`;
}

/** What the picture shows, in a sentence under it: what will be minimised, or that nothing moves
 * (with no window to bring in front of another, `traded`). */
function note(preview: ArrangePreview | null, choice: Choice, traded: boolean): string | null {
  if (!preview || preview.windows.length === 0) return null;
  if (settled(preview) && !traded) return 'Your windows are already like this.';
  const gone = preview.windows.filter((w) => w.minimized).map((w) => w.label);
  const parts: string[] = [];
  if (gone.length) parts.push(`${names(gone)} will be minimized.`);
  const elsewhere = preview.elsewhere;
  if (choice === 'auto' && elsewhere) {
    parts.push(
      elsewhere === 1
        ? 'The window on your other screen is arranged there too.'
        : `The ${String(elsewhere)} windows on your other screens are arranged there too.`,
    );
  }
  return parts.length ? parts.join(' ') : null;
}

const target = (s: Shown): Target =>
  s.kind === 'workspace' ? { kind: 'workspace', id: s.workspace.id } : { kind: 'set', id: s.set.id };

const nameOf = (s: Shown) => (s.kind === 'workspace' ? s.workspace.name : s.set.name);

/** What an action did. */
interface Outcome {
  text: string;
  error: boolean;
}

export function Home({
  onEditWorkspace,
  onEditZones,
  sorted = null,
  onSortedRead,
  startsApps = true,
}: {
  onEditWorkspace: (workspace: string) => void;
  onEditZones: (workspace: string) => void;
  /** The first run's sorting of the windows into workspaces, until its note is put away. */
  sorted?: Sorted | null;
  onSortedRead?: () => void;
  /** Opening a workspace starts its apps that are not open ("Start a workspace’s closed apps"). */
  startsApps?: boolean;
}) {
  const announce = useAnnounce();
  const [choice, setChoice] = useState<Choice>('auto');
  /** The windows put elsewhere on the picture, in order, for the layout chosen. */
  const [puts, setPuts] = useState<Put[]>(NO_PUTS);
  /** The zones as resized on the picture, in its order, for the layout chosen. */
  const [areas, setAreas] = useState<RatioRect[]>(NO_AREAS);
  /** A workspace or a suggested set picked in the column, shown instead of the screen. */
  const [shown, setShown] = useState<Shown | null>(null);
  /** The screen as Arrange would leave it, as Mosaïk last said. */
  const [arranged, setArranged] = useState<ArrangePreview | null>(null);
  /** What the screen's windows say the user is doing, from the last automatic layout. */
  const [task, setTask] = useState<Task | null>(null);
  /** The picture of a workspace picked, once Mosaïk has said (for the workspace it was asked for). */
  const [opening, setOpening] = useState<{ id: string; preview: ArrangePreview } | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [arranging, setArranging] = useState(false);
  /** The workspace or set being opened. */
  const [launching, setLaunching] = useState<Target | null>(null);
  /** What the last action did, said under the picture. */
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const [undoable, setUndoable] = useState(false);
  /** The picture right after a window is put elsewhere or a zone resized, until Mosaïk's answer
   * comes. */
  const [early, setEarly] = useState<ArrangePreview | null>(null);
  /** The window picked on the picture (its app's name), waiting to be put elsewhere. */
  const [picking, setPicking] = useState<string | null>(null);

  const say = useCallback(
    (text: string, error = false) => {
      setOutcome({ text, error });
      if (!error) announce(text);
    },
    [announce],
  );

  const moves = puts.map((p) => p.move);
  // Windows traded in a zone (ADR 0032): which is in front, not where they are; to do until Move
  // windows does them, and done once (each is a swap of the order on screen then).
  const traded = puts.some((p) => p.inZone);
  // One picture is read: the one shown (a suggested set's is drawn from what the column read).
  // After Arrange, Undo or Open, Mosaïk says the workspaces changed, and that reads it again: a
  // command that worked needs no read of its own; one refused says nothing, so it asks for one.
  const what = shown
    ? `${shown.kind}:${target(shown).id}`
    : `arrange:${choice}:${moves.map((m) => `${String(m.window)}>${String(m.zone)}${m.onto === undefined ? '' : `~${String(m.onto)}`}`).join(',')}:${areasKey(areas)}`;
  const poke = useReads(async (_why, current) => {
    if (shown?.kind === 'set') return;
    if (shown?.kind === 'workspace') {
      const id = shown.workspace.id;
      const reply = await request({ cmd: 'preview_workspace', workspace: id });
      if (!current()) return;
      if (reply.reply === 'preview') {
        setOpening({ id, preview: reply });
      } else {
        setShown(null);
        setOutcome({ text: errorMessage(reply) ?? 'That workspace could not be shown.', error: true });
      }
      return;
    }
    const command: Command = { cmd: 'preview_arrange' };
    if (choice !== 'auto') command.preset = choice;
    if (moves.length) command.moves = moves;
    if (areas.length) command.areas = areas;
    const reply = await request(command);
    if (!current()) return;
    if (reply.reply === 'preview') {
      setArranged(reply);
      if (choice === 'auto') setTask(reply.task ?? null);
      setEarly(null);
      setProblem(null);
      // The zones resized are another layout's now (the windows on screen changed): let go.
      if (areas.length && reply.zones.length !== areas.length) setAreas(NO_AREAS);
    } else {
      setProblem(errorMessage(reply) ?? 'Mosaïk could not read your screen.');
    }
  }, what);

  /** Shows the screen again, with focus on the row of what was shown rather than lost with the
   * Back button. */
  const back = useCallback(() => {
    const was = shown;
    setShown(null);
    setOutcome(null);
    if (!was) return;
    requestAnimationFrame(() => {
      const active = document.activeElement;
      if (active && active !== document.body && active.isConnected) return;
      const row =
        was.kind === 'workspace'
          ? `[data-workspace="${CSS.escape(was.workspace.id)}"] > button`
          : `[data-set="${CSS.escape(was.set.id)}"] > button`;
      document.querySelector<HTMLElement>(row)?.focus();
    });
  }, [shown]);

  // Esc shows the screen again (unless something on the page took the key for itself).
  useEffect(() => {
    if (!shown) return;
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !e.defaultPrevented) back();
    };
    document.addEventListener('keydown', key);
    return () => {
      document.removeEventListener('keydown', key);
    };
  }, [shown, back]);

  /** Puts the layout's own windows and zones back. */
  const reset = () => {
    setPuts(NO_PUTS);
    setAreas(NO_AREAS);
    setEarly(null);
    setPicking(null);
  };

  const arrange = () => {
    setArranging(true);
    setOutcome(null);
    const command: Command = { cmd: 'arrange' };
    if (choice !== 'auto') command.preset = choice;
    if (moves.length) command.moves = moves;
    if (areas.length) command.areas = areas;
    void request(command).then((reply) => {
      setArranging(false);
      if (reply.reply === 'done') {
        say(reply.message);
        setUndoable(true);
        // Done: the screen holds the moves and the zones as stretched now, and Mosaïk keeps it
        // as it is while every window stays in its zone (ADR 0032), so they are let go: sent
        // again, each move would trade the windows back. The picture shows the screen as it is.
        setPuts(NO_PUTS);
        setAreas(NO_AREAS);
      } else {
        say(errorMessage(reply) ?? 'Your windows could not be arranged.', true);
        // Some windows may have moved all the same.
        poke('changed');
      }
    });
  };

  const undo = () => {
    setUndoable(false);
    void request({ cmd: 'undo_arrange' }).then((reply) => {
      if (reply.reply === 'done') {
        say(reply.message);
      } else {
        say(errorMessage(reply) ?? 'This could not be undone.', true);
        poke('changed');
      }
    });
  };

  /** Opens a workspace or starts a set's apps; once done, the screen shows again. */
  const launch = (it: Target, command: Command, failed: string) => {
    setLaunching(it);
    setOutcome(null);
    void request(command).then((reply) => {
      setLaunching(null);
      if (reply.reply === 'done') {
        say(reply.message);
        setShown(null);
        // Other windows are on screen now: the layout starts afresh for them.
        reset();
        // Opening a workspace can be undone, as Arrange can: it minimised the other windows on
        // its screens (ADR 0029). A set started is not: its apps would stay open.
        setUndoable(it.kind === 'workspace');
      } else {
        say(errorMessage(reply) ?? failed, true);
        poke('changed');
      }
    });
  };
  const open = (w: WorkspaceSummary) => {
    launch({ kind: 'workspace', id: w.id }, { cmd: 'switch', workspace: w.id }, `“${w.name}” could not be opened.`);
  };
  const start = (s: AppSet) => {
    launch({ kind: 'set', id: s.id }, { cmd: 'open_set', set: s.id }, `The apps of ${s.name} could not be started.`);
  };

  /** A window dropped in a zone of the picture, `onto` a window there (the two trade places,
   * ADR 0032) or onto the zone itself: it shows so at once, then as Mosaïk says. */
  const move = (window: number, zone: number, onto: number | null) => {
    const w = picture?.windows.find((x) => x.window === window);
    const inZone = onto !== null && picture !== null && w !== undefined && zoneIndex(picture, w) === zone;
    if (picture) setEarly(moved(picture, window, zone, onto));
    setPuts((list) => [...list, { move: onto === null ? { window, zone } : { window, zone, onto }, inZone }]);
    setOutcome(null);
  };

  /** The zones resized on the picture: they show so at once, then as Mosaïk says. */
  const stretch = (next: RatioRect[]) => {
    if (picture) setEarly(stretched(picture, next));
    setAreas(next);
    setOutcome(null);
  };

  const choose = (next: Choice) => {
    setChoice(next);
    reset();
    setOutcome(null);
    setShown(null);
  };

  /** A change made on a workspace's picture, saved in it: said once Mosaïk answers, and the
   * picture read again. What Arrange or opening a workspace did is no longer undone after it. */
  const edited = (change: Promise<Change>) => {
    setOutcome(null);
    void change.then(({ text, done }) => {
      say(text, !done);
      if (done) setUndoable(false);
      poke(done ? 'own' : 'changed');
    });
  };

  // While a layout's or a workspace's answer is on its way, the last picture stays.
  const screenPicture = early ?? arranged;
  const picture = shown?.kind === 'workspace' ? (opening?.preview ?? screenPicture) : screenPicture;
  const hasWindows = (arranged?.windows.length ?? 0) > 0;
  const movable = hasWindows && !arranging && !shown;

  /** What can be changed on the picture: the screen's windows and zones before Arrange, or a
   * workspace's apps and zones once its own picture is in. */
  const pictureEdits = ((): PictureEdits | undefined => {
    if (shown?.kind === 'workspace') {
      const id = shown.workspace.id;
      const name = shown.workspace.name;
      if (opening?.id !== id || launching) return undefined;
      const now = opening.preview;
      return {
        kind: 'workspace',
        onDrop: (d) => {
          setOpening({ id, preview: dropped(now, d) });
          edited(moveApp(id, name, now, d));
        },
        onStretch: (next) => {
          setOpening({ id, preview: stretched(now, next) });
          edited(resizeZones(id, name, now, next));
        },
      };
    }
    if (!movable) return undefined;
    return {
      kind: 'arrange',
      onDrop: (d) => {
        move(d.piece.window, d.zone, d.onto?.window ?? null);
      },
      onStretch: stretch,
    };
  })();
  const words = task ? TASK_WORDS[task] : null;
  const layout =
    choice === 'auto' ? (words ? `Automatic layout for ${words}` : 'Automatic layout') : choiceName(choice);
  const subtitle = shown
    ? shown.kind === 'set' && startsApps
      ? 'Nothing starts until you press Start these apps.'
      : 'Nothing moves until you press Open.'
    : problem
      ? 'Your screen could not be read.'
      : hasWindows || !arranged
        ? `${layout}. Nothing moves yet.`
        : 'Open some apps first.';
  const said = picking
    ? `Choose where ${picking} goes. Esc cancels.`
    : shown
      ? shown.kind === 'set'
        ? setNote(shown.set, startsApps)
        : opening?.id === shown.workspace.id
          ? openingNote(opening.preview)
          : null
      : note(picture, choice, traded);
  const hint =
    pictureEdits && !said
      ? shown
        ? 'Drag an app onto another to swap them, or a line between zones to resize them.'
        : 'Drag a window onto another to swap them, or a line between zones to resize them.'
      : null;
  const busy =
    launching !== null && shown !== null && launching.kind === shown.kind && launching.id === target(shown).id;

  /** The main button: Open for a workspace shown, Start these apps for a set, else Arrange
   * windows, with Mosaïk's own tiles (Automatic has the spark). */
  const action = () => {
    if (shown?.kind === 'workspace') {
      return (
        <Button
          tone="primary"
          size="lg"
          pending={busy}
          disabled={launching !== null}
          label={`Open “${shown.workspace.name}”`}
          onClick={() => {
            open(shown.workspace);
          }}
        >
          {busy ? null : <Glyph name="play" size={13} />}
          Open
        </Button>
      );
    }
    if (shown?.kind === 'set') {
      return (
        <Button
          tone="primary"
          size="lg"
          pending={busy}
          disabled={launching !== null}
          {...(startsApps ? {} : { label: `Open “${shown.set.name}”` })}
          onClick={() => {
            start(shown.set);
          }}
        >
          {busy ? null : <Glyph name="play" size={13} />}
          {startsApps ? 'Start these apps' : 'Open'}
        </Button>
      );
    }
    return (
      <Button tone="primary" size="lg" pending={arranging} disabled={!hasWindows || arranging} onClick={arrange}>
        {arranging ? null : <Glyph name="tiles" />}
        Move windows
      </Button>
    );
  };

  return (
    <article aria-labelledby="home-title">
      <PageTitle id="home-title" className="sr-only">
        Home
      </PageTitle>
      <div className="grid gap-x-6 gap-y-8 min-[720px]:grid-cols-[minmax(0,1fr)_16rem]">
        <section aria-labelledby="arrange-title" className="min-w-0">
          {/* The title and its button, the picture, the layouts and what was done: one width, the
              picture's, so they line up (the owner: the layouts went past the picture, "it
              doesnt feel simetric"). */}
          <div
            className="flex min-w-0 flex-col"
            style={pictureWidth(picture?.work_area ?? arranged?.work_area ?? null)}
          >
            <div className="flex flex-wrap items-end justify-between gap-x-4 gap-y-2">
              <div className="min-w-0 flex-1 basis-60">
                <div className="flex flex-wrap items-center gap-x-2">
                  <h2 id="arrange-title" className="font-display text-title font-bold tracking-tight break-words">
                    {shown ? `Preview of “${nameOf(shown)}”` : 'Your screen'}
                  </h2>
                  {shown ? (
                    <Button tone="quiet" size="sm" label="Back to my screen" onClick={back}>
                      <Glyph name="back" size={14} />
                      Back
                    </Button>
                  ) : null}
                </div>
                <p className="mt-0.5 text-small text-[var(--muted)]">{subtitle}</p>
              </div>
              {/* On the right, on a line of its own too when the window is narrow. */}
              <div className="ml-auto">{action()}</div>
            </div>

            <div className="mt-4">
              {shown?.kind === 'set' ? (
                <AppsToStart
                  name={shown.set.name}
                  apps={shown.set.apps}
                  icons={shown.set.icons}
                  area={arranged?.work_area ?? null}
                />
              ) : (
                <Screen preview={picture} unread={problem !== null && !shown} edit={pictureEdits} onPick={setPicking} />
              )}
            </div>

            <div className="mt-3">
              <LayoutPicker value={shown ? null : choice} disabled={!hasWindows} onChoose={choose} />
            </div>

            <div className="mt-2 flex min-h-10 flex-wrap items-center gap-x-3 gap-y-1">
              {problem && !shown ? <Status text={problem} error /> : null}
              {outcome ? (
                <Status text={outcome.text} error={outcome.error} />
              ) : said || hint ? (
                <p className="text-small text-[var(--muted)]">{said ?? hint}</p>
              ) : null}
              {undoable ? (
                <Button tone="quiet" size="sm" onClick={undo}>
                  <Glyph name="undo" size={14} />
                  Undo
                </Button>
              ) : null}
              {(moves.length || areas.length) && !arranging && !shown && !(picture && settled(picture) && !traded) ? (
                <Button
                  tone="quiet"
                  size="sm"
                  hint="Put the windows and zones back as the layout has them"
                  onClick={reset}
                >
                  Undo changes
                </Button>
              ) : null}
            </div>
          </div>
        </section>

        <Workspaces
          shown={shown ? target(shown) : null}
          launching={launching}
          sorted={sorted}
          {...(onSortedRead ? { onSortedRead } : {})}
          onShow={(next) => {
            setShown(next);
            setOutcome(null);
            setPicking(null);
          }}
          onOpen={open}
          onSay={say}
          onEdit={onEditWorkspace}
          onEditZones={onEditZones}
        />
      </div>
    </article>
  );
}
