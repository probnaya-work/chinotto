import { doc, runTransaction, serverTimestamp, type Firestore } from 'firebase/firestore';
import { mergeRecordWire, primaryText, readRecordWire, retainLegacyRecordText, type Digest, type RecordWire } from './recordSyncProtocol';

export async function publishRecordMetadata(
  db: Firestore, uid: string, id: string, wire: RecordWire, digest: Digest,
  sessionIsCurrent: () => boolean,
): Promise<'sent' | 'removed'> {
  const ref = doc(db, 'users', uid, 'entries', id);
  return runTransaction(db, async (transaction) => {
    const remote = await transaction.get(ref);
    if (!sessionIsCurrent()) throw new Error('Sync session changed');
    const data = remote.data();
    if (data?.deletedAt != null) return 'removed';
    let merged = data?.record == null ? wire
      : mergeRecordWire(wire, await readRecordWire(id, data.record, digest));
    if (typeof data?.text === 'string') merged = await retainLegacyRecordText(id, merged, data.text, digest);
    const text = primaryText(merged, typeof data?.text === 'string' ? data.text : undefined);
    if (!sessionIsCurrent()) throw new Error('Sync session changed');
    transaction.set(ref, {
      text, createdAt: merged.capturedAt, updatedAt: serverTimestamp(), record: merged,
    }, { merge: true });
    return 'sent';
  });
}

