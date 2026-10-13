// The home page: tidy the screen in one click. A picture of the main screen shows where each
// window would go while the pointer (or the keyboard) is on Arrange or a layout; a click does it,
// and Undo puts everything back. Below, the saved workspaces, one click away.

import { useCallback, useEffect, useState } from 'react';

import { PRESETS, type Preset, presetShape } from './editor/model';
import { onWorkspacesChanged, request } from './engine';
import { type ArrangePreview, type WorkspaceSummary, type ZoneColor, errorMessage } from './protocol';
import { Stage } from './Stage';
import { Button, Keys, PageTitle, useSay } from './ui';

/** The layouts, the most used first (landscape screens), then rows and the whole screen. */
const ORDER: readonly Preset[] = [
  'halves',
  'thirds',
  'main-side',
  'side-main',
  'half-stack',
  'stack-half',
  'quarters',
  'rows',
  'three-rows',
  'full',
];
const LAYOUTS = ORDER.map((id) => PRESETS.find((p) => p.id === id)).filter((p) => p !== undefined);
const TILE_COLORS: readonly ZoneColor[] = ['lapis', 'verdigris', 'saffron', 'madder'];

/** What the picture shows: the screen now, or what Arrange (`auto`) or a layout would do. */
type Pointing = Preset | 'auto' | null;

/** A layout as a small mosaic of its zones. */
function Tile({ preset }: { preset: Preset }) {
  return (
    <span
      aria-hidden="true"
      className="relative block aspect-[16/9] w-full rounded-md bg-[var(--bg)] forced-colors:border forced-colors:border-[CanvasText]"
    >
      {presetShape(preset).map((a, i) => (
        <span
          key={`${String(a.left)}-${String(a.top)}`}
          className="absolute rounded-[3px] forced-colors:border forced-colors:border-[CanvasText]"
          style={{
            left: `calc(${String(a.left * 100)}% + 2px)`,
            top: `calc(${String(a.top * 100)}% + 2px)`,
            width: `calc(${String((a.right - a.left) * 100)}% - 4px)`,
            height: `calc(${String((a.bottom - a.top) * 100)}% - 4px)`,
            background: `var(--zone-${TILE_COLORS[i % TILE_COLORS.length] ?? 'slate'})`,
          }}
        />
      ))}
    </span>
  );
}

function plural(n: number, one: string, many: string) {
  return `${String(n)} ${n === 1 ? one : many}`;
}

/** The caption under the picture. */
function caption(preview: ArrangePreview | null, pointing: Pointing): string {
  if (!preview) return 'Reading your screen…';
  const n = preview.windows.length;
  const elsewhere = preview.elsewhere
    ? ` (and ${plural(preview.elsewhere, 'window', 'windows')} on your other screens)`
    : '';
  if (pointing === null)
    return n === 0 ? 'Your main screen now' : `Your main screen now: ${plural(n, 'window', 'windows')}${elsewhere}`;
  const name = pointing === 'auto' ? 'Arrange' : (PRESETS.find((p) => p.id === pointing)?.label ?? pointing);
  const tabs = preview.windows.filter((w) => w.tab).length;
  const what =
    n === 0
      ? 'its zones, ready for your windows'
      : tabs
        ? `${plural(tabs, 'window', 'windows')} as ${tabs === 1 ? 'a tab' : 'tabs'}`
        : 'every window in its own zone';
  return `With ${name}: ${what}`;
}

export function Home({
  shortcut,
  edge,
  demo = false,
  onManage,
}: {
  /** The shortcut that opens the switcher now. */
  shortcut: string;
  /** Where the quick menu's line is, when it shows. */
  edge: { enabled: boolean; side: 'left' | 'right' };
  /** Show once what Arrange would do, without being asked (right after the welcome). */
  demo?: boolean;
  onManage: () => void;
}) {
  const say = useSay();
  /** The screen as it is (what Arrange would do, and where the windows are now). */
  const [base, setBase] = useState<ArrangePreview | null>(null);
  /** What the user points at (the mouse or the keyboard), and what that would do. */
  const [want, setWant] = useState<Pointing>(null);
  const [shown, setShown] = useState<{ pointing: Exclude<Pointing, null>; preview: ArrangePreview } | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState<Pointing>(null);
  const [undoable, setUndoable] = useState(false);
  const [list, setList] = useState<WorkspaceSummary[] | null>(null);
  const [active, setActive] = useState<string | null>(null);
  const [opening, setOpening] = useState<string | null>(null);

  const readScreen = useCallback(() => {
    void request({ cmd: 'preview_arrange' }).then((reply) => {
      if (reply.reply === 'preview') {
        setBase(reply);
        setProblem(null);
      } else {
        setProblem(errorMessage(reply) ?? 'Mosaïk could not read your screen.');
      }
    });
  }, []);

  const readWorkspaces = useCallback(() => {
    void request({ cmd: 'workspaces' }).then((reply) => {
      if (reply.reply === 'workspaces') {
        setList(reply.workspaces);
        setActive(reply.active);
      }
    });
  }, []);

  useEffect(() => {
    readScreen();
    readWorkspaces();
    // Windows move while the page is open: the picture follows when the window comes back.
    window.addEventListener('focus', readScreen);
    const stop = onWorkspacesChanged(() => {
      readWorkspaces();
      readScreen();
    });
    return () => {
      window.removeEventListener('focus', readScreen);
      stop();
    };
  }, [readScreen, readWorkspaces]);

  // Right after the welcome, the picture shows once what Arrange would do with the windows on
  // screen, then the screen as it is again (not for those who ask for less motion).
  const [demoing, setDemoing] = useState(false);
  const [demoDone, setDemoDone] = useState(!demo);
  const hasWindows = (base?.windows.length ?? 0) > 0;
  useEffect(() => {
    if (demoDone || !hasWindows) return;
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setTimeout(() => {
        setDemoDone(true);
      });
      return;
    }
    const start = setTimeout(() => {
      setDemoing(true);
    }, 700);
    const end = setTimeout(() => {
      setDemoing(false);
      setDemoDone(true);
    }, 3100);
    return () => {
      clearTimeout(start);
      clearTimeout(end);
    };
  }, [demoDone, hasWindows]);

  // What the user points at is asked for; an answer for something pointed at before is dropped.
  useEffect(() => {
    if (want === null) return;
    let current = true;
    void request(want === 'auto' ? { cmd: 'preview_arrange' } : { cmd: 'preview_arrange', preset: want }).then(
      (reply) => {
        if (current && reply.reply === 'preview') setShown({ pointing: want, preview: reply });
      },
    );
    return () => {
      current = false;
    };
  }, [want]);

  const arrange = (at: Exclude<Pointing, null>) => {
    setBusy(at);
    void request(at === 'auto' ? { cmd: 'arrange' } : { cmd: 'arrange', preset: at }).then((reply) => {
      setBusy(null);
      setWant(null);
      if (reply.reply === 'done') {
        say(reply.message);
        setUndoable(true);
      } else {
        say(errorMessage(reply) ?? 'Your windows could not be arranged.', true);
      }
      readScreen();
      readWorkspaces();
    });
  };

  const undo = () => {
    setUndoable(false);
    void request({ cmd: 'undo_arrange' }).then((reply) => {
      if (reply.reply === 'done') say(reply.message);
      else say(errorMessage(reply) ?? 'Arrange could not be undone.', true);
      readScreen();
      readWorkspaces();
    });
  };

  const open = (w: WorkspaceSummary) => {
    setOpening(w.id);
    void request({ cmd: 'switch', workspace: w.id }).then((reply) => {
      setOpening(null);
      if (reply.reply === 'done') say(reply.message);
      else say(errorMessage(reply) ?? 'It could not be opened.', true);
      setUndoable(false);
      readWorkspaces();
      readScreen();
    });
  };

  /** Pointing and focus both show the preview: a mouse and a keyboard get the same. */
  const previewOn = (at: Exclude<Pointing, null>) => {
    const leave = () => {
      setWant((w) => (w === at ? null : w));
    };
    const enter = () => {
      setWant(at);
    };
    return { onPointerEnter: enter, onPointerLeave: leave, onFocus: enter, onBlur: leave };
  };

  // The answer for what is pointed at now, once it has come; the screen as it is otherwise.
  const pointed = want !== null && shown?.pointing === want ? shown : null;
  const picture = pointed?.preview ?? base;
  const showing = pointed?.pointing ?? (demoing ? 'auto' : null);

  return (
    <article aria-labelledby="home-title">
      <div className="text-center">
        <PageTitle id="home-title">Tidy your screen</PageTitle>
        <p className="mx-auto mt-1.5 max-w-lg text-[var(--muted)]">
          Point at a layout to see where your windows would go. Click it to use it; Undo puts everything back.
        </p>
      </div>

      <div className="mt-5">
        <Stage preview={picture} mode={showing ? 'then' : 'now'} caption={caption(picture, showing)} />
      </div>
      {problem ? (
        <p role="alert" className="mt-2 text-center text-small text-[var(--danger)]">
          {problem}
        </p>
      ) : null}

      <div className="mt-4 flex flex-wrap items-center justify-center gap-2">
        <span {...previewOn('auto')} className="contents">
          <Button
            tone="primary"
            pending={busy === 'auto'}
            disabled={busy !== null && busy !== 'auto'}
            onClick={() => {
              arrange('auto');
            }}
          >
            Arrange windows
          </Button>
        </span>
        {undoable ? <Button onClick={undo}>Undo</Button> : null}
      </div>
      <p className="mt-1.5 text-center text-small text-[var(--muted)]">
        {hasWindows || base === null
          ? 'Mosaïk picks the zone that suits each window, and keeps the result as a workspace.'
          : 'Open the apps you work with, then come back here.'}
      </p>

      <section aria-labelledby="layouts-title" className="mt-8">
        <h2 id="layouts-title" className="font-display text-heading font-semibold tracking-tight">
          Or pick a layout for your main screen
        </h2>
        <ul className="mt-3 grid grid-cols-[repeat(auto-fill,minmax(7.25rem,1fr))] gap-2.5">
          {LAYOUTS.map((p) => (
            <li key={p.id}>
              <button
                type="button"
                {...previewOn(p.id)}
                onClick={() => {
                  arrange(p.id);
                }}
                disabled={busy !== null}
                aria-busy={busy === p.id || undefined}
                className={`group flex w-full flex-col gap-1.5 rounded-xl border p-2 text-left transition-colors duration-150 disabled:opacity-60 ${
                  showing === p.id
                    ? 'border-[var(--accent)] bg-[var(--hover)]'
                    : 'border-[var(--line)] bg-[var(--surface)] hover:border-[var(--control)]'
                }`}
              >
                <Tile preset={p.id} />
                <span className="text-small leading-tight font-semibold text-balance">{p.label}</span>
              </button>
            </li>
          ))}
        </ul>
      </section>

      <section aria-labelledby="home-workspaces-title" className="mt-8">
        <div className="flex items-baseline justify-between gap-4">
          <h2 id="home-workspaces-title" className="font-display text-heading font-semibold tracking-tight">
            Your workspaces
          </h2>
          {list?.length ? (
            <Button tone="quiet" onClick={onManage}>
              Manage workspaces
            </Button>
          ) : null}
        </div>
        {list === null ? null : list.length === 0 ? (
          <p className="mt-1 text-small text-[var(--muted)]">
            When you arrange your windows, Mosaïk keeps them as a workspace you can come back to in one click.
          </p>
        ) : (
          <ul className="mt-2 flex flex-col divide-y divide-[var(--line)] rounded-xl border border-[var(--line)] bg-[var(--surface)]">
            {list.slice(0, 6).map((w) => (
              <li key={w.id} className="flex items-center justify-between gap-3 px-4 py-2">
                <span className="flex min-w-0 items-center gap-2">
                  <span className="truncate font-semibold">{w.name}</span>
                  {active === w.id ? (
                    <span className="shrink-0 rounded-full border border-[var(--ok)] px-2 text-caption leading-5 font-semibold text-[var(--ok)]">
                      Open now
                    </span>
                  ) : null}
                </span>
                <Button
                  label={`Open “${w.name}”`}
                  pending={opening === w.id}
                  onClick={() => {
                    open(w);
                  }}
                >
                  Open
                </Button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <p className="mt-6 text-small text-[var(--muted)]">
        From anywhere, <Keys shortcut={shortcut} /> switches workspaces
        {edge.enabled ? `, and the thin line at the ${edge.side} edge of your screen opens the quick menu.` : '.'}
      </p>
    </article>
  );
}
