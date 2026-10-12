// Mosaïk's window: welcome on first run, then workspaces, settings and about, with the zone
// editor for one workspace. Messages from Mosaïk show on top of every page.

import { useCallback, useEffect, useState } from 'react';

import { About } from './About';
import { onNotices, request, takeNotices } from './engine';
import { errorMessage, type Notice, type Settings } from './protocol';
import { SettingsPane } from './SettingsPane';
import { Notices } from './Notices';
import { Welcome } from './Welcome';
import { Workspaces } from './Workspaces';
import { ZoneEditor } from './ZoneEditor';

export type View = { page: 'welcome' } | { page: 'workspaces' } | { page: 'settings' } | { page: 'about' } | { page: 'zones'; workspace: string };

const PAGES = [
  { page: 'workspaces', label: 'Workspaces' },
  { page: 'settings', label: 'Settings' },
  { page: 'about', label: 'About' },
] as const;

/** Saves settings; resolves to an error message, or null when saved. */
export type SaveSettings = (next: Settings) => Promise<string | null>;

export function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [view, setView] = useState<View | null>(null);
  const [notices, setNotices] = useState<Notice[]>([]);

  useEffect(() => {
    void request({ cmd: 'settings' }).then((reply) => {
      if (reply.reply === 'settings') {
        setSettings(reply.settings);
        setView(reply.settings.onboarding_done ? { page: 'workspaces' } : { page: 'welcome' });
      } else {
        setProblem(errorMessage(reply) ?? 'Mosaïk did not answer.');
      }
    });
    // Notices wait in the app until a window takes them: now, and whenever more arrive.
    const collect = () => {
      void takeNotices().then((fresh) => {
        if (fresh.length) setNotices((n) => [...n, ...fresh]);
      });
    };
    collect();
    return onNotices(collect);
  }, []);

  // The theme the user picked; "system" follows Windows (and its high-contrast themes).
  useEffect(() => {
    const root = document.documentElement;
    if (!settings || settings.theme === 'system') delete root.dataset.theme;
    else root.dataset.theme = settings.theme;
  }, [settings]);

  // Shown at once, saved, then read back: the engine may have adjusted something (starting with
  // Windows turned off in Task Manager), or refused the change, which puts the old value back.
  const saveSettings: SaveSettings = useCallback(async (next) => {
    setSettings(next);
    const reply = await request({ cmd: 'set_settings', settings: next });
    const fresh = await request({ cmd: 'settings' });
    if (fresh.reply === 'settings') setSettings(fresh.settings);
    return reply.reply === 'done' ? null : (errorMessage(reply) ?? 'Your settings could not be saved.');
  }, []);

  if (problem) {
    return (
      <main className="mx-auto max-w-xl p-10">
        <h1 className="font-display text-2xl font-bold">Mosaïk</h1>
        <p role="alert" className="mt-3 text-[var(--danger)]">
          {problem}
        </p>
      </main>
    );
  }
  if (!settings || !view) {
    return (
      <main className="p-10 text-[var(--muted)]" aria-busy="true">
        Starting…
      </main>
    );
  }

  const content = (() => {
    switch (view.page) {
      case 'welcome':
        return (
          <Welcome
            settings={settings}
            saveSettings={saveSettings}
            onDone={() => {
              setView({ page: 'workspaces' });
            }}
          />
        );
      case 'workspaces':
        return (
          <Workspaces
            switcherShortcut={settings.switcher_hotkey}
            onEditZones={(workspace) => {
              setView({ page: 'zones', workspace });
            }}
          />
        );
      case 'zones':
        return (
          <ZoneEditor
            workspace={view.workspace}
            onClose={() => {
              setView({ page: 'workspaces' });
            }}
          />
        );
      case 'settings':
        return <SettingsPane settings={settings} saveSettings={saveSettings} />;
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

  const current = view.page === 'zones' ? 'workspaces' : view.page;
  return (
    <div className="flex h-full">
      <nav aria-label="Mosaïk" className="flex w-52 shrink-0 flex-col gap-1 border-r border-[var(--line)] bg-[var(--surface)] p-4">
        <p className="mb-4 px-2 font-display text-xl font-bold tracking-tight">Mosaïk</p>
        {PAGES.map((p) => (
          <button
            key={p.page}
            type="button"
            aria-current={current === p.page ? 'page' : undefined}
            onClick={() => {
              setView({ page: p.page });
            }}
            className={`rounded-lg px-3 py-2 text-left font-semibold transition-colors duration-150 ${
              current === p.page ? 'bg-[var(--hover)] text-[var(--text)]' : 'text-[var(--muted)] hover:bg-[var(--hover)]'
            }`}
          >
            {p.label}
          </button>
        ))}
      </nav>
      <main className="min-w-0 flex-1 overflow-y-auto px-10 py-8">
        <div className="mx-auto max-w-3xl">
          <Notices
            notices={notices}
            onDismiss={(index) => {
              setNotices((n) => n.filter((_, i) => i !== index));
            }}
          />
          <div className={notices.length ? 'mt-6' : ''}>{content}</div>
        </div>
      </main>
    </div>
  );
}
