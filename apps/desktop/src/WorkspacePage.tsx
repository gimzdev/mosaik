// One workspace, to change: its name, its shortcut, its apps (each in the zone it goes in), its
// zones, or to delete it. Opened from its card's ⋯ menu on Home. At the top, the picture of the
// main screen as opening it would leave it, as on Home (the owner: "show visually the zones same
// as homepage for that workspace specifically with arrangements aswell"), where its apps are
// dragged to other zones and its zones resized (ADR 0031), saved at once.

import { type SyntheticEvent, useId, useState } from 'react';

import { AppsList } from './AppsList';
import { request } from './engine';
import { Glyph } from './icons';
import { type ArrangePreview, MAX_NAME, type WorkspaceApps, type WorkspaceSummary, errorMessage } from './protocol';
import { useReads } from './reads';
import { type PictureEdits, Screen, dropped, stretched } from './Screen';
import { ShortcutRecorder } from './shortcut';
import { Button, Confirm, OnScreen, PageTitle, Section, Status, useAnnounce } from './ui';
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
        <AppsList
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
