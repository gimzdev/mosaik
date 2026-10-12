// Settings: the switcher's shortcut, starting with Windows, how zones and switching behave, and
// the theme. Each change is saved at once.

import { useState } from 'react';

import type { SaveSettings } from './App';
import type { HideMethod, Settings, Theme } from './protocol';
import { ShortcutRecorder } from './shortcut';
import { Choice, Section, Status, Toggle } from './ui';

const THEMES: readonly { value: Theme; label: string }[] = [
  { value: 'system', label: 'Like Windows' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
];

const SHIFT: readonly { value: 'free' | 'snap'; label: string }[] = [
  { value: 'free', label: 'Hold Shift to move freely' },
  { value: 'snap', label: 'Hold Shift to snap' },
];

const HIDING: readonly { value: 'auto' | 'minimize'; label: string }[] = [
  { value: 'auto', label: 'Hide them (recommended)' },
  { value: 'minimize', label: 'Minimise them' },
];

export function SettingsPane({ settings, saveSettings }: { settings: Settings; saveSettings: SaveSettings }) {
  const [status, setStatus] = useState<{ text: string; error: boolean } | null>(null);

  const save = (next: Settings) => {
    void saveSettings(next).then((error) => {
      setStatus(error ? { text: error, error: true } : { text: 'Saved.', error: false });
    });
  };

  return (
    <article aria-labelledby="settings-title">
      <h1 id="settings-title" className="font-display text-3xl font-bold tracking-tight">
        Settings
      </h1>
      <div className="mt-2">
        <Status text={status?.text ?? null} error={status?.error ?? false} />
      </div>

      <Section title="Shortcut" intro="One shortcut opens the switcher; everything else is in there.">
        <ShortcutRecorder
          label="Open the switcher"
          value={settings.switcher_hotkey}
          onSave={(hotkey) => saveSettings({ ...settings, switcher_hotkey: hotkey })}
        />
      </Section>

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
          description="While you drag a window by its title bar, the zones show; let go over one and the window fills it."
          checked={settings.snap.enabled}
          onChange={(on) => {
            save({ ...settings, snap: { ...settings.snap, enabled: on } });
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
          description="Each window in a zone wears the zone's colour around it."
          checked={settings.frames.show_frames}
          onChange={(on) => {
            save({ ...settings, frames: { ...settings.frames, show_frames: on } });
          }}
        />
        <Toggle
          label="Zone tags"
          description="The zone's name above its windows. Dropping a window on a tag makes them tabs."
          checked={settings.frames.show_tags}
          onChange={(on) => {
            save({ ...settings, frames: { ...settings.frames, show_tags: on } });
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

      <Section title="Tabs" intro="How background tabs are kept out of sight.">
        <Choice
          label="Background tabs"
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
