// Settings: the switcher's shortcut, starting with Windows, the theme, how zones, switching and
// tabs behave. Each change is saved at once.

import type { SaveSettings } from './App';
import type { EdgeSide, HideMethod, HotkeyState, Settings, Theme } from './protocol';
import { ShortcutRecorder } from './shortcut';
import { Choice, PageTitle, Section, Toggle, useSay } from './ui';

const THEMES: readonly { value: Theme; label: string }[] = [
  { value: 'system', label: 'Like Windows' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
];

const SHIFT: readonly { value: 'free' | 'snap'; label: string }[] = [
  { value: 'free', label: 'Hold Shift to move freely' },
  { value: 'snap', label: 'Hold Shift to snap' },
];

const TAG_PLACES: readonly { value: 'title' | 'above'; label: string }[] = [
  { value: 'above', label: 'In a bar above each window' },
  { value: 'title', label: 'In each window’s title bar, where there is room' },
];

const EDGES: readonly { value: EdgeSide; label: string }[] = [
  { value: 'right', label: 'Right edge' },
  { value: 'left', label: 'Left edge' },
];

const HIDING: readonly { value: 'auto' | 'minimize'; label: string }[] = [
  { value: 'auto', label: 'Hide them (recommended)' },
  { value: 'minimize', label: 'Minimise them' },
];

export function SettingsPane({
  settings,
  hotkey,
  saveSettings,
}: {
  settings: Settings;
  /** The switcher shortcut as registered. */
  hotkey: HotkeyState;
  saveSettings: SaveSettings;
}) {
  const say = useSay();

  const save = (next: Settings) => {
    void saveSettings(next).then((saved) => {
      // Windows can keep Mosaïk from starting with it: the setting goes back and the message
      // says where to change it, and stays until dismissed.
      const adjusted = saved.settings.start_with_windows !== next.start_with_windows;
      say(saved.message, !saved.ok || adjusted);
    });
  };

  return (
    <article aria-labelledby="settings-title">
      <PageTitle id="settings-title">Settings</PageTitle>

      <div className="mt-6">
        <Section title="Switcher" intro="One shortcut opens the switcher, where you open, save and manage workspaces.">
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
        </Section>
      </div>

      <Section title="General">
        <Toggle
          label="Start Mosaïk with Windows"
          description="It waits in the tray until you press its shortcut."
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

      <Section title="Zones">
        <Toggle
          label="Snap windows into zones"
          description="While you drag a window by its title bar, the zones show; let go over one and the window goes into it."
          checked={settings.snap.enabled}
          onChange={(on) => {
            save({ ...settings, snap: { ...settings.snap, enabled: on } });
          }}
        />
        <Toggle
          label="Zones follow a resized window"
          description="Resize a window in a zone by an edge it shares with other zones, and those zones and their windows follow. Shift resizes the window alone."
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
        <Toggle
          label="Coloured frames"
          description="Each window in a zone wears the zone’s colour around it."
          checked={settings.frames.show_frames}
          onChange={(on) => {
            save({ ...settings, frames: { ...settings.frames, show_frames: on } });
          }}
        />
        <Toggle
          label="Zone tags"
          description="The zone’s name on its windows. Drop a window on a tag to make them tabs."
          checked={settings.frames.show_tags}
          onChange={(on) => {
            save({ ...settings, frames: { ...settings.frames, show_tags: on } });
          }}
        />
        <Choice
          label="Tags and tabs"
          description="In a bar, every window in a zone starts at the same line. In title bars, windows fill their zones; one whose title bar has no room sits below a bar."
          value={settings.frames.tags_in_title_bar ? 'title' : 'above'}
          options={TAG_PLACES}
          onChange={(v) => {
            save({ ...settings, frames: { ...settings.frames, tags_in_title_bar: v === 'title' } });
          }}
        />
      </Section>

      <Section
        title="Quick menu"
        intro="A thin line at the edge of your main screen. Rest the pointer on it for Arrange, the switcher and more; drag it along the edge to move it."
      >
        <Toggle
          label="Show the quick menu"
          checked={settings.edge.enabled}
          onChange={(on) => {
            save({ ...settings, edge: { ...settings.edge, enabled: on } });
          }}
        />
        <Choice
          label="Where"
          value={settings.edge.side}
          options={EDGES}
          onChange={(side) => {
            save({ ...settings, edge: { ...settings.edge, side } });
          }}
        />
      </Section>

      <Section title="Switching workspaces">
        <Toggle
          label="Minimise the previous workspace’s windows"
          description="Windows the new workspace does not use are minimised, so only its windows are on screen."
          checked={settings.switching.minimize_others}
          onChange={(on) => {
            save({ ...settings, switching: { ...settings.switching, minimize_others: on } });
          }}
        />
        <Toggle
          label="Start apps that are not open"
          description="When a workspace opens, apps it needs are started, and their windows go to their places."
          checked={settings.switching.launch_missing}
          onChange={(on) => {
            save({ ...settings, switching: { ...settings.switching, launch_missing: on } });
          }}
        />
      </Section>

      <Section title="Tabs">
        <Choice
          label="Tabs not shown"
          value={settings.tab_hiding === 'minimize' ? 'minimize' : 'auto'}
          options={HIDING}
          onChange={(v) => {
            const tab_hiding: HideMethod | null = v === 'minimize' ? 'minimize' : null;
            save({ ...settings, tab_hiding });
          }}
        />
      </Section>
    </article>
  );
}
