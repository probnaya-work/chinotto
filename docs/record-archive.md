# Portable Record archives

Implemented locally in phase 5, 15 November 2026. This describes the full Record archive,
not an app installation backup. It does not restore accounts, purchases or device settings.

## Using an archive

In Settings, choose **restore from archive** and select a Chinotto ZIP from Files on iPhone
or the system file picker on Mac. Everything works offline. Chinotto validates the archive,
stages and verifies its original recordings, then shows the dates, sample wording and counts
of new moments, recordings, preserved legacy items, duplicates and missing relationships.
Nothing is added until **restore** is pressed. Leaving Settings cancels the pending review.
When device protection is enabled, preparing an import requires system authentication.
A relock invalidates its preview. Preparation and restoration check permission throughout
and immediately before committing; a completed transaction is kept.

Restoration adds absent stable IDs. It keeps the original capture dates, provenance, revision
order, continuation edges, held state, encounters, enrichment, transcript state and original
audio. It preserves all known wording versions, including concurrent and unsent corrections.
It rebuilds Line and Find indexes. Imported text enters the ordinary durable sync path if
sync is enabled; the archive never enables sync or changes accounts. Audio stays local.

An ID already present is skipped, even when its archive wording differs: the existing Record
keeps its current wording and history. Previously removed/suppressed IDs are also skipped.
The same ID with a different capture instant is rejected. Reimporting the same archive creates
no extra moments, recordings or recovery database. Missing continuation predecessors stay
pending; they join when their real moment arrives. Trace choices for absent targets are
reported and skipped. No placeholder moments or guessed histories are created. Restoring
more than five active holds is rejected with the number to release first.

A relevant Record change after the preview requires a fresh review. Before a nonempty import,
Chinotto saves a consistent SQLite recovery copy. The import itself uses one transaction;
existing rows and recordings are never overwritten or removed. Audio receives generated
filenames, independent of archive paths. A write-ahead file journal lets startup cleanup remove
only uncommitted restore audio and retain committed audio after a crash. It runs before orphan
recording recovery. Temporary staging is removed after use or retried at the next launch.

## Format and compatibility

ZIP contains these files under `chinotto-record/`:

| File | Contents |
|---|---|
| `manifest.json` | `{"format":"chinotto-record","version":1}` |
| `record.jsonl` | One version-1 canonical moment and its child rows per line |
| `archived.jsonl` | Stable IDs, dates and wording of preserved legacy material |
| `record.txt`, `README.txt` | Readable copies usable without Chinotto |
| `audio/` | Original M4A or CAF bytes referenced by metadata |

The allowlist is `record/archiveFields.json` on mobile and
`src-tauri/src/db/archive_fields.json` on Mac. Numeric child row IDs and filesystem paths are
local implementation details; imports regenerate them. Optional `wording_versions` contains
only its selected content hash and the validated ancestry graph, never sync credentials,
account state or upload bookkeeping. Mac exports omit absolute recording source paths.
Missing audio must already be explicitly marked missing; a listed recording that is absent,
truncated or fails its ZIP checksum rejects the archive.

Earlier iPhone version-1 ZIPs without a manifest are accepted. Earlier Mac exports with only
readable text/audio cannot faithfully restore IDs and history and are rejected with an
explanation. Broader plain-text imports remain a separate follow-up; no dates or IDs are
inferred from those old exports.

Imports currently accept stored/DEFLATE ZIPs up to 2 GiB compressed and expanded, 32 MiB per
metadata file, 2 MiB per JSON line and 100,000 moments/legacy items. Mobile requires standard
ZIP32 (no ZIP64 or multipart archives). Canonical fields and ancestry have additional bounded
sizes. Traversal paths, encrypted files, symlinks, duplicate paths/IDs, unsupported versions,
invalid fields and continuation/wording cycles are rejected. These are resource limits, not
an instruction to truncate data. Free disk space must fit staging, new audio and a database
copy; failed writes roll back. Archive contents are private and unencrypted.

Recovery databases stay on the device: `Documents/chinotto/recovery/before-restore-*.db` on
iPhone; `recovery/before-restore-*.db` beside `chinotto.db` in Mac app data. Existing audio stays
in its original location. These copies are not encrypted by Face ID / Touch ID and are not
synced. They require manual database recovery; there is no one-click rollback of a completed
import. Standard Mac rolling backups retain their existing policy.

## Verification and acceptance

Real SQLite tests cover original IDs/dates, revisions, sources, links, duplicate imports,
removed IDs, stale previews, cancellation, failed audio writes, and crash-journal cleanup.
Tests also cover invalid ZIP paths/checksums/metadata and UI confirmation/cancellation.
Each repository checks in a small synthetic archive produced by the other implementation so
compatibility runs in ordinary CI. Its audio bytes are test payloads, not playable recordings.

For a fresh implementation-to-implementation round trip, run the mobile export test with
`CHINOTTO_ARCHIVE_FIXTURE_OUTPUT`, native tests with `CHINOTTO_MOBILE_ARCHIVE_FIXTURE` and
`CHINOTTO_NATIVE_ARCHIVE_OUTPUT`, then mobile restore tests with
`CHINOTTO_NATIVE_ARCHIVE_FIXTURE`. The local acceptance run also includes native CAF audio.

Still required before release: signed iPhone Files/iCloud selection; real M4A/CAF playback;
signed sandboxed Mac open-panel permissions; large archives/low-storage on real devices;
background/relock/cancel at the picker and during a restore; forced process termination before
and after the transaction commit. Automated tests and the unsigned simulator build do not
claim to replace these device checks. No release is published by this phase.
