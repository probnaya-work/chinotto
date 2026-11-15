/**
 * Additive Record metadata carried beside legacy entry text.
 * Immutable, content-addressed wordings preserve causal ancestry across retries and devices.
 * The same module and fixtures are mirrored in the desktop repository.
 */
export type Wording = {
  body: string;
  at: string;
  kind: 'capture' | 'correction' | 'legacy';
  parents: string[];
};
export type Continuation = {
  continuesId: string;
  linkedAt: string;
  origin: 'explicit' | 'accepted_suggestion';
};
export type RecordWire = {
  version: 1;
  capturedAt: string;
  wordings: Record<string, Wording>;
  continuation: Continuation | null;
};
export type SyncState = {
  wire: RecordWire;
  selected: string;
  revisionCount: number;
  revisions: string[];
  resolvedAt: string | null;
};
export type SyncSnapshot = {
  id: string;
  body: string;
  capturedAt: string;
  history: { body: string; supersededAt: string }[];
  continuation: Continuation | null;
  state: string | null;
  generation: number;
  dirty: boolean;
  resolvedAt: string | null;
};
export type SyncCommit = {
  snapshot: SyncSnapshot;
  state: string;
  wire: RecordWire;
  body: string;
  correctedAt: string | null;
  revisions: { body: string; supersededAt: string }[];
  conflict: string | null;
};
export type Digest = (value: string) => Promise<string>;
const iso = (s: string): string => {
  if (!s || !Number.isFinite(Date.parse(s))) throw new Error('invalid Record date');
  return new Date(s).toISOString();
};
const signature = (id: string, w: Wording) =>
  JSON.stringify([id, w.body, w.at, w.kind, [...w.parents].sort()]);
const object = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === 'object' && !Array.isArray(v);
function checkSize(value: unknown): void {
  if (object(value) && object(value.wordings)) {
    const nodes = Object.values(value.wordings);
    if (nodes.length > 2048) throw new Error('Record history has too many versions to sync in one entry');
    if (nodes.some((node) => object(node) && Array.isArray(node.parents) && node.parents.length > 32)) {
      throw new Error('Record history has too many concurrent versions to sync in one entry');
    }
  }
  const text = JSON.stringify(value);
  let bytes = 0;
  for (const char of text) {
    const n = char.codePointAt(0)!;
    bytes += n < 128 ? 1 : n < 2048 ? 2 : n < 65536 ? 3 : 4;
  }
  if (bytes > 400_000) throw new Error('Record history is too large to sync in one entry');
}
export async function readRecordWire(id: string, value: unknown, digest: Digest): Promise<RecordWire> {
  if (!object(value) || value.version !== 1 || typeof value.capturedAt !== 'string' ||
      !object(value.wordings)) throw new Error('unsupported Record metadata');
  checkSize(value);
  const wordings: Record<string, Wording> = {};
  const entries = Object.entries(value.wordings);
  if (!entries.length || entries.length > 2048) throw new Error('invalid Record history length');
  for (const [key, w] of entries) {
    if (!/^[a-f0-9]{64}$/.test(key) || !object(w) || typeof w.body !== 'string' ||
        typeof w.at !== 'string' || !['capture', 'correction', 'legacy'].includes(String(w.kind)) ||
        !Array.isArray(w.parents) || w.parents.length > 32 ||
        !w.parents.every((p) => typeof p === 'string' && /^[a-f0-9]{64}$/.test(p)) ||
        new Set(w.parents).size !== w.parents.length) throw new Error('invalid Record wording');
    const node: Wording = {
      body: w.body, at: iso(w.at), kind: w.kind as Wording['kind'], parents: [...w.parents].sort(),
    };
    if (await digest(signature(id, node)) !== key) throw new Error('Record wording identity mismatch');
    wordings[key] = node;
  }
  // A complete snapshot must carry every ancestor. Missing nodes cannot become false heads.
  const visited = new Set<string>();
  const visiting = new Set<string>();
  function visit(key: string): void {
    if (visited.has(key)) return;
    if (visiting.has(key) || !wordings[key]) throw new Error('invalid Record ancestry');
    visiting.add(key);
    for (const parent of wordings[key].parents) visit(parent);
    visiting.delete(key);
    visited.add(key);
  }
  for (const key of Object.keys(wordings)) visit(key);
  let continuation: Continuation | null = null;
  if (value.continuation != null) {
    const c = value.continuation;
    if (!object(c) || typeof c.continuesId !== 'string' || !c.continuesId ||
        c.continuesId.length > 1024 || c.continuesId === id || typeof c.linkedAt !== 'string' ||
        !['explicit', 'accepted_suggestion'].includes(String(c.origin))) {
      throw new Error('invalid Record continuation');
    }
    continuation = {
      continuesId: c.continuesId, linkedAt: iso(c.linkedAt), origin: c.origin as Continuation['origin'],
    };
  }
  return { version: 1, capturedAt: iso(value.capturedAt), wordings, continuation };
}
/** A legacy text projection can precede the authored version of exactly the same words. */
function legacyAliases(wire: RecordWire): Map<string, string[]> {
  const authored = new Map<string, boolean>();
  function hasAuthoredAncestry(key: string): boolean {
    if (authored.has(key)) return authored.get(key)!;
    const node = wire.wordings[key];
    // Only an entirely authored chain can supply the missing ancestry of a legacy root.
    const known = node.kind !== 'legacy' && node.parents.every(hasAuthoredAncestry);
    authored.set(key, known);
    return known;
  }
  const byBody = new Map<string, string[]>();
  for (const [key, node] of Object.entries(wire.wordings)) {
    if (hasAuthoredAncestry(key)) byBody.set(node.body, [...(byBody.get(node.body) ?? []), key]);
  }
  return new Map(Object.entries(wire.wordings)
    .filter(([, node]) => node.kind === 'legacy' && byBody.has(node.body))
    .map(([key, node]) => [key, byBody.get(node.body)!]));
}
function rawHeads(wire: RecordWire): string[] {
  const parents = new Set(Object.values(wire.wordings).flatMap((w) => w.parents));
  return Object.keys(wire.wordings).filter((key) => !parents.has(key)).sort();
}
export function heads(wire: RecordWire): string[] {
  const aliases = legacyAliases(wire);
  const parents = new Set(Object.values(wire.wordings).flatMap((w) =>
    w.parents.flatMap((key) => [key, ...(aliases.get(key) ?? [])])));
  return Object.keys(wire.wordings).filter((key) => !parents.has(key) && !aliases.has(key)).sort();
}
export function mergeRecordWire(local: RecordWire, remote: RecordWire): RecordWire {
  if (iso(local.capturedAt) !== iso(remote.capturedAt)) throw new Error('Record capture date changed');
  const wordings = { ...local.wordings };
  for (const [key, node] of Object.entries(remote.wordings)) {
    if (wordings[key] && JSON.stringify(wordings[key]) !== JSON.stringify(node)) {
      throw new Error('Record wording was rewritten');
    }
    wordings[key] = node;
  }
  const result: RecordWire = {
    version: 1, capturedAt: iso(local.capturedAt), wordings,
    continuation: local.continuation ?? remote.continuation,
  };
  checkSize(result);
  return result;
}
export function primaryText(wire: RecordWire, previous?: string): string {
  const tips = heads(wire);
  // Legacy clients keep their current wording during a genuine conflict.
  if (previous !== undefined && tips.some((id) => wire.wordings[id].body === previous)) return previous;
  return wire.wordings[tips[0]].body;
}
export async function retainLegacyRecordText(id: string, wire: RecordWire, text: string, digest: Digest): Promise<RecordWire> {
  if (!text.trim() || Object.values(wire.wordings).some((w) => w.body === text)) return wire;
  const node: Wording = { body: text, at: wire.capturedAt, kind: 'legacy', parents: [] };
  const key = await digest(signature(id, node));
  const result = { ...wire, wordings: { ...wire.wordings, [key]: node } };
  checkSize(result);
  return result;
}
function descends(wire: RecordWire, child: string, ancestor: string): boolean {
  const todo = [child];
  const aliases = legacyAliases(wire);
  const equivalents = new Set([ancestor, ...(aliases.get(ancestor) ?? [])]);
  const seen = new Set<string>();
  while (todo.length) {
    const key = todo.pop()!;
    if (equivalents.has(key)) return true;
    if (seen.has(key)) continue;
    seen.add(key);
    todo.push(...(wire.wordings[key]?.parents ?? []), ...(aliases.get(key) ?? []));
  }
  return false;
}
export async function prepareRecordSync(
  snapshot: SyncSnapshot,
  digest: Digest,
  received?: { wire: unknown; text: string },
): Promise<SyncCommit> {
  const incoming = received ? await readRecordWire(snapshot.id, received.wire, digest) : null;
  if (incoming && iso(incoming.capturedAt) !== iso(snapshot.capturedAt)) {
    throw new Error('Record capture date changed');
  }
  let state: SyncState | null = snapshot.state ? JSON.parse(snapshot.state) as SyncState : null;
  async function add(wire: RecordWire, body: string, at: string, kind: Wording['kind'], parents: string[]) {
    const node: Wording = { body, at: iso(at), kind, parents: [...parents].sort() };
    const key = await digest(signature(snapshot.id, node));
    wire.wordings[key] = node;
    return key;
  }
  if (!state) {
    const wire: RecordWire = {
      version: 1, capturedAt: iso(snapshot.capturedAt), wordings: {}, continuation: snapshot.continuation,
    };
    // A legacy projection may arrive before its metadata. Reuse the actual known version.
    const matches = incoming && !snapshot.history.length
      ? Object.keys(incoming.wordings).filter((key) => incoming.wordings[key].body === snapshot.body)
      : [];
    if (matches.length) {
      state = {
        wire: { ...incoming!, continuation: snapshot.continuation ?? incoming!.continuation },
        selected: matches.sort()[0], revisionCount: 0, revisions: [], resolvedAt: snapshot.resolvedAt,
      };
    } else {
      const first = snapshot.history[0]?.body ?? snapshot.body;
      state = {
        wire, selected: await add(wire, first, snapshot.capturedAt, 'capture', []),
        revisionCount: 0, revisions: [], resolvedAt: null,
      };
    }
  }
  if (snapshot.history.length < state.revisionCount) throw new Error('Record history was shortened');
  for (let index = state.revisionCount; index < snapshot.history.length; index += 1) {
    const old = state.selected;
    const revision = snapshot.history[index];
    if (state.wire.wordings[old].body !== revision.body) throw new Error('Record history diverged locally');
    state.revisions.push(old);
    state.selected = await add(state.wire, snapshot.history[index + 1]?.body ?? snapshot.body,
      revision.supersededAt, 'correction', [...new Set([old, ...rawHeads(state.wire)
        .filter((key) => state!.wire.wordings[key].body === revision.body)])]);
  }
  if (state.wire.wordings[state.selected].body !== snapshot.body) {
    // Legacy/derived text changes have no authored correction history. Do not invent one.
    state.selected = await add(state.wire, snapshot.body, snapshot.capturedAt, 'legacy', []);
  }
  state.wire.continuation = snapshot.continuation ?? state.wire.continuation;
  if (snapshot.resolvedAt && snapshot.resolvedAt !== state.resolvedAt) {
    state.selected = await add(state.wire, snapshot.body, snapshot.resolvedAt, 'correction', rawHeads(state.wire));
    state.resolvedAt = snapshot.resolvedAt;
  }
  if (incoming) state.wire = mergeRecordWire(state.wire, incoming);
  if (received && !Object.values(state.wire.wordings).some((w) => w.body === received.text)) {
    await add(state.wire, received.text, snapshot.capturedAt, 'legacy', []);
  }
  const tips = heads(state.wire);
  if (!tips.length) throw new Error('Record has no current wording');
  if (!tips.includes(state.selected)) {
    const descendants = tips.filter((key) => descends(state!.wire, key, state!.selected));
    if (descendants.length === 1) state.selected = descendants[0];
  }
  if (tips.length === 1) state.selected = tips[0];
  const conflictKey = tips.find((key) => state!.wire.wordings[key].body !== state!.wire.wordings[state!.selected].body);
  const additions = new Map<string, { body: string; supersededAt: string }>();
  for (const child of Object.values(state.wire.wordings)) {
    if (child.kind !== 'correction') continue;
    for (const parent of child.parents) {
      if (state.revisions.includes(parent) || state.wire.wordings[parent].body === child.body) continue;
      const previous = additions.get(parent);
      if (!previous || child.at < previous.supersededAt) {
        additions.set(parent, { body: state.wire.wordings[parent].body, supersededAt: child.at });
      }
    }
  }
  const ordered = [...additions.entries()].sort((a, b) =>
    a[1].supersededAt.localeCompare(b[1].supersededAt) || a[0].localeCompare(b[0]));
  for (const [key] of ordered) state.revisions.push(key);
  state.revisionCount = snapshot.history.length + ordered.length;
  const selected = state.wire.wordings[state.selected];
  checkSize(state.wire);
  return {
    snapshot, state: JSON.stringify(state), wire: state.wire, body: selected.body,
    correctedAt: selected.kind === 'correction' ? selected.at : null,
    revisions: ordered.map(([, row]) => row),
    conflict: conflictKey ? state.wire.wordings[conflictKey].body : null,
  };
}

