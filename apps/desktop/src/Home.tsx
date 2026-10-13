// Home: the main screen as the chosen layout would leave it, with Arrange beside it, and the
// workspaces in a column of their own. The smart layout is chosen first (Arrange reads what the
// windows are for); any of the ten layouts can be chosen instead, and the picture shows it before
// anything moves. Arrange does it; Undo puts everything back.

import { useCallback, useEffect, useId, useState } from 'react';

import { PRESETS, type Preset, presetShape } from './editor/model';
import { onWorkspacesChanged, request } from './engine';
import { Glyph } from './icons';
import { LayoutGlyph } from './Layouts';
import { type ArrangePreview, TASK_WORDS, errorMessage } from './protocol';
import { Screen, settled } from './Screen';
import { Button, PageTitle, Status, useAnnounce } from './ui';
import { Workspaces } from './Workspaces';

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

/** What Arrange will do: lay the windows out by itself (`smart`), or in a layout. */
type Choice = Preset | 'smart';

/** "Slack", "Slack and Teams", "Slack, Teams and Mail". */
function names(list: string[]): string {
  if (list.length <= 1) return list.join('');
  return `${list.slice(0, -1).join(', ')} and ${list[list.length - 1] ?? ''}`;
}

/** What the picture shows, in a sentence under it: what will be minimised, or that nothing moves. */
function note(preview: ArrangePreview | null, choice: Choice): string | null {
  if (!preview || preview.windows.length === 0) return null;
  if (settled(preview)) return 'Your windows are already like this.';
  const gone = preview.windows.filter((w) => w.minimized).map((w) => w.label);
  const parts: string[] = [];
  if (gone.length) parts.push(`${names(gone)} will be minimized.`);
  const elsewhere = preview.elsewhere;
  if (choice === 'smart' && elsewhere) {
    parts.push(
      elsewhere === 1
        ? 'The window on your other screen is arranged there too.'
        : `The ${String(elsewhere)} windows on your other screens are arranged there too.`,
    );
  }
  return parts.length ? parts.join(' ') : null;
}

export function Home({
  onEditWorkspace,
  onEditZones,
}: {
  onEditWorkspace: (workspace: string) => void;
  onEditZones: (workspace: string) => void;
}) {
  const announce = useAnnounce();
  const group = useId();
  const [choice, setChoice] = useState<Choice>('smart');
  /** The smart layout: what Arrange would do by itself (and whether anything is open). */
  const [smart, setSmart] = useState<ArrangePreview | null>(null);
  /** The layout chosen, once Mosaïk has said where it would put the windows. */
  const [chosen, setChosen] = useState<{ choice: Preset; preview: ArrangePreview } | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  /** What the last Arrange or Undo did, said under the picture. */
  const [outcome, setOutcome] = useState<{ text: string; error: boolean } | null>(null);
  const [undoable, setUndoable] = useState(false);
  /** Counts the times the screen is read again: windows moved, a workspace opened… */
  const [reads, setReads] = useState(0);
  const readAgain = useCallback(() => {
    setReads((n) => n + 1);
  }, []);

  // Windows move while the page is open: the picture follows when the window comes back, and
  // when a workspace or a set opens.
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
    void request({ cmd: 'preview_arrange' }).then((reply) => {
      if (!live) return;
      if (reply.reply === 'preview') {
        setSmart(reply);
        setProblem(null);
      } else {
        setProblem(errorMessage(reply) ?? 'Mosaïk could not read your screen.');
      }
    });
    return () => {
      live = false;
    };
  }, [reads]);

  // The layout chosen is asked for; an answer for one chosen before is dropped.
  useEffect(() => {
    if (choice === 'smart') return;
    let live = true;
    void request({ cmd: 'preview_arrange', preset: choice }).then((reply) => {
      if (live && reply.reply === 'preview') setChosen({ choice, preview: reply });
    });
    return () => {
      live = false;
    };
  }, [choice, reads]);

  const arrange = () => {
    setBusy(true);
    setOutcome(null);
    void request(choice === 'smart' ? { cmd: 'arrange' } : { cmd: 'arrange', preset: choice }).then((reply) => {
      setBusy(false);
      if (reply.reply === 'done') {
        setOutcome({ text: reply.message, error: false });
        setUndoable(true);
        announce(reply.message);
      } else {
        setOutcome({ text: errorMessage(reply) ?? 'Your windows could not be arranged.', error: true });
      }
      readAgain();
    });
  };

  const undo = () => {
    setUndoable(false);
    void request({ cmd: 'undo_arrange' }).then((reply) => {
      if (reply.reply === 'done') {
        setOutcome({ text: reply.message, error: false });
        announce(reply.message);
      } else {
        setOutcome({ text: errorMessage(reply) ?? 'Arrange could not be undone.', error: true });
      }
      readAgain();
    });
  };

  // While a layout's answer is on its way, the last picture stays.
  const picture =
    choice === 'smart' ? smart : chosen?.choice === choice ? chosen.preview : (chosen?.preview ?? smart);
  const hasWindows = (smart?.windows.length ?? 0) > 0;
  const words = smart?.task ? TASK_WORDS[smart.task] : null;
  const title =
    choice === 'smart'
      ? words
        ? `Smart layout for ${words}`
        : 'Smart layout'
      : (PRESETS.find((p) => p.id === choice)?.label ?? 'Layout');
  const said = note(picture, choice);

  return (
    <article aria-labelledby="home-title">
      <PageTitle id="home-title" className="sr-only">
        Home
      </PageTitle>
      <div className="grid gap-x-8 gap-y-8 min-[880px]:grid-cols-[minmax(0,1fr)_19rem]">
        <section aria-labelledby="arrange-title" className="flex min-w-0 flex-col">
          <div className="flex flex-wrap items-end justify-between gap-x-4 gap-y-2">
            <div className="min-w-0">
              <h2 id="arrange-title" className="font-display text-title font-bold tracking-tight">
                Your screen
              </h2>
              <p className="mt-0.5 truncate text-small text-[var(--muted)]">
                {problem ? 'Your screen could not be read' : hasWindows || !smart ? title : 'Open some apps first'}
              </p>
            </div>
            <Button tone="primary" size="lg" pending={busy} disabled={!hasWindows || busy} onClick={arrange}>
              {busy ? null : <Glyph name="spark" />}
              Arrange
            </Button>
          </div>

          <div className="mt-4">
            <Screen preview={picture} unread={problem !== null} />
          </div>

          {/* The layouts, drawn: the one chosen is named; the others say their name when pointed at,
              and to screen readers. */}
          <fieldset className="mt-3 min-w-0">
            <legend className="sr-only">Layout</legend>
            <div className="flex flex-wrap items-center gap-1">
              {[{ id: 'smart' as const, label: 'Smart' }, ...LAYOUTS].map((option) => {
                const on = choice === option.id;
                return (
                  <label
                    key={option.id}
                    title={option.label}
                    className={`relative inline-flex h-9 items-center gap-2 rounded-full px-2.5 text-small font-semibold transition-colors duration-150 has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-[var(--focus)] has-[:disabled]:opacity-50 ${
                      on
                        ? 'bg-[var(--raised)] pr-3.5 text-[var(--text)] ring-1 ring-[var(--accent)] forced-colors:ring-[Highlight]'
                        : 'text-[var(--muted)] hover:bg-[var(--panel)] hover:text-[var(--text)]'
                    }`}
                  >
                    <input
                      type="radio"
                      name={group}
                      value={option.id}
                      checked={on}
                      disabled={!hasWindows}
                      onChange={() => {
                        setChoice(option.id);
                        setOutcome(null);
                      }}
                      // Over the whole chip, unseen: a click anywhere on it chooses it.
                      className="absolute inset-0 m-0 cursor-pointer appearance-none rounded-full opacity-0 disabled:cursor-default"
                    />
                    {option.id === 'smart' ? (
                      <Glyph name="spark" size={15} className={on ? 'text-[var(--accent)]' : ''} />
                    ) : (
                      <LayoutGlyph shape={presetShape(option.id)} width={24} height={15} colored={on} />
                    )}
                    <span className={on || option.id === 'smart' ? '' : 'sr-only'}>{option.label}</span>
                  </label>
                );
              })}
            </div>
          </fieldset>

          <div className="mt-2 flex min-h-8 flex-wrap items-center gap-x-3 gap-y-1">
            {problem ? <Status text={problem} error /> : null}
            {outcome ? (
              <Status text={outcome.text} error={outcome.error} />
            ) : said ? (
              <p className="text-small text-[var(--muted)]">{said}</p>
            ) : null}
            {undoable ? (
              <Button tone="quiet" size="sm" onClick={undo}>
                <Glyph name="undo" size={14} />
                Undo
              </Button>
            ) : null}
          </div>
        </section>

        <Workspaces
          onEdit={onEditWorkspace}
          onEditZones={onEditZones}
          onOpened={() => {
            // What Undo would undo is not on screen any more.
            setUndoable(false);
            setOutcome(null);
            readAgain();
          }}
        />
      </div>
    </article>
  );
}
