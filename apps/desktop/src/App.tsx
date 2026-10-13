// Mosaïk's window: welcome on first run, then two places: Home (tidy the screen in one click,
// sets of apps, the saved workspaces) and Settings (with About at the end), and the zone editor
// for one workspace, opened from its workspace. Messages from Mosaïk show on top of every page.

import { useCallback, useEffect, useRef, useState } from 'react';

import { guardClose, onNotices, onZonesToEdit, request, styleTitleBar, takeNotices, takeZonesToEdit } from './engine';
import { keepTabInside } from './focusLoop';
import { Notices, type ShownNotice } from './Notices';
import { Home } from './Home';
import { errorMessage, type HotkeyState, type Settings } from './protocol';
import { SettingsPane } from './SettingsPane';
import { Button, Confirm, Feedback, Mark } from './ui';
import { Welcome } from './Welcome';
import { ZoneEditor } from './ZoneEditor';

type View = { page: 'welcome' } | { page: 'home' } | { page: 'settings' } | { page: 'zones'; workspace: string };

const PAGES = [
  { page: 'home', label: 'Home' },
  { page: 'settings', label: 'Settings' },
] as const;

/** What saving settings came to: the engine's message, and the settings as they now are (the
 * engine may have adjusted one, such as starting with Windows turned off in Task Manager). */
export interface Saved {
  ok: boolean;
  message: string;
  settings: Settings;
}
export type SaveSettings = (next: Settings) => Promise<Saved>;

/** The page's colours on the window's title bar, following the theme and high contrast. */
function useTitleBar(theme: Settings['theme'] | undefined) {
  useEffect(() => {
    const queries = [matchMedia('(prefers-color-scheme: dark)'), matchMedia('(forced-colors: active)')];
    const apply = () => {
      if (queries[1]?.matches) {
        styleTitleBar(null);
        return;
      }
      const css = getComputedStyle(document.documentElement);
      styleTitleBar({
        dark: css.colorScheme === 'dark',
        caption: css.getPropertyValue('--surface').trim(),
        text: css.getPropertyValue('--text').trim(),
      });
    };
    apply();
    for (const q of queries) q.addEventListener('change', apply);
    return () => {
      for (const q of queries) q.removeEventListener('change', apply);
    };
  }, [theme]);
}

export function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [hotkey, setHotkey] = useState<HotkeyState | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [view, setView] = useState<View | null>(null);
  const [notices, setNotices] = useState<ShownNotice[]>([]);
  const [editorDirty, setEditorDirty] = useState(false);
  /** The welcome was just finished: the home page shows once what Arrange does. */
  const [welcomed, setWelcomed] = useState(false);
  /** Where the user asked to go while the zone editor has unsaved changes. */
  const [leavingTo, setLeavingTo] = useState<View | null>(null);
  /** The window is closing with unsaved zone edits: resolves the question. */
  const [closing, setClosing] = useState<((close: boolean) => void) | null>(null);
  const noticeCount = useRef(0);
  const dirty = useRef(false);
  const scroller = useRef<HTMLElement>(null);
  useEffect(() => {
    dirty.current = editorDirty;
  }, [editorDirty]);

  const load = useCallback(() => {
    void request({ cmd: 'settings' }).then((reply) => {
      if (reply.reply === 'settings') {
        setSettings(reply.settings);
        setHotkey(reply.hotkey);
        setView((v) => v ?? (reply.settings.onboarding_done ? { page: 'home' } : { page: 'welcome' }));
      } else {
        setProblem(errorMessage(reply) ?? 'Mosaïk did not answer.');
      }
    });
  }, []);

  useEffect(() => {
    load();
    // Notices wait in the app until a window takes them: now, and whenever more arrive.
    const collect = () => {
      void takeNotices().then((fresh) => {
        if (fresh.length === 0) return;
        const shown = fresh.map((n) => {
          noticeCount.current += 1;
          return { ...n, id: noticeCount.current };
        });
        setNotices((n) => [...n, ...shown]);
      });
    };
    collect();
    return onNotices(collect);
  }, [load]);

  // The quick menu's "Edit zones" opens the editor for the open workspace (asking first when
  // the editor has unsaved changes).
  useEffect(() => {
    const take = () => {
      void takeZonesToEdit().then((workspace) => {
        if (!workspace) return;
        const next: View = { page: 'zones', workspace };
        if (dirty.current) setLeavingTo(next);
        else setView(next);
      });
    };
    take();
    return onZonesToEdit(take);
  }, []);

  // The theme the user picked; "system" follows Windows (and its high-contrast themes).
  const theme = settings?.theme;
  useEffect(() => {
    const root = document.documentElement;
    if (!theme || theme === 'system') delete root.dataset.theme;
    else root.dataset.theme = theme;
  }, [theme]);
  useTitleBar(theme);

  // The Tab key goes round the window's controls and never leaves it (focusLoop.ts).
  useEffect(() => {
    document.addEventListener('keydown', keepTabInside);
    return () => {
      document.removeEventListener('keydown', keepTabInside);
    };
  }, []);

  // Closing the window with unsaved zone edits asks first.
  useEffect(() => {
    if (!editorDirty) return;
    return guardClose(
      () =>
        new Promise<boolean>((resolve) => {
          setClosing(() => resolve);
        }),
    );
  }, [editorDirty]);

  // Shown at once, saved, then read back: the engine may have adjusted something, or refused
  // the change, which puts the old value back.
  const saveSettings: SaveSettings = useCallback(async (next) => {
    setSettings(next);
    const reply = await request({ cmd: 'set_settings', settings: next });
    const fresh = await request({ cmd: 'settings' });
    const now = fresh.reply === 'settings' ? fresh.settings : next;
    if (fresh.reply === 'settings') {
      setSettings(fresh.settings);
      setHotkey(fresh.hotkey);
    }
    return reply.reply === 'done'
      ? { ok: true, message: reply.message, settings: now }
      : {
          ok: false,
          message: errorMessage(reply) ?? 'Your settings could not be saved.',
          settings: now,
        };
  }, []);

  // A page opens at its top, not where the last one was scrolled to.
  const page = view?.page;
  useEffect(() => {
    scroller.current?.scrollTo({ top: 0 });
  }, [page]);

  const go = (next: View) => {
    if (view?.page === 'zones' && editorDirty) setLeavingTo(next);
    else setView(next);
  };

  const notes = (
    <Notices
      notices={notices}
      onDismiss={(id) => {
        setNotices((n) => n.filter((x) => x.id !== id));
      }}
    />
  );

  return <Feedback>{body()}</Feedback>;

  /** The window's content for where the user is. */
  function body() {
    if (!settings || !hotkey || !view) {
      return (
        <main className="mx-auto max-w-xl p-10">
          <h1 className="flex items-center gap-2 font-display text-title font-bold">
            <Mark size={26} />
            Mosaïk
          </h1>
          {problem ? (
            <div className="mt-4">
              <p role="alert" className="text-[var(--danger)]">
                {problem}
              </p>
              <div className="mt-4">
                <Button
                  tone="primary"
                  onClick={() => {
                    setProblem(null);
                    load();
                  }}
                >
                  Try again
                </Button>
              </div>
            </div>
          ) : (
            <p className="mt-4 text-[var(--muted)]" aria-busy="true">
              Starting…
            </p>
          )}
          {notes}
        </main>
      );
    }

    const shortcut = hotkey.shortcut;
    const content = (() => {
      switch (view.page) {
        case 'welcome':
          return (
            <Welcome
              settings={settings}
              shortcut={shortcut}
              saveSettings={saveSettings}
              onDone={() => {
                setWelcomed(true);
                setView({ page: 'home' });
              }}
            />
          );
        case 'home':
          return (
            <Home
              shortcut={shortcut}
              demo={welcomed}
              onEditZones={(workspace) => {
                setView({ page: 'zones', workspace });
              }}
            />
          );
        case 'zones':
          return (
            <ZoneEditor
              workspace={view.workspace}
              onDirtyChange={setEditorDirty}
              onClose={() => {
                setEditorDirty(false);
                setView({ page: 'home' });
              }}
            />
          );
        case 'settings':
          return (
            <SettingsPane
              settings={settings}
              hotkey={hotkey}
              saveSettings={saveSettings}
              onWelcome={() => {
                setView({ page: 'welcome' });
              }}
            />
          );
      }
    })();

    const dialogs = (
      <>
        <Confirm
          open={leavingTo !== null}
          title="Leave without saving?"
          body="The changes to these zones will be lost."
          confirm="Discard changes"
          onCancel={() => {
            setLeavingTo(null);
          }}
          onConfirm={() => {
            const next = leavingTo;
            setLeavingTo(null);
            setEditorDirty(false);
            if (next) setView(next);
          }}
        />
        <Confirm
          open={closing !== null}
          title="Close without saving?"
          body="The changes to these zones will be lost."
          confirm="Close anyway"
          onCancel={() => {
            closing?.(false);
            setClosing(null);
          }}
          onConfirm={() => {
            closing?.(true);
            setClosing(null);
          }}
        />
      </>
    );

    // The welcome has the whole window: one thing to read, one thing to do, in the middle of the
    // window when it has room (auto margins never push it off the top when it does not).
    if (view.page === 'welcome') {
      return (
        <main className="relative flex h-full flex-col overflow-y-auto px-10 py-10">
          <div className="m-auto w-full max-w-3xl">
            {notes}
            <div className={notices.length ? 'mt-6' : ''}>{content}</div>
          </div>
        </main>
      );
    }

    // The zone editor is opened from a workspace on the home page.
    const current = view.page === 'zones' ? 'home' : view.page;
    return (
      <div className="flex h-full flex-col">
        <header className="flex shrink-0 items-stretch gap-8 border-b border-[var(--line)] bg-[var(--surface)] px-6">
          <p className="flex items-center gap-2 font-display text-lg font-bold tracking-tight">
            <Mark size={20} />
            Mosaïk
          </p>
          <nav aria-label="Mosaïk" className="flex items-stretch gap-1">
            {PAGES.map((p) => (
              <button
                key={p.page}
                type="button"
                aria-current={current === p.page ? 'page' : undefined}
                onClick={() => {
                  go({ page: p.page });
                }}
                className={`relative px-3 py-3 font-semibold transition-colors duration-150 focus-visible:-outline-offset-2 ${
                  current === p.page
                    ? 'text-[var(--text)] after:absolute after:inset-x-3 after:bottom-0 after:h-[3px] after:rounded-t-full after:bg-[var(--accent)] forced-colors:underline forced-colors:underline-offset-4'
                    : 'text-[var(--muted)] hover:text-[var(--text)]'
                }`}
              >
                {p.label}
              </button>
            ))}
          </nav>
        </header>
        <main ref={scroller} className="relative min-h-0 flex-1 overflow-y-auto px-10 py-8">
          <div className="mx-auto max-w-3xl">
            {notes}
            <div className={notices.length ? 'mt-6' : ''}>{content}</div>
          </div>
        </main>
        {dialogs}
      </div>
    );
  }
}
