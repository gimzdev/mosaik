// First run: what Mosaïk does, in three steps, and one choice (start with Windows).

import { useState } from 'react';

import type { SaveSettings } from './App';
import type { Settings, ZoneColor } from './protocol';
import { Button, Keys, Status, Toggle } from './ui';

/** A small picture of a screen with zones in the palette's colours. */
function Mosaic({ tiles }: { tiles: { area: string; color: ZoneColor }[] }) {
  return (
    <div
      aria-hidden="true"
      className="grid h-24 w-36 shrink-0 gap-1 rounded-lg border border-[var(--line)] bg-[var(--bg)] p-1.5"
      style={{ gridTemplateAreas: '"a b" "a c"', gridTemplateColumns: '3fr 2fr' }}
    >
      {tiles.map((t) => (
        <div
          key={t.area}
          className="rounded-[5px] border-2 forced-colors:border-[CanvasText]"
          style={{ gridArea: t.area, borderColor: `var(--zone-${t.color})`, background: `color-mix(in srgb, var(--zone-${t.color}) 14%, transparent)` }}
        />
      ))}
    </div>
  );
}

export function Welcome({
  settings,
  saveSettings,
  onDone,
}: {
  settings: Settings;
  saveSettings: SaveSettings;
  onDone: () => void;
}) {
  const [startWithWindows, setStartWithWindows] = useState(settings.start_with_windows);
  const [problem, setProblem] = useState<string | null>(null);
  const shortcut = settings.switcher_hotkey;
  const steps = [
    {
      title: 'Arrange a project’s windows',
      body: 'Open the apps you use for one project and put them where you like them, side by side.',
      tiles: [
        { area: 'a', color: 'lapis' },
        { area: 'b', color: 'verdigris' },
        { area: 'c', color: 'saffron' },
      ],
    },
    {
      title: 'Save them as a workspace',
      body: (
        <>
          Press <Keys shortcut={shortcut} />, then <Keys shortcut="S" />, and give it a name. Windows side by side
          become zones, each with its colour and name.
        </>
      ),
      tiles: [
        { area: 'a', color: 'lapis' },
        { area: 'b', color: 'madder' },
        { area: 'c', color: 'saffron' },
      ],
    },
    {
      title: 'Switch in one go',
      body: (
        <>
          Press <Keys shortcut={shortcut} /> and pick a workspace: its windows come back where they were, and missing
          apps start. Drag a window onto another’s tag to make them tabs.
        </>
      ),
      tiles: [
        { area: 'a', color: 'verdigris' },
        { area: 'b', color: 'slate' },
        { area: 'c', color: 'lapis' },
      ],
    },
  ] as const;

  const finish = async () => {
    const error = await saveSettings({ ...settings, onboarding_done: true, start_with_windows: startWithWindows });
    if (error) setProblem(error);
    else onDone();
  };

  return (
    <article aria-labelledby="welcome-title">
      <h1 id="welcome-title" className="font-display text-3xl font-bold tracking-tight">
        Welcome to Mosaïk
      </h1>
      <p className="mt-2 max-w-xl text-[var(--muted)]">
        Every project gets its own workspace: its windows, where you put them, one shortcut away. Mosaïk is free, makes no
        network requests, and keeps your layouts on this PC.
      </p>
      <ol className="mt-8 flex flex-col gap-4">
        {steps.map((s, i) => (
          <li key={s.title} className="flex items-center gap-5 rounded-xl border border-[var(--line)] bg-[var(--surface)] p-4">
            <Mosaic tiles={[...s.tiles]} />
            <div>
              <h2 className="font-display text-lg font-semibold">
                <span className="mr-2 text-[var(--muted)]">{i + 1}.</span>
                {s.title}
              </h2>
              <p className="mt-1 text-[var(--muted)]">{s.body}</p>
            </div>
          </li>
        ))}
      </ol>
      <div className="mt-6 rounded-xl border border-[var(--line)] bg-[var(--surface)] px-4">
        <Toggle
          label="Start Mosaïk with Windows"
          description="It waits in the tray, using almost nothing until you press its shortcut."
          checked={startWithWindows}
          onChange={setStartWithWindows}
        />
      </div>
      <div className="mt-6 flex items-center gap-4">
        <Button
          tone="primary"
          onClick={() => {
            void finish();
          }}
        >
          Get started
        </Button>
        <Status text={problem} error />
      </div>
    </article>
  );
}
