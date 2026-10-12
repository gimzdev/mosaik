import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { Notices } from './Notices';

afterEach(cleanup);

describe('Notices', () => {
  it('is an empty live region when there is nothing to say', () => {
    render(<Notices notices={[]} onDismiss={vi.fn()} />);
    const region = screen.getByRole('region', { name: 'Messages from Mosaïk' });
    expect(region.getAttribute('aria-live')).toBe('polite');
    expect(region.textContent).toBe('');
  });

  it('dismisses the notice asked, by its own number', async () => {
    const onDismiss = vi.fn();
    render(
      <Notices
        notices={[
          { id: 7, title: 'The switcher opens with Win+Alt+S', body: 'Win+Alt+Space is taken.' },
          { id: 9, title: 'Rescue windows', body: '2 windows restored' },
        ]}
        onDismiss={onDismiss}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Dismiss: Rescue windows' }));
    expect(onDismiss).toHaveBeenCalledWith(9);
  });
});
