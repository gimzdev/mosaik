import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { Notices } from './Notices';

afterEach(cleanup);

describe('Notices', () => {
  it('shows nothing when there is nothing to say', () => {
    const { container } = render(<Notices notices={[]} onDismiss={vi.fn()} />);
    expect(container.innerHTML).toBe('');
  });

  it('announces each notice and dismisses the one asked', async () => {
    const onDismiss = vi.fn();
    render(
      <Notices
        notices={[
          { title: 'The switcher opens with Win+Alt+S', body: 'Win+Alt+Space is taken.' },
          { title: 'Rescue windows', body: '2 window(s) restored' },
        ]}
        onDismiss={onDismiss}
      />,
    );
    expect(screen.getAllByRole('status')).toHaveLength(2);
    expect(screen.getByRole('region', { name: 'Messages from Mosaïk' })).toBeTruthy();
    const buttons = screen.getAllByRole('button', { name: 'Dismiss' });
    const second = buttons[1];
    if (!second) throw new Error('expected two dismiss buttons');
    await userEvent.click(second);
    expect(onDismiss).toHaveBeenCalledWith(1);
  });
});
