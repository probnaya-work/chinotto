import { invoke } from '@tauri-apps/api/core';
import { getAuth } from 'firebase/auth';
import { getOrInitApp, getOrInitFirestore, isFirestoreSessionAccessLostError } from './desktopFirestoreSync';
import { isThisDeviceKnownRevoked, isThisDeviceRevoked } from './syncDevices';
import { isFirebaseSyncConfigured } from './firebaseConfig';
import { deleteLocalEntriesForSync } from '../features/entries/entryApi';
import { prepareRecordSync, readRecordWire, type SyncSnapshot } from './recordSyncProtocol';
import { publishRecordMetadata } from './recordMetadataTransport';

const digest = (value: string) => invoke<string>('hash_record_sync_value', { value });
let flushing = false;
function available(): boolean {
  return '__TAURI_INTERNALS__' in window && isFirebaseSyncConfigured();
}
export async function prepareStoredDesktopRecord(id: string, incoming?: { wire: unknown; text: string }) {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    const snapshot = await invoke<SyncSnapshot | null>('record_sync_snapshot', { id });
    if (!snapshot) return null;
    const prepared = await prepareRecordSync(snapshot, digest, incoming);
    if (await invoke<boolean>('commit_record_sync', { commit: prepared })) {
      await invoke('apply_pending_record_sync_links');
      return prepared;
    }
  }
  throw new Error('Record changed during sync; retrying later');
}
export async function flushDesktopRecordMetadata(): Promise<void> {
  if (flushing || !available()) return;
  const user = getAuth(getOrInitApp()).currentUser;
  if (!user || user.isAnonymous) return;
  const current = () => getAuth(getOrInitApp()).currentUser?.uid === user.uid && !isThisDeviceKnownRevoked();
  flushing = true;
  try {
    if (await isThisDeviceRevoked() || !current()) return;
    await invoke('activate_record_sync_user', { uid: user.uid });
    const ids = await invoke<string[]>('pending_record_sync_ids', { limit: 25 });
    for (const id of ids) {
      if (!current()) return;
      try {
        const prepared = await prepareStoredDesktopRecord(id);
        if (!prepared || !current()) continue;
        const outcome = await publishRecordMetadata(getOrInitFirestore(), user.uid, id, prepared.wire, digest, current);
        if (!current()) return;
        if (outcome === 'removed') {
          await deleteLocalEntriesForSync([id]);
          await invoke('absorb_remote_deletes', { ids: [id] });
        } else {
          await invoke('acknowledge_record_sync', { id, generation: prepared.snapshot.generation });
        }
      } catch (error) {
        await invoke('record_sync_error', {
          id, message: error instanceof Error ? error.message : 'Record sync failed',
        });
        if (isFirestoreSessionAccessLostError(error)) return;
        console.warn('[Chinotto sync] Record metadata remains pending', id,
          error instanceof Error ? error.message : 'sync failed');
      }
    }
  } finally { flushing = false; }
}
export async function ingestDesktopRecordMetadata(
  id: string, text: string, createdAt: string, value: unknown,
): Promise<boolean> {
  if (!('__TAURI_INTERNALS__' in window) || isThisDeviceKnownRevoked()) return false;
  await readRecordWire(id, value, digest);
  const allowed = await invoke<boolean>('ensure_record_sync_fragment', {
    id, body: text, capturedAt: createdAt,
  });
  if (!allowed) return false;
  return (await prepareStoredDesktopRecord(id, { wire: value, text })) !== null;
}

