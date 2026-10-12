import { afterEach, describe, expect, it } from 'vitest';

import { keepTabInside } from './focusLoop';

afterEach(() => {
  document.body.innerHTML = '';
});

function tab(shift = false): KeyboardEvent {
  const e = new KeyboardEvent('keydown', { key: 'Tab', shiftKey: shift, bubbles: true, cancelable: true });
  keepTabInside(e);
  return e;
}

const byId = (id: string) => document.getElementById(id);

describe('the Tab key in Mosaïk’s window', () => {
  // Job 0070: on every page, Tab from the last control left the page for an unnamed stop with
  // no focus to see, before coming back to the first control.
  it('goes from the last control to the first, and back with Shift', () => {
    document.body.innerHTML = `
      <nav><button id="first">Workspaces</button></nav>
      <h1 tabindex="-1" id="title">Settings</h1>
      <button id="middle">Change</button>
      <button disabled>Save</button>
      <button id="last">Show the welcome again</button>`;
    byId('last')?.focus();
    expect(tab().defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(byId('first'));
    expect(tab(true).defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(byId('last'));
    // Between them the browser goes on as usual.
    byId('middle')?.focus();
    expect(tab().defaultPrevented).toBe(false);
    byId('title')?.focus();
    expect(tab(true).defaultPrevented).toBe(false);
  });

  it('counts a group of radio buttons once, by its chosen one', () => {
    document.body.innerHTML = `
      <button id="first">Workspaces</button>
      <input type="radio" name="hide" id="auto" checked />
      <input type="radio" name="hide" id="minimise" />`;
    byId('auto')?.focus();
    expect(tab().defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(byId('first'));
    expect(tab(true).defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(byId('auto'));
  });

  it('is not held by a dialog that is merely open (a modal one holds it: the browser test)', () => {
    document.body.innerHTML = `
      <button id="page">Settings</button>
      <dialog open><button id="cancel">Cancel</button><button id="discard">Discard changes</button></dialog>`;
    byId('discard')?.focus();
    expect(tab().defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(byId('page'));
  });

  it('goes by tabindex first, then the page’s order', () => {
    document.body.innerHTML = `<button id="zero">Zero</button><button id="one" tabindex="1">One</button>`;
    // The order: One (tabindex 1), then Zero.
    byId('zero')?.focus();
    expect(tab().defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(byId('one'));
  });

  it('keeps radio groups of different forms apart', () => {
    document.body.innerHTML = `
      <form><input type="radio" name="theme" id="a" checked /></form>
      <form><input type="radio" name="theme" id="b" checked /></form>`;
    byId('b')?.focus();
    expect(tab().defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(byId('a'));
  });

  it('leaves alone keys that are not a plain Tab, and Tabs already handled', () => {
    document.body.innerHTML = `<button id="first">A</button><button id="last">B</button>`;
    byId('last')?.focus();
    const ctrl = new KeyboardEvent('keydown', { key: 'Tab', ctrlKey: true, cancelable: true });
    keepTabInside(ctrl);
    expect(ctrl.defaultPrevented).toBe(false);
    const handled = new KeyboardEvent('keydown', { key: 'Tab', cancelable: true });
    handled.preventDefault();
    keepTabInside(handled);
    expect(document.activeElement).toBe(byId('last'));
  });
});
