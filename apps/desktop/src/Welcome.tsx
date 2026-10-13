// First run: what Mosaïk does, shown once (windows gliding into their zones) and said in three
// steps, and one choice (start with Windows).

import { useEffect, useState } from 'react';

import type { SaveSettings } from './App';
import type { ArrangePreview, Settings } from './protocol';
import { Stage } from './Stage';
import { Button, Keys, Mark, PageTitle, Status, Toggle, useSay } from './ui';

/** A made-up screen for the welcome: four windows left where they fell, and where Arrange
 * puts them (the chat as a tab of the notes). */
const DEMO: ArrangePreview = {
  work_area: { left: 0, top: 0, right: 1600, bottom: 900 },
  zones: [
    { name: 'Browser', color: 'lapis', rect: { left: 0, top: 0, right: 800, bottom: 900 } },
    { name: 'Code', color: 'verdigris', rect: { left: 800, top: 0, right: 1600, bottom: 450 } },
    { name: 'Notes', color: 'saffron', rect: { left: 800, top: 450, right: 1600, bottom: 900 } },
  ],
  windows: [
    {
      label: 'Browser',
      now: { left: 140, top: 90, right: 860, bottom: 610 },
      then: { left: 3, top: 3, right: 797, bottom: 897 },
      tab: false,
    },
    {
      label: 'Code editor',
      now: { left: 620, top: 220, right: 1380, bottom: 760 },
      then: { left: 803, top: 3, right: 1597, bottom: 447 },
      tab: false,
    },
    {
      label: 'Notes',
      now: { left: 1040, top: 60, right: 1520, bottom: 420 },
      then: { left: 803, top: 453, right: 1597, bottom: 897 },
      tab: false,
    },
    {
      label: 'Chat',
      now: { left: 260, top: 520, right: 760, bottom: 860 },
      then: { left: 803, top: 453, right: 1597, bottom: 897 },
      tab: true,
    },
  ],
  elsewhere: 0,
};

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
  /** The windows in their zones: after a moment, as Arrange does it (at once without motion). */
  const [arranged, setArranged] = useState(false);
  const say = useSay();

  useEffect(() => {
    const wait = matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : 900;
    const t = setTimeout(() => {
      setArranged(true);
    }, wait);
    return () => {
      clearTimeout(t);
    };
  }, []);

  const steps = [
    {
      title: 'Arrange',
      body: 'Click Arrange windows, or pick a layout. Every window snaps into its place.',
    },
    {
      title: 'Keep',
      body: 'Mosaïk remembers the arrangement as a workspace. Each zone wears its own colour.',
    },
    {
      title: 'Come back',
      body: (
        <>
          Press <Keys shortcut={shortcut} /> and pick a workspace: its windows return, and closed apps open again.
        </>
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
      <p className="mt-2 max-w-2xl text-[var(--muted)]">
        One click tidies your screen: every window snaps into a zone, and Mosaïk remembers them for next time. It is
        free, makes no network requests, and keeps your layouts on this PC.
      </p>
      <div className="mt-5">
        <Stage
          preview={DEMO}
          mode={arranged ? 'then' : 'now'}
          maxHeight={230}
          caption={
            arranged ? (
              <>
                Arranged: each window in a zone, the chat as a tab.{' '}
                <button
                  type="button"
                  onClick={() => {
                    setArranged(false);
                    setTimeout(() => {
                      setArranged(true);
                    }, 700);
                  }}
                  className="rounded-md px-1 font-semibold text-[var(--accent)] hover:bg-[var(--hover)]"
                >
                  Show it again
                </button>
              </>
            ) : (
              'A screen before Arrange'
            )
          }
        />
      </div>
      <ol className="mt-5 grid grid-cols-3 gap-4">
        {steps.map((s, i) => (
          <li key={s.title}>
            <h2 className="font-display text-heading font-semibold">
              <span className="mr-1.5 text-[var(--muted)]">{i + 1}.</span>
              {s.title}
            </h2>
            <p className="mt-1 text-small text-[var(--muted)]">{s.body}</p>
          </li>
        ))}
      </ol>
      <div className="mt-6 flex flex-wrap items-center justify-between gap-x-8 gap-y-2 border-t border-[var(--line)] pt-3">
        <div>
          <Toggle
            leading
            label="Start Mosaïk with Windows"
            description="It waits in the tray until you need it."
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
