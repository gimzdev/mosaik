// Mosaïk's window: welcome on first run, then workspaces, settings and about, with the zone
// editor for one workspace. Messages from Mosaïk show on top of every page.

import { useCallback, useEffect, useRef, useState } from 'react';

import { About } from './About';
import { guardClose, onNotices, request, styleTitleBar, takeNotices } from './engine';
import { keepTabInside } from './focusLoop';
import { Notices, type ShownNotice } from './Notices';
import { errorMessage, type HotkeyState, type Settings } from './protocol';
import { SettingsPane } from './SettingsPane';
import { Button, Confirm, Feedback, Mark } from './ui';
import { Welcome } from './Welcome';
import { Workspaces } from './Workspaces';
import { ZoneEditor } from './ZoneEditor';

type View =
  | { page: 'welcome' }
  | { page: 'workspaces' }
  | { page: 'settings' }
  | { page: 'about' }
  | { page: 'zones'; workspace: string };

const PAGES = [
  { page: 'workspaces', label: 'Workspaces' },
  { page: 'settings', label: 'Settings' },
  { page: 'about', label: 'About' },
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
  /** Where the user asked to go while the zone editor has unsaved changes. */
  const [leavingTo, setLeavingTo] = useState<View | null>(null);
  /** The window is closing with unsaved zone edits: resolves the question. */
  const [closing, setClosing] = useState<((close: boolean) => void) | null>(null);
  const noticeCount = useRef(0);

  const load = useCallback(() => {
    void request({ cmd: 'settings' }).then((reply) => {
      if (reply.reply === 'settings') {
        setSettings(reply.settings);
        setHotkey(reply.hotkey);
        setView((v) => v ?? (reply.settings.onboarding_done ? { page: 'workspaces' } : { page: 'welcome' }));
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
                setView({ page: 'workspaces' });
              }}
            />
          );
        case 'workspaces':
          return (
            <Workspaces
              shortcut={shortcut}
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
                setView({ page: 'workspaces' });
              }}
            />
          );
        case 'settings':
          return <SettingsPane settings={settings} hotkey={hotkey} saveSettings={saveSettings} />;
        case 'about':
          return (
            <About
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

    const current = view.page === 'zones' ? 'workspaces' : view.page;
    return (
      <div className="flex h-full">
        <nav
          aria-label="Mosaïk"
          className="flex w-52 shrink-0 flex-col gap-1 border-r border-[var(--line)] bg-[var(--surface)] p-4"
        >
          <p className="mb-5 flex items-center gap-2 px-2 pt-1 font-display text-xl font-bold tracking-tight">
            <Mark />
            Mosaïk
          </p>
          {PAGES.map((p) => (
            <button
              key={p.page}
              type="button"
              aria-current={current === p.page ? 'page' : undefined}
              onClick={() => {
                go({ page: p.page });
              }}
              className={`relative rounded-lg px-3 py-2 text-left font-semibold transition-colors duration-150 ${
                current === p.page
                  ? 'bg-[var(--hover)] text-[var(--text)] before:absolute before:top-2 before:bottom-2 before:left-0 before:w-[3px] before:rounded-full before:bg-[var(--accent)] forced-colors:underline forced-colors:underline-offset-4'
                  : 'text-[var(--muted)] hover:bg-[var(--hover)] hover:text-[var(--text)]'
              }`}
            >
              {p.label}
            </button>
          ))}
        </nav>
        <main className="relative min-w-0 flex-1 overflow-y-auto px-10 py-8">
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
