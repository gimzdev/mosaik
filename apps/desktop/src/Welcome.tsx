// First run: what Mosaïk does, in three steps, and one choice (start with Windows).

import { useState } from 'react';

import type { SaveSettings } from './App';
import type { Settings, ZoneColor } from './protocol';
import { Button, Keys, Mark, PageTitle, Status, Toggle, useSay } from './ui';

interface Tile {
  area: string;
  color: ZoneColor | null;
}

/** A small screen: three windows, plain while being arranged, in their zones' colours once
 * saved. */
function Screen({ tiles, tags = false, switcher = false }: { tiles: Tile[]; tags?: boolean; switcher?: boolean }) {
  return (
    <div
      aria-hidden="true"
      className="relative grid aspect-[16/9] w-full gap-1.5 rounded-lg border border-[var(--line)] bg-[var(--bg)] p-2"
      style={{ gridTemplateAreas: '"a b" "a c"', gridTemplateColumns: '3fr 2fr' }}
    >
      {tiles.map((t) => (
        <div
          key={t.area}
          className="relative rounded-[5px] border-2 forced-colors:border-[CanvasText]"
          style={{
            gridArea: t.area,
            borderColor: t.color ? `var(--zone-${t.color})` : 'var(--line)',
            background: t.color ? `color-mix(in srgb, var(--zone-${t.color}) 14%, transparent)` : 'var(--surface)',
          }}
        >
          {tags && t.color ? (
            <span
              className="absolute -top-[5px] left-1 h-[9px] w-7 rounded-full"
              style={{ background: `var(--zone-${t.color})` }}
            />
          ) : null}
        </div>
      ))}
      {switcher ? (
        <div className="absolute inset-x-[22%] top-[22%] flex flex-col gap-1 rounded-md border border-[var(--line)] bg-[var(--surface)] p-1.5 shadow-md">
          <span className="h-2 rounded-sm bg-[var(--accent)]" />
          <span className="h-2 rounded-sm bg-[var(--line)]" />
          <span className="h-2 w-2/3 rounded-sm bg-[var(--line)]" />
        </div>
      ) : null}
    </div>
  );
}

export function Welcome({
  settings,
  shortcut,
  saveSettings,
  onDone,
}: {
  settings: Settings;
  /** The shortcut that opens the switcher now. */
  shortcut: string;
  saveSettings: SaveSettings;
  onDone: () => void;
}) {
  const [startWithWindows, setStartWithWindows] = useState(settings.start_with_windows);
  const [problem, setProblem] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const say = useSay();

  const steps = [
    {
      title: 'Arrange',
      body: 'Open the apps you use for one project and place them side by side.',
      screen: (
        <Screen
          tiles={[
            { area: 'a', color: null },
            { area: 'b', color: null },
            { area: 'c', color: null },
          ]}
        />
      ),
    },
    {
      title: 'Save',
      body: (
        <>
          Press <Keys shortcut={shortcut} /> then <Keys shortcut="S" /> and name it. Each place becomes a zone with its
          own colour.
        </>
      ),
      screen: (
        <Screen
          tags
          tiles={[
            { area: 'a', color: 'lapis' },
            { area: 'b', color: 'verdigris' },
            { area: 'c', color: 'saffron' },
          ]}
        />
      ),
    },
    {
      title: 'Switch',
      body: (
        <>
          Press <Keys shortcut={shortcut} /> and pick a workspace: its windows come back and missing apps start.
        </>
      ),
      screen: (
        <Screen
          tags
          switcher
          tiles={[
            { area: 'a', color: 'madder' },
            { area: 'b', color: 'slate' },
            { area: 'c', color: 'lapis' },
          ]}
        />
      ),
    },
  ];

  const finish = async () => {
    setPending(true);
    const saved = await saveSettings({ ...settings, onboarding_done: true, start_with_windows: startWithWindows });
    setPending(false);
    if (!saved.ok) {
      setProblem(saved.message);
      return;
    }
    // Windows may have kept Mosaïk from starting with it: the message says why.
    if (saved.settings.start_with_windows !== startWithWindows) say(saved.message, true);
    onDone();
  };

  return (
    <article aria-labelledby="welcome-title">
      <div className="flex items-center gap-3">
        <Mark size={34} />
        <PageTitle id="welcome-title">Welcome to Mosaïk</PageTitle>
      </div>
      <p className="mt-3 max-w-2xl text-[var(--muted)]">
        Every project gets its own workspace: its windows, where you put them, one shortcut away. Mosaïk is free, makes
        no network requests, and keeps your layouts on this PC.
      </p>
      <ol className="mt-7 grid grid-cols-3 gap-4">
        {steps.map((s, i) => (
          <li
            key={s.title}
            className="flex flex-col gap-3 rounded-xl border border-[var(--line)] bg-[var(--surface)] p-4"
          >
            {s.screen}
            <div>
              <h2 className="font-display text-heading font-semibold">
                <span className="mr-1.5 text-[var(--muted)]">{i + 1}.</span>
                {s.title}
              </h2>
              <p className="mt-1 text-small text-[var(--muted)]">{s.body}</p>
            </div>
          </li>
        ))}
      </ol>
      <p className="mt-4 text-small text-[var(--muted)]">
        Tip: drop a window on another’s tag and they become tabs, one window shown at a time.
      </p>
      <div className="mt-6 flex flex-wrap items-center justify-between gap-x-8 gap-y-2 border-t border-[var(--line)] pt-3">
        <div>
          <Toggle
            leading
            label="Start Mosaïk with Windows"
            description="It waits in the tray until you press its shortcut."
            checked={startWithWindows}
            onChange={setStartWithWindows}
          />
        </div>
        <Button
          tone="primary"
          pending={pending}
          onClick={() => {
            void finish();
          }}
        >
          Get started
        </Button>
      </div>
      <Status text={problem} error />
    </article>
  );
}
