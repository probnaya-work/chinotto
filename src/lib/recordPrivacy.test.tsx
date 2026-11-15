import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
const harness = vi.hoisted(() => ({ invoke: vi.fn(), event: null as null | ((event: { payload: unknown }) => void) }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: harness.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async (_: string, fn: typeof harness.event) => { harness.event = fn; return () => {}; }) }));
import { authenticateRecord, invokeRecord, recordCanRead, refreshProtection, useRecordPrivacy } from './recordPrivacy';
let epoch = 0;
beforeEach(async () => {
  Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true });
  epoch++;
  harness.invoke.mockReset().mockImplementation(async (command) => {
    if (command === 'privacy_state') return { enabled: true, locked: true, epoch, busy: false };
    return undefined;
  });
  await refreshProtection();
});
it('starts protected without prompting and stays hidden on cancellation', async () => {
  const { result } = renderHook(() => useRecordPrivacy());
  await waitFor(() => expect(result.current.ready).toBe(true));
  expect(recordCanRead()).toBe(false);
  expect(harness.invoke.mock.calls.some(([c]) => c === 'privacy_authenticate')).toBe(false);
  harness.invoke.mockRejectedValueOnce(new Error('cancel'));
  await act(async () => { expect(await authenticateRecord()).toBe(false); });
  expect(recordCanRead()).toBe(false);
});
it('rejects late authentication success after a newer native lock event', async () => {
  renderHook(() => useRecordPrivacy());
  let complete!: (value: unknown) => void;
  harness.invoke.mockImplementation(async (command) => {
    if (command === 'privacy_authenticate') return new Promise((resolve) => { complete = resolve; });
    return { enabled: true, locked: true, epoch, busy: false };
  });
  let authentication!: Promise<boolean>;
  act(() => { authentication = authenticateRecord(); });
  act(() => { epoch++; harness.event?.({ payload: { enabled: true, locked: true, epoch, busy: true } }); });
  await act(async () => { complete({ enabled: true, locked: false, epoch: epoch - 1, busy: false }); expect(await authentication).toBe(false); });
  expect(recordCanRead()).toBe(false);
});
it('suppresses old private results but still returns a newly saved capture', async () => {
  renderHook(() => useRecordPrivacy());
  await act(async () => { harness.invoke.mockResolvedValueOnce({ enabled: true, locked: false, epoch, busy: false }); await refreshProtection(); });
  let complete!: (value: unknown) => void;
  harness.invoke.mockImplementation((command) => command === 'recent_fragments' ? new Promise((resolve) => { complete = resolve; }) : Promise.resolve());
  const reading = invokeRecord('recent_fragments');
  act(() => { epoch++; harness.event?.({ payload: { enabled: true, locked: true, epoch, busy: false } }); });
  complete(['private']);
  await expect(reading).rejects.toThrow('record_locked');
  harness.invoke.mockResolvedValueOnce({ id: 'new' });
  await expect(invokeRecord('capture_fragment')).resolves.toEqual({ id: 'new' });
});
