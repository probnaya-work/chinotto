import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useLayoutEffect, useSyncExternalStore } from 'react';

export type Protection = { enabled: boolean; locked: boolean; epoch: number; busy: boolean; ready: boolean; error: string | null };
let state: Protection = { enabled: true, locked: true, epoch: -1, busy: false, ready: false, error: null };
const listeners = new Set<() => void>();
let initialized = false;
const native = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
const publish = (next: Partial<Protection>) => {
  if (next.epoch !== undefined && next.epoch < state.epoch) return;
  state = { ...state, ...next };
  if (state.locked) {
    document.querySelectorAll('audio, video').forEach((media) => (media as HTMLMediaElement).pause());
    window.dispatchEvent(new Event('chinotto-record-locked'));
  }
  listeners.forEach((listener) => listener());
};
const snapshot = () => state;
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
export const recordCanRead = () => state.ready && !state.locked;
export const recordEpoch = () => state.epoch;
export async function refreshProtection() {
  if (!native()) { publish({ enabled: false, locked: false, epoch: 0, busy: false, ready: true }); return; }
  try { publish({ ...await invoke<Protection>('privacy_state'), ready: true }); }
  catch { publish({ locked: true, ready: true, error: 'Device protection is unavailable. Try again.' }); }
}
export async function authenticateRecord(action: 'unlock' | 'enable' | 'disable' = 'unlock') {
  if (!native()) return false;
  if (state.busy) return false;
  publish({ busy: true, error: null });
  try {
    const next = await invoke<Protection>('privacy_authenticate', { action });
    publish({ ...next, ready: true, error: null });
    return recordCanRead() && next.epoch === state.epoch;
  } catch {
    await refreshProtection();
    publish({ error: 'Could not authenticate. Use your Mac password or try again.' });
    return false;
  } finally { publish({ busy: false }); }
}
export function useRecordPrivacy() {
  const protection = useSyncExternalStore(subscribe, snapshot);
  useEffect(() => {
    if (initialized) return;
    initialized = true;
    void (async () => {
      if (native()) await listen<Protection>('chinotto-privacy', ({ payload }) => publish({ ...payload, ready: true }));
      await refreshProtection();
    })().catch(() => publish({ locked: true, ready: true, error: 'Device protection is unavailable. Try again.' }));
  }, []);
  useLayoutEffect(() => {
    if (native() && protection.ready) void invoke('privacy_rendered', { epoch: protection.epoch, locked: protection.locked });
  }, [protection]);
  return protection;
}

/** Suppress results that completed after a native relock, including a lock/unlock cycle. */
export async function invokeRecord<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const epoch = recordEpoch();
  const result = await invoke<T>(command, args);
  const ownCapture = ['capture_fragment', 'capture_encounter', 'capture_voice', 'record_transcript'].includes(command);
  const control = ['privacy_state', 'privacy_rendered', 'refresh_tray', 'fit_capture_popover', 'open_tray_capture', 'prepare_voice_access', 'run_native_speech_recognition', 'stop_voice_capture', 'discard_voice_recording', 'record_transcript'].includes(command);
  if (!ownCapture && !control && native() && (!recordCanRead() || epoch !== recordEpoch())) throw new Error('record_locked');
  return result;
}
