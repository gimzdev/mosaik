// Settings, in the order a beginner meets them: starting with Windows and the theme, the shortcut
// that switches workspace, how windows snap and look, the side menu; the rest under More
// settings, then About. Each change is saved at once; a change Mosaïk could not make is said at
// the top of the page.

import { useEffect, useId, useState } from 'react';

import type { SaveSettings } from './App';
import { type Link, openLicences, openLink, openLogs, request, rescueWindows } from './engine';
import { Glyph } from './icons';
import type { EdgeSide, HideMethod, HotkeyState, Settings, SideMenuState, Theme } from './protocol';
import { ShortcutRecorder } from './shortcut';
import { Button, PageTitle, Row, Section, Segmented, Status, Toggle, useAnnounce } from './ui';

const THEMES: readonly { value: Theme; label: string }[] = [
  { value: 'dark', label: 'Dark' },
  { value: 'light', label: 'Light' },
];

/** Where a window shows its zone's number, when it shows it. */
const TAG_PLACES: readonly { value: 'title' | 'above'; label: string }[] = [
  { value: 'title', label: 'Title bar' },
  { value: 'above', label: 'Above' },
];

type SideMenu = EdgeSide | 'off';
const SIDE_MENU: readonly { value: SideMenu; label: string }[] = [
  { value: 'right', label: 'Right' },
  { value: 'left', label: 'Left' },
  { value: 'off', label: 'Off' },
];

const SHIFT: readonly { value: 'free' | 'snap'; label: string }[] = [
  { value: 'free', label: 'Moves freely' },
  { value: 'snap', label: 'Snaps' },
];

const HIDING: readonly { value: 'auto' | 'minimize'; label: string }[] = [
  { value: 'auto', label: 'Hidden' },
  { value: 'minimize', label: 'Minimized' },
];

/** Where the side menu is, when it is not where it was asked for. */
function sideNote(state: SideMenuState | null): string | null {
  if (!state?.moved) return null;
  const chosen = state.side === 'left' ? 'right' : 'left';
  return `On the ${state.side} edge for now: the pointer doesn’t stop at the ${chosen} edge of your main screen, because another screen is there.`;
}

function About() {
  const announce = useAnnounce();
  const [version, setVersion] = useState<string | null>(null);
  const [rescuing, setRescuing] = useState(false);
  const [outcome, setOutcome] = useState<{ text: string; error: boolean } | null>(null);

  useEffect(() => {
    let live = true;
    void request({ cmd: 'hello', client: 'about' }).then((reply) => {
      if (live && reply.reply === 'hello') setVersion(reply.app_version);
    });
    return () => {
      live = false;
    };
  }, []);

  const opened = (error: string | null) => {
    if (error) setOutcome({ text: error, error: true });
  };
  const link = (l: Link) => () => {
    void openLink(l).then(opened);
  };

  return (
    <Section title="About">
      <div className="flex flex-wrap items-center justify-between gap-4 py-4">
        <div className="min-w-0 flex-1 basis-64">
          <p className="font-semibold">Mosaïk{version ? ` ${version}` : ''}</p>
          <p className="mt-0.5 text-small text-[var(--muted)]">
            Free and open source, in the public domain. No account, no tracking, no network requests: your layouts stay
            on this PC.
          </p>
        </div>
        <Button onClick={link('patreon')}>Support on Patreon</Button>
      </div>
      <div className="flex flex-wrap gap-1 py-3">
        <Button tone="quiet" size="sm" onClick={link('site')}>
          Website
        </Button>
        <Button tone="quiet" size="sm" onClick={link('source')}>
          Source code
        </Button>
        <Button tone="quiet" size="sm" onClick={link('privacy')}>
          Privacy
        </Button>
        <Button
          tone="quiet"
          size="sm"
          onClick={() => {
            void openLicences().then(opened);
          }}
        >
          Open-source licences
        </Button>
      </div>
      <Row label="Windows gone missing?" description="Rescue brings back every window Mosaïk hid.">
        <Button
          size="sm"
          pending={rescuing}
          onClick={() => {
            setRescuing(true);
            void rescueWindows().then(({ text, error }) => {
              setRescuing(false);
              setOutcome({ text, error });
              if (!error) announce(text);
            });
          }}
        >
          Rescue windows
        </Button>
        <Button
          tone="quiet"
          size="sm"
          onClick={() => {
            void openLogs().then(opened);
          }}
        >
          Logs
        </Button>
      </Row>
      {outcome ? (
        <div className="py-3">
          <Status text={outcome.text} error={outcome.error} />
        </div>
      ) : null}
    </Section>
  );
}

export function SettingsPage({
  settings,
  hotkey,
  sideMenu,
  readOnly,
  saveSettings,
}: {
  settings: Settings;
  /** The switcher shortcut as registered. */
  hotkey: HotkeyState;
  /** Where the side menu is now. */
  sideMenu: SideMenuState | null;
  /** The settings come from a newer Mosaïk: they are shown, and cannot be changed here. */
  readOnly: boolean;
  saveSettings: SaveSettings;
}) {
  const announce = useAnnounce();
  const [problem, setProblem] = useState<string | null>(null);
  const [more, setMore] = useState(false);
  const moreId = useId();

  const save = (next: Settings) => {
    setProblem(null);
    void saveSettings(next).then((saved) => {
      // Windows can keep Mosaïk from starting with it: the setting goes back, and the message
      // says where to change it.
      const adjusted = saved.settings.start_with_windows !== next.start_with_windows;
      if (!saved.ok || adjusted) setProblem(saved.message);
      else announce(saved.message);
    });
  };

  const side: SideMenu = settings.edge.enabled ? settings.edge.side : 'off';
  const moved = sideNote(sideMenu);

  return (
    <article aria-labelledby="settings-title" className="mx-auto w-full max-w-[46rem]">
      <PageTitle id="settings-title">Settings</PageTitle>
      {readOnly ? (
        <p className="mt-2 flex items-center gap-2 text-small text-[var(--muted)]">
          <Glyph name="bell" size={14} />
          These settings were saved by a newer Mosaïk. Update Mosaïk to change them here.
        </p>
      ) : null}
      <div className="mt-3">
        <Status text={problem} error />
      </div>

      <div className="mt-4">
        <Section title="General">
          <Toggle
            label="Start with Windows"
            description="Mosaïk waits in the tray, so the side menu and shortcuts are always there."
            checked={settings.start_with_windows}
            disabled={readOnly}
            onChange={(on) => {
              save({ ...settings, start_with_windows: on });
            }}
          />
          <Row label="Theme">
            <Segmented
              label="Theme"
              value={settings.theme}
              options={THEMES}
              disabled={readOnly}
              onChange={(theme) => {
                save({ ...settings, theme });
              }}
            />
          </Row>
          <ShortcutRecorder
            label="Shortcut to switch workspace"
            description="Shows your workspaces over any app."
            value={hotkey.registered ? hotkey.shortcut : null}
            onSave={async (shortcut) => {
              const saved = await saveSettings({ ...settings, switcher_hotkey: shortcut });
              if (!saved.ok) return saved.message;
              announce(saved.message);
              return null;
            }}
          />
          {hotkey.problem ? (
            <div className="py-3">
              <Status text={hotkey.problem} error />
            </div>
          ) : null}
        </Section>

        <Section title="Windows">
          <Toggle
            label="Snap windows to zones"
            description="Drag a window over a zone and let go."
            checked={settings.snap.enabled}
            disabled={readOnly}
            onChange={(on) => {
              save({ ...settings, snap: { ...settings.snap, enabled: on } });
            }}
          />
          <Toggle
            label="Show zone numbers on windows"
            description="Each window shows its zone’s number. Drop a window on a number to make tabs."
            checked={settings.frames.show_tags}
            disabled={readOnly}
            onChange={(on) => {
              // Off hides the numbers only: tabs stay where they were.
              save({ ...settings, frames: { ...settings.frames, show_tags: on } });
            }}
          />
          <Toggle
            label="Coloured borders"
            description="Each window wears its zone’s colour."
            checked={settings.frames.show_frames}
            disabled={readOnly}
            onChange={(on) => {
              save({ ...settings, frames: { ...settings.frames, show_frames: on } });
            }}
          />
        </Section>

        <Section title="Side menu">
          <Row
            label="Edge of the main screen"
            description={
              <>
                Point at the line on that edge for Arrange, layouts and your workspaces. It stays out of games.
                {moved ? <span className="mt-1 block text-[var(--text)]">{moved}</span> : null}
              </>
            }
          >
            <Segmented
              label="Side menu"
              value={side}
              options={SIDE_MENU}
              disabled={readOnly}
              onChange={(v) => {
                const edge =
                  v === 'off' ? { ...settings.edge, enabled: false } : { ...settings.edge, enabled: true, side: v };
                save({ ...settings, edge });
              }}
            />
          </Row>
        </Section>

        <div className="mt-7">
          <button
            type="button"
            aria-expanded={more}
            aria-controls={moreId}
            onClick={() => {
              setMore((m) => !m);
            }}
            className="-ml-1 inline-flex min-h-10 items-center gap-1.5 rounded-lg px-1 font-display text-heading font-semibold tracking-tight hover:text-[var(--accent)]"
          >
            <Glyph name="next" size={14} className={`transition-transform duration-150 ${more ? 'rotate-90' : ''}`} />
            More settings
          </button>
          <div id={moreId} hidden={!more} className="mt-2">
            <div className="tile divide-y divide-[var(--line)] px-5">
              <Row label="Zone tags" description="Where each window shows its zone’s number.">
                <Segmented
                  label="Zone tags"
                  value={settings.frames.tags_in_title_bar ? 'title' : 'above'}
                  options={TAG_PLACES}
                  disabled={readOnly}
                  onChange={(v) => {
                    save({ ...settings, frames: { ...settings.frames, tags_in_title_bar: v === 'title' } });
                  }}
                />
              </Row>
              <Toggle
                label="Zones follow a resized window"
                description="Neighbouring zones move with its edge. Shift resizes the window alone."
                checked={settings.snap.resize_zones}
                disabled={readOnly}
                onChange={(on) => {
                  save({ ...settings, snap: { ...settings.snap, resize_zones: on } });
                }}
              />
              <Row label="Holding Shift while dragging">
                <Segmented
                  label="Holding Shift while dragging"
                  value={settings.snap.invert_shift ? 'snap' : 'free'}
                  options={SHIFT}
                  disabled={readOnly}
                  onChange={(v) => {
                    save({ ...settings, snap: { ...settings.snap, invert_shift: v === 'snap' } });
                  }}
                />
              </Row>
              <Toggle
                label="Minimize other windows when a workspace opens"
                description="Only the workspace’s windows stay on screen."
                checked={settings.switching.minimize_others}
                disabled={readOnly}
                onChange={(on) => {
                  save({ ...settings, switching: { ...settings.switching, minimize_others: on } });
                }}
              />
              <Toggle
                label="Start a workspace’s closed apps"
                description="Its apps that are not open start when it opens."
                checked={settings.switching.launch_missing}
                disabled={readOnly}
                onChange={(on) => {
                  save({ ...settings, switching: { ...settings.switching, launch_missing: on } });
                }}
              />
              <Row label="Tabs not shown">
                <Segmented
                  label="Tabs not shown"
                  value={settings.tab_hiding === 'minimize' ? 'minimize' : 'auto'}
                  options={HIDING}
                  disabled={readOnly}
                  onChange={(v) => {
                    const tab_hiding: HideMethod | null = v === 'minimize' ? 'minimize' : null;
                    save({ ...settings, tab_hiding });
                  }}
                />
              </Row>
            </div>
          </div>
        </div>

        <About />
      </div>
    </article>
  );
}
