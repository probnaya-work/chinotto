# Record continuity sync, version 1

This additive protocol carries authored wording history and continuation links between current iPhone and Mac clients. Capture and Find stay local. Android sync, audio transfer, encounters, holds, judgements and Returns are outside this change.

## Cloud shape

The existing `users/{uid}/entries/{id}` document keeps its `text`, `createdAt`, `updatedAt` and optional `deletedAt`. A new `record` map contains:

```ts
{
  version: 1,
  capturedAt: string, // UTC ISO 8601, immutable
  wordings: {
    [sha256]: { body: string, at: string, kind: 'capture' | 'correction' | 'legacy', parents: string[] }
  },
  continuation: { continuesId: string, linkedAt: string, origin: 'explicit' | 'accepted_suggestion' } | null
}
```

A wording key is SHA-256 of UTF-8 `JSON.stringify([fragmentId, body, at, kind, sortedParents])`. Dates are canonical UTC ISO strings. The snapshot contains every ancestor. Keys, dates, ancestry, identity, continuation origin and size are validated before use. Immutable nodes are merged by union. A correction names the wording it supersedes; a deliberate resolution names all known heads. Capture dates never become edit dates.

A causal descendant can replace the currently shown wording. Concurrent branches keep the local wording and surface the existing “both kept” choice. If several new descendants arrive together, the local wording stays until that choice. A resolution cannot consume a branch that was unknown when it was prepared. Replayed ancestors do not roll a later correction back. Both clients retain the full graph even though the existing conflict surface compares two wordings at a time.

A text edit from an older client that has no known version is retained as an independent `legacy` root. If authored metadata later supplies exactly those words and an entirely authored ancestry, the provisional legacy root becomes an alias for that known version; descendants retain the supplied ancestry without changing immutable nodes. Equal known heads are joined by the next authored correction. Its historical edit time and ancestry are otherwise unknown; it is never described as a reconstructed correction. The legacy `text` field remains a readable current wording for older clients.

## Durable local work

SQLite migration 8 on mobile and migration 7 on desktop add `record_sync_state`, `record_sync_links` and a one-row `record_sync_account` UID marker. Existing tables, Entry shapes and commands remain. Writes mark a generation dirty in the same database operation; network work happens later. Preparation reads a snapshot, hashes outside the database queue/lock, then commits only if that generation and local state still match. A successful upload acknowledges only its own generation. Restarting or going offline preserves pending work.

A stored sync UID activates all live metadata for a different signed-in account; it does not change the Firebase identity or link accounts. Untried and recently authored material precedes failed histories; failed sends rotate oldest attempt first. Failures remain pending and appear in the existing sync surface. There is no network request in capture.

A link may arrive before its parent. It waits durably for real endpoints, with no placeholder moment. Removed endpoints and cycles cannot become applied links. Two devices can continue the same parent: both children belong to that Line. Each child retains its own capture time.

## Transport and compatibility

A Firestore transaction reads the current entry, merges immutable nodes, preserves unknown legacy text and writes the merged metadata and legacy projection. A transaction retry recomputes the merge. It checks that the signed-in session is still current before publishing. A remote tombstone wins over active writes; this client never clears `deletedAt` during an upsert.

The initial backfill still pages by `createdAt`, 500 at a time, until exhausted or cancelled. Live entry updates are ordered by `updatedAt`, so a correction to an old moment enters the listening window. Old documents without `updatedAt` still arrive through backfill; supported writers supply it on subsequent updates. Existing tombstone catch-up remains in place.

Older clients continue to read text and ignore `record`. They do not acquire continuity features, and old binaries can still have their own older deletion behaviour. This is wire compatibility, not a promise that an older binary can open a newer local schema. Neither local migration drops the legacy tables.

Packets are limited to 400,000 UTF-8 JSON bytes, 2,048 versions and 32 parents per wording. These are conservative application limits, not Firebase quotas. A larger history stays local and pending, with an error in sync; it is never truncated. Separate paging/storage would be needed to lift the limit. Firestore's full document size also includes the legacy text and field overhead.

## Removal and privacy

Permanent removal scrubs the local metadata graph along with the existing wording, history and relationships. Suppression IDs prevent delayed metadata from restoring removed moments. Local parent removal also clears pending link references. As with existing text sync, the cloud document is marked removed; its text and metadata remain in cloud storage until cloud account deletion. Audio remains on its original device.

The mobile structured export includes `wording_versions` with selected ID and the full known version graph, including concurrent alternatives. It omits sync UID, retry state and errors. Current iPhone and Mac archives can be previewed and restored through the [portable archive flow](record-archive.md); older Mac text-only exports are not restorable.

## Verification and release checks

Automated checks exercise two real mobile SQLite databases, close/reopen recovery, CAS and acknowledgement races, concurrent branches and continuations, deletion suppression and Firestore transaction retries. A committed JSON fixture generated by the shared TypeScript protocol is consumed by Rust SQLite tests; Rust emits actual parent/child snapshots that the mobile test suite receives. Set `CHINOTTO_NATIVE_SYNC_FIXTURE` to the same temporary file for those two runs.

Before shipping both clients:

1. Verify deployed Firestore rules accept the additive `record` field and transaction read/write under the existing UID, entitlement and device-revocation checks. No rules or production cloud configuration were changed in this implementation.
2. Verify `record` has a single-field indexing exemption: its large text/map fields are never queried. Confirm the `updatedAt` listener and legacy documents in the deployed project.
3. Run a signed iPhone ↔ Mac cloud round trip: capture, Continue, Correct, offline parallel edit, reconnect/restart, older-client read and remove during delayed upload. Local fixtures and mocked transactions do not replace this check.
4. Review sync/privacy copy in store materials before release. Do not claim audio transfer or end-to-end encryption.

Firebase references: [transactions](https://firebase.google.com/docs/firestore/manage-data/transactions), [order and limit queries](https://firebase.google.com/docs/firestore/query-data/order-limit-data), [quotas](https://firebase.google.com/docs/firestore/quotas), [indexing best practices](https://firebase.google.com/docs/firestore/best-practices).
