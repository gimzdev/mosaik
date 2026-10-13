// Mosaïk's window: a bar with its two places, Home (the screen, Arrange, the layouts, the
// workspaces) and Settings (with About at the end), and Mosaïk's notes; under it, the page. A
// workspace's own page and the zone editor open from Home. The first time Mosaïk opens, it sorts
// the windows open into workspaces: there is no tour to sit through, Home shows what Arrange will
// do.

import { useCallback, useEffect, useRef, useState } from 'react';

import {
  guardClose,
  onNotices,
  onZonesToEdit,
  request,
  showSwitcher,
  styleTitleBar,
  takeNotices,
  takeZonesToEdit,
} from './engine';
import { keepTabInside } from './focusLoop';
import { Home } from './Home';
import { type Note, Notes } from './Notes';
import { type HotkeyState, type Settings, type SideMenuState, errorMessage } from './protocol';
import { SettingsPage } from './SettingsPage';
import { Announcer, Button, Confirm, Keys, Mark } from './ui';
import { WorkspacePage } from './WorkspacePage';
import { ZoneEditor } from './ZoneEditor';

type View =
  | { page: 'home' }
  | { page: 'settings' }
  | { page: 'workspace'; workspace: string }
  | { page: 'zones'; workspace: string; from: 'home' | 'workspace' };

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
        caption: css.getPropertyValue('--panel').trim(),
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
  const [sideMenu, setSideMenu] = useState<SideMenuState | null>(null);
  const [readOnly, setReadOnly] = useState(false);
  /** Installed from the Store, rather than a build run from a folder. */
  const [packaged, setPackaged] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [view, setView] = useState<View>({ page: 'home' });
  const [notes, setNotes] = useState<Note[]>([]);
  const [editorDirty, setEditorDirty] = useState(false);
  /** The first run's work is under way (once per window). */
  const firstRunStarted = useRef(false);
  /** Where the user asked to go while the zone editor has unsaved changes. */
  const [leavingTo, setLeavingTo] = useState<View | null>(null);
  /** The window is closing with unsaved zone edits: resolves the question. */
  const [closing, setClosing] = useState<((close: boolean) => void) | null>(null);
  const noteCount = useRef(0);
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
        setSideMenu(reply.side_menu);
        setReadOnly(reply.read_only);
        setPackaged(reply.packaged);
      } else {
        setProblem(errorMessage(reply) ?? 'Mosaïk did not answer.');
      }
    });
  }, []);

  useEffect(() => {
    load();
    // Notes wait in the app until a window takes them: now, and whenever more arrive.
    const collect = () => {
      void takeNotices().then((fresh) => {
        if (fresh.length === 0) return;
        const shown = fresh.map((n) => {
          noteCount.current += 1;
          return { ...n, id: noteCount.current };
        });
        setNotes((n) => [...shown.reverse(), ...n]);
      });
    };
    collect();
    return onNotices(collect);
  }, [load]);

  // The side menu's "Edit zones" opens the editor for the open workspace (asking first when the
  // editor has unsaved changes).
  useEffect(() => {
    const take = () => {
      void takeZonesToEdit().then((workspace) => {
        if (!workspace) return;
        const next: View = { page: 'zones', workspace, from: 'home' };
        if (dirty.current) setLeavingTo(next);
        else setView(next);
      });
    };
    take();
    return onZonesToEdit(take);
  }, []);

  // The theme the user picked: dark unless they chose otherwise; "Like Windows" follows Windows
  // (and its high-contrast themes).
  const theme = settings?.theme;
  useEffect(() => {
    document.documentElement.dataset.theme = theme ?? 'dark';
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
      setSideMenu(fresh.side_menu);
      setReadOnly(fresh.read_only);
    }
    return reply.reply === 'done'
      ? { ok: true, message: reply.message, settings: now }
      : {
          ok: false,
          message: errorMessage(reply) ?? 'Your settings could not be saved.',
          settings: now,
        };
  }, []);

  // The first run: the windows open now are sorted into a workspace per task (none moves), and
  // Mosaïk as installed starts with Windows from now on (Settings turns that off; a build run
  // from a folder is left as it is).
  useEffect(() => {
    if (!settings || settings.onboarding_done || readOnly || firstRunStarted.current) return;
    firstRunStarted.current = true;
    void (async () => {
      await request({ cmd: 'sort_windows' });
      await saveSettings({
        ...settings,
        onboarding_done: true,
        start_with_windows: packaged || settings.start_with_windows,
      });
    })();
  }, [settings, readOnly, packaged, saveSettings]);

  // A page opens at its top, not where the last one was scrolled to.
  const page = view.page;
  useEffect(() => {
    scroller.current?.scrollTo({ top: 0 });
  }, [page]);

  const go = (next: View) => {
    if (view.page === 'zones' && editorDirty) setLeavingTo(next);
    else setView(next);
  };

  return <Announcer>{body()}</Announcer>;

  /** The window's content for where the user is. */
  function body() {
    if (!settings || !hotkey) {
      return (
        <main className="grid h-full place-items-center p-10">
          <div className="flex max-w-md flex-col items-center gap-4 text-center">
            <Mark size={36} />
            {problem ? (
              <>
                <h1 className="font-display text-title font-bold">Mosaïk isn’t answering</h1>
                <p role="alert" className="text-[var(--muted)]">
                  {problem}
                </p>
                <Button
                  tone="primary"
                  onClick={() => {
                    setProblem(null);
                    load();
                  }}
                >
                  Try again
                </Button>
              </>
            ) : (
              <p className="text-[var(--muted)]" aria-busy="true">
                Starting…
              </p>
            )}
          </div>
        </main>
      );
    }

    const content = (() => {
      switch (view.page) {
        case 'home':
          return (
            <Home
              onEditWorkspace={(workspace) => {
                setView({ page: 'workspace', workspace });
              }}
              onEditZones={(workspace) => {
                setView({ page: 'zones', workspace, from: 'home' });
              }}
            />
          );
        case 'workspace':
          return (
            <WorkspacePage
              workspace={view.workspace}
              onBack={() => {
                setView({ page: 'home' });
              }}
              onEditZones={(workspace) => {
                setView({ page: 'zones', workspace, from: 'workspace' });
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
                setView(
                  view.from === 'workspace' ? { page: 'workspace', workspace: view.workspace } : { page: 'home' },
                );
              }}
            />
          );
        case 'settings':
          return (
            <SettingsPage
              settings={settings}
              hotkey={hotkey}
              sideMenu={sideMenu}
              readOnly={readOnly}
              saveSettings={saveSettings}
            />
          );
      }
    })();

    // A workspace's page and the zone editor belong to Home.
    const current = view.page === 'settings' ? 'settings' : 'home';
    return (
      <div className="flex h-full flex-col">
        <header className="flex h-12 shrink-0 items-center gap-6 border-b border-[var(--line)] bg-[var(--panel)] px-5">
          <p className="flex items-center gap-2 font-display text-[1.0625rem] font-bold tracking-tight">
            <Mark size={20} />
            Mosaïk
          </p>
          <nav aria-label="Mosaïk" className="flex items-center gap-1">
            {PAGES.map((p) => (
              <button
                key={p.page}
                type="button"
                aria-current={current === p.page ? 'page' : undefined}
                onClick={() => {
                  go({ page: p.page });
                }}
                className={`rounded-lg px-3 py-1.5 text-small font-semibold transition-colors duration-150 ${
                  current === p.page
                    ? 'bg-[var(--raised)] text-[var(--text)] forced-colors:underline forced-colors:underline-offset-4'
                    : 'text-[var(--muted)] hover:text-[var(--text)]'
                }`}
              >
                {p.label}
              </button>
            ))}
          </nav>
          <div className="ml-auto flex items-center gap-2">
            <Notes
              notes={notes}
              onDismiss={(id) => {
                setNotes((n) => n.filter((x) => x.id !== id));
              }}
              onClear={() => {
                setNotes([]);
              }}
            />
            <button
              type="button"
              onClick={showSwitcher}
              aria-label={`Open the switcher (${hotkey.shortcut.split('+').join(' plus ')})`}
              title="Open the switcher"
              className="inline-flex min-h-8 items-center gap-2 rounded-lg px-2 text-small font-semibold text-[var(--muted)] hover:bg-[var(--hover)] hover:text-[var(--text)]"
            >
              Switcher
              <Keys shortcut={hotkey.shortcut} quiet />
            </button>
          </div>
        </header>
        <main ref={scroller} className="relative min-h-0 flex-1 overflow-y-auto px-5 py-5">
          <div className="mx-auto max-w-[68rem]">{content}</div>
        </main>
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
      </div>
    );
  }
}
