import { describe, it, expect } from 'vitest';
import { createHash } from 'node:crypto';
import { heads, mergeRecordWire, prepareRecordSync, readRecordWire, type SyncSnapshot } from './recordSyncProtocol';
const digest = async (s: string) => createHash('sha256').update(s).digest('hex');
const AT = '2026-11-01T10:00:00.000Z';
const NEXT = '2026-11-02T10:00:00.000Z';
const base = (over: Partial<SyncSnapshot> = {}): SyncSnapshot => ({
  id: 'moment-1', body: 'original', capturedAt: AT, history: [], continuation: null,
  state: null, generation: 1, dirty: true, resolvedAt: null, ...over,
});
describe('additive Record protocol', () => {
  it('carries a sequential correction and its real earlier wording', async () => {
    const original = await prepareRecordSync(base(), digest);
    const corrected = await prepareRecordSync(base({
      body: 'corrected', history: [{ body: 'original', supersededAt: NEXT }], state: original.state,
    }), digest);
    const received = await prepareRecordSync(base({ state: original.state }), digest, {
      wire: corrected.wire, text: 'corrected',
    });
    expect(received.body).toBe('corrected');
    expect(received.revisions).toEqual([{ body: 'original', supersededAt: NEXT }]);
    expect(received.conflict).toBeNull();
    expect(received.wire.capturedAt).toBe(AT);
  });
  it('bootstraps metadata received after the legacy text without inventing a conflict', async () => {
    const remote = await prepareRecordSync(base({
      body: 'corrected', history: [{ body: 'original', supersededAt: NEXT }],
    }), digest);
    const arrived = await prepareRecordSync(base({ body: 'corrected' }), digest, {
      wire: remote.wire, text: 'corrected',
    });
    expect(arrived.body).toBe('corrected');
    expect(arrived.conflict).toBeNull();
    expect(arrived.revisions).toEqual([{ body: 'original', supersededAt: NEXT }]);
  });
  it('keeps both offline branches without choosing by wall clock', async () => {
    const original = await prepareRecordSync(base(), digest);
    const a = await prepareRecordSync(base({
      body: 'phone wording', state: original.state,
      history: [{ body: 'original', supersededAt: NEXT }],
    }), digest);
    const b = await prepareRecordSync(base({
      body: 'Mac wording', state: original.state,
      history: [{ body: 'original', supersededAt: AT }],
    }), digest);
    const seen = await prepareRecordSync(base({
      body: 'phone wording', state: a.state,
      history: [{ body: 'original', supersededAt: NEXT }],
    }), digest, { wire: b.wire, text: 'Mac wording' });
    expect(seen.body).toBe('phone wording');
    expect(seen.conflict).toBe('Mac wording');
    expect(heads(seen.wire)).toHaveLength(2);
    expect(Object.values(seen.wire.wordings).map((w) => w.body)).toEqual(
      expect.arrayContaining(['original', 'phone wording', 'Mac wording']),
    );
  });
  it('does not revert a newer wording when snapshots replay out of order', async () => {
    const original = await prepareRecordSync(base(), digest);
    const corrected = await prepareRecordSync(base({
      body: 'corrected', state: original.state,
      history: [{ body: 'original', supersededAt: NEXT }],
    }), digest);
    const replayed = await prepareRecordSync(base({
      body: 'corrected', state: corrected.state,
      history: [{ body: 'original', supersededAt: NEXT }],
    }), digest, { wire: original.wire, text: 'original' });
    expect(replayed.body).toBe('corrected');
    expect(replayed.revisions).toEqual([]);
    expect(replayed.conflict).toBeNull();
  });
  it('preserves an old-client edit outside the known version graph as a conflict', async () => {
    const known = await prepareRecordSync(base(), digest);
    const arrived = await prepareRecordSync(base({ state: known.state }), digest, {
      wire: known.wire, text: 'written by an older app',
    });
    expect(arrived.body).toBe('original');
    expect(arrived.conflict).toBe('written by an older app');
    expect(arrived.revisions).toEqual([]);
  });
  it('rejects altered wording identities and capture dates', async () => {
    const original = await prepareRecordSync(base(), digest);
    const altered = JSON.parse(JSON.stringify(original.wire));
    altered.wordings[Object.keys(altered.wordings)[0]].body = 'rewritten';
    await expect(readRecordWire('moment-1', altered, digest)).rejects.toThrow('identity');
    expect(() => mergeRecordWire(original.wire, { ...original.wire, capturedAt: NEXT }))
      .toThrow('capture date');
  });
  it('carries a continuation with its own date', async () => {
    const result = await prepareRecordSync(base({
      continuation: { continuesId: 'earlier', linkedAt: NEXT, origin: 'explicit' },
    }), digest);
    expect((await readRecordWire('moment-1', result.wire, digest)).continuation).toEqual({
      continuesId: 'earlier', linkedAt: NEXT, origin: 'explicit',
    });
  });
  it('keeps the local wording when two new descendants arrive together', async () => {
    const root = await prepareRecordSync(base(), digest);
    const branch = (body: string) => prepareRecordSync(base({
      body, state: root.state, history: [{ body: 'original', supersededAt: NEXT }],
    }), digest);
    const a = await branch('phone');
    const b = await branch('Mac');
    const seen = await prepareRecordSync(base({ state: root.state }), digest, {
      wire: mergeRecordWire(a.wire, b.wire), text: 'phone',
    });
    expect(seen.body).toBe('original');
    expect(seen.conflict).not.toBeNull();
    // A third branch was unknown when the person settled the first two.
    const c = await branch('another offline device');
    const settled = await prepareRecordSync(base({
      state: seen.state, history: seen.revisions, resolvedAt: '2026-11-03T10:00:00.000Z',
    }), digest, { wire: c.wire, text: 'another offline device' });
    expect(settled.body).toBe('original');
    expect(settled.conflict).toBe('another offline device');
    expect(heads(settled.wire)).toHaveLength(2);
  });
  it('keeps oversized local history out of the remote protocol', async () => {
    await expect(prepareRecordSync(base({ body: 'x'.repeat(400_000) }), digest))
      .rejects.toThrow('too large');
    await expect(prepareRecordSync(base({
      body: 'latest', history: Array.from({ length: 2048 }, (_, i) => ({
        body: String(i), supersededAt: NEXT,
      })),
    }), digest)).rejects.toThrow('too many versions');
  });

  it('does not turn an early legacy projection into a conflict after the next correction', async () => {
    const root = await prepareRecordSync(base(), digest);
    const remote = await prepareRecordSync(base({
      body: 'corrected', state: root.state, history: [{ body: 'original', supersededAt: NEXT }],
    }), digest);
    const early = await prepareRecordSync(base({ state: root.state }), digest, {
      wire: root.wire, text: 'corrected',
    });
    const arrived = await prepareRecordSync(base({ state: early.state }), digest, {
      wire: remote.wire, text: 'corrected',
    });
    expect(arrived.body).toBe('corrected');
    expect(arrived.conflict).toBeNull();
    const next = await prepareRecordSync(base({
      body: 'next correction', state: arrived.state,
      history: [...arrived.revisions, { body: 'corrected', supersededAt: NEXT }],
    }), digest);
    expect(next.conflict).toBeNull();
    expect(heads(next.wire)).toHaveLength(1);
  });
  it('keeps corrections made after settling an early text projection ahead of its late metadata', async () => {
    const root = await prepareRecordSync(base(), digest);
    const remote = await prepareRecordSync(base({
      body: 'corrected', state: root.state, history: [{ body: 'original', supersededAt: NEXT }],
    }), digest);
    const early = await prepareRecordSync(base({ state: root.state }), digest, {
      wire: root.wire, text: 'corrected',
    });
    const localHistory = [{ body: 'original', supersededAt: '2026-11-03T10:00:00.000Z' }];
    const settled = await prepareRecordSync(base({
      body: 'corrected', state: early.state, history: localHistory,
      resolvedAt: '2026-11-03T10:00:00.000Z',
    }), digest);
    const nextHistory = [...localHistory, { body: 'corrected', supersededAt: '2026-11-04T10:00:00.000Z' }];
    const next = await prepareRecordSync(base({
      body: 'next correction', state: settled.state, history: nextHistory,
      resolvedAt: '2026-11-03T10:00:00.000Z',
    }), digest, { wire: remote.wire, text: 'corrected' });
    expect(next.body).toBe('next correction');
    expect(next.conflict).toBeNull();
    expect(heads(next.wire)).toHaveLength(1);
  });
  it('joins equal known offline wordings when the person next corrects them', async () => {
    const root = await prepareRecordSync(base(), digest);
    const a = await prepareRecordSync(base({
      body: 'same words', state: root.state, history: [{ body: 'original', supersededAt: NEXT }],
    }), digest);
    const b = await prepareRecordSync(base({
      body: 'same words', state: root.state, history: [{ body: 'original', supersededAt: AT }],
    }), digest);
    const seen = await prepareRecordSync(base({
      body: a.body, state: a.state, history: [{ body: 'original', supersededAt: NEXT }],
    }), digest, { wire: b.wire, text: b.body });
    const next = await prepareRecordSync(base({
      body: 'next', state: seen.state,
      history: [{ body: 'original', supersededAt: NEXT }, { body: 'same words', supersededAt: NEXT }],
    }), digest);
    expect(next.conflict).toBeNull();
    expect(heads(next.wire)).toHaveLength(1);
  });

});

