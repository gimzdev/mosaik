import type { Notice } from './protocol';

/** A notice as shown: with a number of its own, so dismissing one never dismisses another. */
export interface ShownNotice extends Notice {
  id: number;
}

/** Things Mosaïk had to tell the user: each says what happened and what to do. The list is a
 * live region that is always there, so a notice that arrives is read out. */
export function Notices({ notices, onDismiss }: { notices: ShownNotice[]; onDismiss: (id: number) => void }) {
  return (
    <section
      aria-label="Messages from Mosaïk"
      aria-live="polite"
      className={notices.length ? 'flex flex-col gap-3' : ''}
    >
      {notices.map((n) => (
        <div
          key={n.id}
          data-notice
          className="flex items-start gap-4 rounded-xl border border-[var(--line)] border-l-4 border-l-[var(--accent)] bg-[var(--surface)] p-4"
        >
          <div className="min-w-0 flex-1">
            <p className="font-semibold">{n.title}</p>
            <p className="mt-1 text-small text-[var(--muted)]">{n.body}</p>
          </div>
          <button
            type="button"
            aria-label={`Dismiss: ${n.title}`}
            className="rounded-lg px-3 py-1.5 text-small font-semibold text-[var(--accent)] hover:bg-[var(--hover)]"
            onClick={(e) => {
              // Focus moves to the next notice, else the previous one, else the page's title,
              // rather than being lost with the button.
              const card = e.currentTarget.closest('[data-notice]');
              const next = card?.nextElementSibling ?? card?.previousElementSibling;
              onDismiss(n.id);
              requestAnimationFrame(() => {
                const target = next?.querySelector('button') ?? document.querySelector<HTMLElement>('h1[tabindex]');
                target?.focus();
              });
            }}
          >
            Dismiss
          </button>
        </div>
      ))}
    </section>
  );
}
