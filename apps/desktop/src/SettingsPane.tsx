// Settings: what most people change (the switcher's shortcut, starting with Windows, the theme,
// how windows look and snap, the quick menu), then the rest behind More settings, then About.
// Each change is saved at once.

import { useId, useState } from 'react';

import { About } from './About';
import type { SaveSettings } from './App';
import type { EdgeSide, HideMethod, HotkeyState, Settings, Theme } from './protocol';
import { ShortcutRecorder } from './shortcut';
import { Choice, PageTitle, Section, Toggle, useSay } from './ui';

const THEMES: readonly { value: Theme; label: string }[] = [
  { value: 'system', label: 'Like Windows' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
];

type TagPlace = 'title' | 'above' | 'off';
const TAG_PLACES: readonly { value: TagPlace; label: string }[] = [
  { value: 'title', label: 'In title bars' },
  { value: 'above', label: 'Above windows' },
  { value: 'off', label: 'Off' },
];

type QuickMenu = EdgeSide | 'off';
const QUICK_MENU: readonly { value: QuickMenu; label: string }[] = [
  { value: 'right', label: 'Right edge' },
  { value: 'left', label: 'Left edge' },
  { value: 'off', label: 'Off' },
];

const SHIFT: readonly { value: 'free' | 'snap'; label: string }[] = [
  { value: 'free', label: 'Hold Shift to move freely' },
  { value: 'snap', label: 'Hold Shift to snap' },
];

const HIDING: readonly { value: 'auto' | 'minimize'; label: string }[] = [
  { value: 'auto', label: 'Hide them (recommended)' },
  { value: 'minimize', label: 'Minimise them' },
];

export function SettingsPane({
  settings,
  hotkey,
  saveSettings,
  onWelcome,
}: {
  settings: Settings;
  /** The switcher shortcut as registered. */
  hotkey: HotkeyState;
  saveSettings: SaveSettings;
  /** Shows the welcome again (from About). */
  onWelcome: () => void;
}) {
  const say = useSay();
  const [more, setMore] = useState(false);
  const moreId = useId();

  const save = (next: Settings) => {
    void saveSettings(next).then((saved) => {
      // Windows can keep Mosaïk from starting with it: the setting goes back and the message
      // says where to change it, and stays until dismissed.
      const adjusted = saved.settings.start_with_windows !== next.start_with_windows;
      say(saved.message, !saved.ok || adjusted);
    });
  };

  const tags: TagPlace = settings.frames.show_tags ? (settings.frames.tags_in_title_bar ? 'title' : 'above') : 'off';
  const quick: QuickMenu = settings.edge.enabled ? settings.edge.side : 'off';

  return (
    <article aria-labelledby="settings-title">
      <PageTitle id="settings-title">Settings</PageTitle>

      <div className="mt-6">
        <Section title="General">
          <ShortcutRecorder
            label="Open the switcher"
            value={hotkey.registered ? hotkey.shortcut : null}
            onSave={async (shortcut) => {
              const saved = await saveSettings({ ...settings, switcher_hotkey: shortcut });
              if (!saved.ok) return saved.message;
              say(saved.message);
              return null;
            }}
          />
          {hotkey.problem ? (
            <p role="status" className="py-3 text-small text-[var(--danger)]">
              {hotkey.problem}
            </p>
          ) : null}
          <Toggle
            label="Start Mosaïk with Windows"
            description="It waits in the tray."
            checked={settings.start_with_windows}
            onChange={(on) => {
              save({ ...settings, start_with_windows: on });
            }}
          />
          <Choice
            label="Theme"
            value={settings.theme}
            options={THEMES}
            onChange={(theme) => {
              save({ ...settings, theme });
            }}
          />
        </Section>
      </div>

      <Section title="Windows">
        <Toggle
          label="Snap windows into zones"
          description="Drag a window over a zone and let go."
          checked={settings.snap.enabled}
          onChange={(on) => {
            save({ ...settings, snap: { ...settings.snap, enabled: on } });
          }}
        />
        <Choice
          label="Zone tags"
          description="Each window shows its zone’s name. Drop a window on a tag to make tabs."
          value={tags}
          options={TAG_PLACES}
          onChange={(v) => {
            // Off hides the names only: tabs stay where tags were.
            const frames =
              v === 'off'
                ? { ...settings.frames, show_tags: false }
                : { ...settings.frames, show_tags: true, tags_in_title_bar: v === 'title' };
            save({ ...settings, frames });
          }}
        />
        <Toggle
          label="Coloured borders"
          description="Each window wears its zone’s colour, a little apart from the next."
          checked={settings.frames.show_frames}
          onChange={(on) => {
            save({ ...settings, frames: { ...settings.frames, show_frames: on } });
          }}
        />
        <Choice
          label="Quick menu"
          description="A thin line at the screen’s edge. Point at it for Arrange and more."
          value={quick}
          options={QUICK_MENU}
          onChange={(v) => {
            const edge =
              v === 'off' ? { ...settings.edge, enabled: false } : { ...settings.edge, enabled: true, side: v };
            save({ ...settings, edge });
          }}
        />
      </Section>

      <div className="mt-6">
        <button
          type="button"
          aria-expanded={more}
          aria-controls={moreId}
          onClick={() => {
            setMore((m) => !m);
          }}
          className="inline-flex min-h-8 items-center gap-2 rounded-lg px-2 text-small font-semibold text-[var(--accent)] hover:bg-[var(--hover)]"
        >
          <span
            aria-hidden="true"
            className={`inline-block transition-transform duration-150 ${more ? 'rotate-90' : ''}`}
          >
            ›
          </span>
          More settings
        </button>
        <div id={moreId} hidden={!more} className="mt-2">
          <Section title="Dragging and resizing">
            <Toggle
              label="Zones follow a resized window"
              description="Neighbouring zones follow its edge. Shift resizes the window alone."
              checked={settings.snap.resize_zones}
              onChange={(on) => {
                save({ ...settings, snap: { ...settings.snap, resize_zones: on } });
              }}
            />
            <Choice
              label="Shift while dragging"
              value={settings.snap.invert_shift ? 'snap' : 'free'}
              options={SHIFT}
              onChange={(v) => {
                save({ ...settings, snap: { ...settings.snap, invert_shift: v === 'snap' } });
              }}
            />
          </Section>
          <Section title="Opening a workspace">
            <Toggle
              label="Minimise the previous workspace’s windows"
              description="Only the new workspace’s windows stay on screen."
              checked={settings.switching.minimize_others}
              onChange={(on) => {
                save({ ...settings, switching: { ...settings.switching, minimize_others: on } });
              }}
            />
            <Toggle
              label="Start apps that are not open"
              description="Its closed apps open again."
              checked={settings.switching.launch_missing}
              onChange={(on) => {
                save({ ...settings, switching: { ...settings.switching, launch_missing: on } });
              }}
            />
          </Section>
          <Section title="Tabs">
            <Choice
              label="Hidden tabs"
              value={settings.tab_hiding === 'minimize' ? 'minimize' : 'auto'}
              options={HIDING}
              onChange={(v) => {
                const tab_hiding: HideMethod | null = v === 'minimize' ? 'minimize' : null;
                save({ ...settings, tab_hiding });
              }}
            />
          </Section>
        </div>
      </div>

      <About onWelcome={onWelcome} />
    </article>
  );
}
