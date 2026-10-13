// First run: what Mosaïk does, shown once (windows gliding into their zones) and said in three
// steps, and one choice (start with Windows).

import { useEffect, useState } from 'react';

import type { SaveSettings } from './App';
import { request } from './engine';
import type { ArrangePreview, Settings } from './protocol';
import { Stage } from './Stage';
import { Button, Keys, Mark, PageTitle, Status, Toggle, useSay } from './ui';

/** A made-up screen for the welcome: three windows left where they fell, and where Arrange
 * puts them: the editor the most room, the browser and the notes beside it, edge to edge. */
const DEMO: ArrangePreview = {
  work_area: { left: 0, top: 0, right: 1600, bottom: 900 },
  zones: [
    { name: 'Code', color: 'lapis', rect: { left: 0, top: 0, right: 928, bottom: 900 } },
    { name: 'Browser', color: 'verdigris', rect: { left: 928, top: 0, right: 1600, bottom: 495 } },
    { name: 'Notes', color: 'saffron', rect: { left: 928, top: 495, right: 1600, bottom: 900 } },
  ],
  windows: [
    {
      label: 'Browser',
      now: { left: 140, top: 90, right: 860, bottom: 610 },
      then: { left: 928, top: 0, right: 1600, bottom: 495 },
      minimized: false,
    },
    {
      label: 'Code editor',
      now: { left: 620, top: 220, right: 1380, bottom: 760 },
      then: { left: 0, top: 0, right: 928, bottom: 900 },
      minimized: false,
    },
    {
      label: 'Notes',
      now: { left: 1040, top: 60, right: 1520, bottom: 420 },
      then: { left: 928, top: 495, right: 1600, bottom: 900 },
      minimized: false,
    },
  ],
  elsewhere: 0,
  task: 'coding',
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
      body: 'One click, or pick a layout.',
    },
    {
      title: 'Keep',
      body: 'Mosaïk keeps it as a workspace.',
    },
    {
      title: 'Come back',
      body: (
        <>
          <Keys shortcut={shortcut} /> opens it again.
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
    const refused = saved.settings.start_with_windows !== startWithWindows ? saved.message : null;
    // The windows open now, sorted into a workspace per task; none moves. Said only when it
    // made some, and after why Mosaïk will not start with Windows, never in its place.
    const sorted = await request({ cmd: 'sort_windows' });
    const made = sorted.reply === 'done' && sorted.message.startsWith('Your open windows') ? sorted.message : null;
    if (refused) say(made ? `${refused} ${made}` : refused, true);
    else if (made) say(made);
    onDone();
  };

  return (
    <article aria-labelledby="welcome-title">
      <div className="flex items-center gap-3">
        <Mark size={34} />
        <PageTitle id="welcome-title">Welcome to Mosaïk</PageTitle>
      </div>
      <p className="mt-2 max-w-2xl text-[var(--muted)]">
        One click tidies your screen, and Mosaïk remembers it for next time. Free, and nothing leaves this PC.
      </p>
      <div className="mt-5">
        <Stage
          preview={DEMO}
          mode={arranged ? 'then' : 'now'}
          maxHeight={230}
          caption={
            arranged ? (
              <>
                Arranged: each window in a zone.{' '}
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
            description="It waits in the tray."
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
