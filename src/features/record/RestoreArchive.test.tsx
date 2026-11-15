import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
const mock = vi.hoisted(() => ({ invoke: vi.fn(), open: vi.fn(), authenticate: vi.fn(), read: vi.fn(() => true) }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mock.invoke }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: mock.open }));
vi.mock('../../lib/recordPrivacy', () => ({ invokeRecord: mock.invoke, authenticateRecord: mock.authenticate, recordCanRead: mock.read, recordEpoch: () => 1 }));
import { RestoreArchive } from './RestoreArchive';
const preview = { newMoments: 2, audioFiles: 1, newArchived: 0, duplicates: 1, changed: 1, removed: 1, waitingLinks: 0, skippedJudgements: 0, from: null, to: null, sample: [{ body: 'older thought', at: '' }] };
beforeEach(() => { vi.clearAllMocks(); mock.read.mockReturnValue(true); mock.open.mockResolvedValue('/archive.zip'); mock.authenticate.mockResolvedValue(true); });
it('requires reviewing the archive before the first write and reloads after success', async () => {
  mock.invoke.mockImplementation(async (command) => command === 'prepare_record_restore' ? { token: 't', preview } : preview);
  const onRestored = vi.fn(); render(<RestoreArchive protectedRecord={false} onRestored={onRestored} />);
  fireEvent.click(screen.getByText('choose archive ›')); await screen.findByText('older thought');
  expect(mock.invoke.mock.calls.some(([c]) => c === 'restore_record_archive')).toBe(false);
  fireEvent.click(screen.getByRole('button', { name: 'Restore archive' }));
  await waitFor(() => expect(onRestored).toHaveBeenCalledOnce()); expect(await screen.findByText(/Restored 2 thoughts/)).toBeTruthy();
});
it('cancels a prepared preview when settings closes', async () => {
  mock.invoke.mockImplementation(async (command) => command === 'prepare_record_restore' ? { token: 't', preview } : undefined);
  const ui = render(<RestoreArchive protectedRecord={false} onRestored={vi.fn()} />);
  fireEvent.click(screen.getByText('choose archive ›')); await screen.findByText('older thought'); ui.unmount();
  expect(mock.invoke.mock.calls.some(([c]) => c === 'discard_record_restore')).toBe(true);
  expect(mock.invoke.mock.calls.some(([c]) => c === 'restore_record_archive')).toBe(false);
});
it('does not pick a file when authentication is cancelled', async () => {
  mock.authenticate.mockResolvedValue(false); render(<RestoreArchive protectedRecord onRestored={vi.fn()} />);
  fireEvent.click(screen.getByText('choose archive ›')); await waitFor(() => expect(mock.authenticate).toHaveBeenCalledOnce());
  expect(mock.open).not.toHaveBeenCalled();
});
