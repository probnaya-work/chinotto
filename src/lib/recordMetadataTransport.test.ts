import { createHash } from 'node:crypto';
import { describe, expect, it, beforeEach, vi } from 'vitest';
const mockCloud = vi.hoisted(() => ({
  data: undefined as Record<string, unknown> | undefined,
  retry: undefined as Record<string, unknown> | undefined,
  writes: [] as Record<string, unknown>[],
}));
vi.mock('firebase/firestore', () => ({
  doc: (_db: unknown, ...path: string[]) => path.join('/'),
  serverTimestamp: () => 'server-time',
  runTransaction: async (_db: unknown, run: (tx: unknown) => Promise<unknown>) => {
    let pending: Record<string, unknown> | undefined;
    const tx = {
      get: async () => ({ data: () => mockCloud.data }),
      set: (_ref: unknown, payload: Record<string, unknown>) => { pending = payload; },
    };
    let result = await run(tx);
    if (mockCloud.retry) {
      mockCloud.data = mockCloud.retry;
      mockCloud.retry = undefined;
      pending = undefined;
      result = await run(tx);
    }
    if (pending) {
      mockCloud.data = { ...mockCloud.data, ...pending };
      mockCloud.writes.push(pending);
    }
    return result;
  },
}));
import type { Firestore } from 'firebase/firestore';
import { prepareRecordSync, heads, type SyncSnapshot, type RecordWire } from './recordSyncProtocol';
import { publishRecordMetadata } from './recordMetadataTransport';
const digest = async (s: string) => createHash('sha256').update(s).digest('hex');
const at = '2026-11-01T10:00:00.000Z';
const snapshot: SyncSnapshot = {
  id: 'one', body: 'original', capturedAt: at, history: [], continuation: null,
  state: null, generation: 1, dirty: true, resolvedAt: null,
};
const db = {} as Firestore;
async function branches() {
  const root = await prepareRecordSync(snapshot, digest);
  const branch = (body: string) => prepareRecordSync({
    ...snapshot, body, state: root.state, history: [{ body: 'original', supersededAt: '2026-11-02T10:00:00.000Z' }],
  }, digest);
  return { root, a: await branch('phone'), b: await branch('Mac') };
}
describe('Record metadata transport', () => {
  beforeEach(() => { mockCloud.data = undefined; mockCloud.retry = undefined; mockCloud.writes = []; });
  it('merges concurrent branches inside the server transaction', async () => {
    const { a, b } = await branches();
    mockCloud.data = { text: 'Mac', record: b.wire };
    await publishRecordMetadata(db, 'user', 'one', a.wire, digest, () => true);
    const wire = mockCloud.data!.record as RecordWire;
    expect(heads(wire)).toHaveLength(2);
    expect(mockCloud.data!.text).toBe('Mac');
    expect(Object.values(wire.wordings).map((w) => w.body)).toEqual(
      expect.arrayContaining(['original', 'phone', 'Mac']));
  });
  it('recomputes the merge when Firestore retries after another writer', async () => {
    const { root, a, b } = await branches();
    mockCloud.data = { text: 'original', record: root.wire };
    mockCloud.retry = { text: 'Mac', record: b.wire };
    await publishRecordMetadata(db, 'user', 'one', a.wire, digest, () => true);
    expect(mockCloud.writes).toHaveLength(1);
    expect(heads(mockCloud.data!.record as RecordWire)).toHaveLength(2);
  });
  it('retains a legacy edit that arrived without matching metadata', async () => {
    const { a } = await branches();
    mockCloud.data = { text: 'old app wording' };
    await publishRecordMetadata(db, 'user', 'one', a.wire, digest, () => true);
    expect(mockCloud.data!.text).toBe('old app wording');
    expect(Object.values((mockCloud.data!.record as RecordWire).wordings).map((w) => w.body))
      .toContain('old app wording');
  });
  it('never clears or writes across a remote tombstone', async () => {
    const { a } = await branches();
    mockCloud.data = { text: 'removed', deletedAt: 'server-time' };
    expect(await publishRecordMetadata(db, 'user', 'one', a.wire, digest, () => true)).toBe('removed');
    expect(mockCloud.writes).toEqual([]);
  });
  it('does not publish after the signed-in user changed', async () => {
    const { a } = await branches();
    await expect(publishRecordMetadata(db, 'user', 'one', a.wire, digest, () => false))
      .rejects.toThrow('session changed');
    expect(mockCloud.writes).toEqual([]);
  });
});

