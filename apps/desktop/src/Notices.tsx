import type { Notice } from './protocol';

export interface NoticesProps {
  notices: Notice[];
  onDismiss: (index: number) => void;
}

/** Things Mosaïk had to tell the user: each says what happened and what to do. */
export function Notices({ notices, onDismiss }: NoticesProps) {
  if (notices.length === 0) return null;
  return (
    <section aria-label="Messages from Mosaïk" className="mt-6 flex flex-col gap-3">
      {notices.map((n, i) => (
        <div
          key={`${n.title}-${String(i)}`}
          role="status"
          className="flex items-start gap-4 rounded-xl border border-[var(--line)] bg-[var(--surface)] p-4"
        >
          <div className="min-w-0 flex-1">
            <p className="font-semibold">{n.title}</p>
            <p className="mt-1 text-[var(--muted)]">{n.body}</p>
          </div>
          <button
            type="button"
            className="rounded-lg px-3 py-1.5 text-[13px] font-semibold text-[var(--accent)] hover:bg-[var(--line)]"
            onClick={() => {
              onDismiss(i);
            }}
          >
            Dismiss
          </button>
        </div>
      ))}
    </section>
  );
}
