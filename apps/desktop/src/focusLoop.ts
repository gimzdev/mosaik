// Keeps the Tab key inside Mosaïk's window. In a browser, Tab from a page's last control goes
// on to the browser's own controls; Mosaïk's window has none, so the focus left the page for
// one press with nothing to see (job 0070). Here Tab from the last control goes to the first,
// and Shift+Tab from the first to the last, inside an open dialog when there is one.

const FOCUSABLE = 'a[href], button, input, select, textarea, summary, [tabindex]';

/** Whether the Tab key stops at `el`: focusable in the tab order, enabled, shown, and for a
 * radio button, the chosen one of its group (or the first, when none is chosen). */
function stopsAt(el: HTMLElement, scope: ParentNode): boolean {
  if (el.tabIndex < 0 || el.matches(':disabled') || el.closest('[hidden], [inert]')) return false;
  // Not laid out (display: none and the like). jsdom has no layout: everything counts there.
  if ('checkVisibility' in el && !el.checkVisibility()) return false;
  if (el instanceof HTMLInputElement && el.type === 'radio' && el.name) {
    const group = Array.from(scope.querySelectorAll<HTMLInputElement>('input[type="radio"]')).filter(
      (r) => r.name === el.name,
    );
    const chosen = group.find((r) => r.checked);
    return chosen ? chosen === el : group[0] === el;
  }
  return true;
}

/** For a keydown listener on the document. */
export function keepTabInside(e: KeyboardEvent): void {
  if (e.key !== 'Tab' || e.defaultPrevented || e.altKey || e.ctrlKey || e.metaKey) return;
  const scope: ParentNode = document.querySelector('dialog[open]') ?? document;
  const stops = Array.from(scope.querySelectorAll<HTMLElement>(FOCUSABLE)).filter((el) => stopsAt(el, scope));
  const first = stops.at(0);
  const last = stops.at(-1);
  if (!first || !last) return;
  const active = document.activeElement;
  if (!e.shiftKey && active === last) {
    e.preventDefault();
    first.focus();
  } else if (e.shiftKey && active === first) {
    e.preventDefault();
    last.focus();
  }
}
