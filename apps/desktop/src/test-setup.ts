// The DOM matchers the tests read with (toBeInTheDocument, toHaveTextContent…), then what jsdom
// lacks, just enough for the components: modal dialogs, media queries, CSS.escape, scrolling into
// view.

import '@testing-library/jest-dom/vitest';

if (typeof Element.prototype.scrollIntoView !== 'function') {
  Element.prototype.scrollIntoView = () => undefined;
}
if (typeof Element.prototype.scrollTo !== 'function') {
  Element.prototype.scrollTo = () => undefined;
}

if (typeof HTMLDialogElement !== 'undefined' && !('showModal' in HTMLDialogElement.prototype)) {
  Object.assign(HTMLDialogElement.prototype, {
    showModal(this: HTMLDialogElement) {
      this.open = true;
    },
    close(this: HTMLDialogElement) {
      this.open = false;
    },
  });
}

if (typeof window.matchMedia !== 'function') {
  window.matchMedia = (query: string) =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
      addListener: () => undefined,
      removeListener: () => undefined,
      dispatchEvent: () => false,
    }) as MediaQueryList;
}

if (typeof CSS === 'undefined' || typeof CSS.escape !== 'function') {
  Object.assign(globalThis, {
    CSS: { escape: (s: string) => s.replace(/[^a-zA-Z0-9_-]/g, (c) => `\\${c}`) },
  });
}
