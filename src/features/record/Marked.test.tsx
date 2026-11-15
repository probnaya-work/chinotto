import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { Marked } from './Marked';

describe('Unicode phrase evidence', () => {
  it('marks the actual phrase across punctuation', () => {
    render(<Marked text="спокойное, внимание" mark="спокойное внимание" />);
    expect(screen.getByText('спокойное, внимание').tagName).toBe('MARK');
  });

  it('does not present intervening Cyrillic words as punctuation', () => {
    const { container } = render(
      <Marked text="спокойное чужое внимание" mark="спокойное внимание" />,
    );
    expect(container.querySelector('mark')).toBeNull();
  });
});
