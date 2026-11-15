import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
vi.mock('../../lib/recordApi', () => ({
  lineFor: vi.fn(async () => [{ fragment: { id: 'f1', body: 'private wording', capturedAt: '2026-11-01T10:00:00Z', captureMethod: 'typed', correctionCount: 0 }, position: 0, isContinuation: false }]),
  fragmentHistory: vi.fn(async () => []),
}));
vi.mock('./traces', () => ({ tracesFor: () => [], tracesHeading: () => '', assembleTraces: async () => [] }));
import { FragmentFocus } from './FragmentFocus';
import * as api from '../../lib/recordApi';
const props = { id: 'f1', corpus: [], materials: { encounters: new Map(), voices: new Map() }, onLeave: () => {}, onChanged: () => {}, onOpen: () => {}, onHold: () => {}, onRelease: () => {}, heldIds: new Set<string>() };
it.each(['continue', 'correct'])('removes private DOM while preserving the unfinished %s', async (mode) => {
  const view = render(<FragmentFocus {...props} startCorrecting={mode === 'correct'} />);
  const label = mode === 'correct' ? 'Correct the wording' : 'Continue this line';
  const field = await screen.findByLabelText(label);
  fireEvent.change(field, { target: { value: 'unfinished words' } });
  vi.mocked(api.lineFor).mockClear();
  view.rerender(<FragmentFocus {...props} blocked startCorrecting={mode === 'correct'} />);
  expect(screen.queryByLabelText(label)).toBeNull();
  expect(view.container.textContent).toBe('');
  expect(api.lineFor).not.toHaveBeenCalled();
  // Escape in the capture field must not erase the suspended correction.
  fireEvent.keyDown(window, { key: 'Escape' });
  view.rerender(<FragmentFocus {...props} startCorrecting={mode === 'correct'} />);
  await waitFor(() => expect(screen.getByLabelText(label)).toHaveValue('unfinished words'));
});
