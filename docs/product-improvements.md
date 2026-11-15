# Chinotto product improvement phases

Started 15 November 2026 following the product review and the owner's request to proceed in priority order. Scope: strengthen the current Record and add optional Face ID / Touch ID protection. Capture remains local, immediate and free of mandatory organisation.

Both repositories use `feat/product-phases-20261115`. Each phase produces its own implementation, regression coverage and verification evidence before the next is implemented. Shipping remains a separate release step. This document tracks work; it does not schedule a background worker.

| Order | Phase | Current state | Completion criterion |
|---|---|---|---|
| 1 | Trustworthy search and word evidence | Implemented and checked locally | No unrelated fallback suggestions; Unicode word evidence; accurate labels and highlights; existing tests pass |
| 2 | Record continuity between iPhone and Mac | Implemented and checked locally; cloud release checks pending | Continuation edges and wording history survive a two-device round trip, reconnect and concurrent edits; older clients remain compatible |
| 3 | First-week value and discoverable Find | Implemented and checked locally | One visible entrance to existing Find; evidence-backed Returns can be experienced during the first week; consistent policy on both platforms |
| 4 | Optional Face ID / Touch ID protection | Next, implementation requirements below | Private history, widgets and app-switcher previews remain hidden until successful system authentication; new capture remains available |
| 5 | Restore and import | Queued | A portable Record archive restores dates, stable IDs, histories, links and audio with a preview, validation and duplicate handling |
| 6 | Readability and public product explanation | Queued | Phone reading works in daylight and enlarged text; website, manifesto and store describe the current Record |

## Phase 1

Implemented:
- Preserve Unicode letters and combining marks, including Cyrillic and accented Portuguese.
- Keep English suffix rules from corrupting non-ASCII words; exclude common Russian and Portuguese function words from evidence.
- Never substitute arbitrary local entries for a search with no supporting words.
- Name mobile suggestions as shared-word suggestions; remove the cosmetic meaning switch.
- Show a real empty state after no candidates remain, including after rejection.
- Highlight phrases across punctuation without treating intervening Cyrillic words as separators.

Verification:
- Mobile: 114 Jest suites, 749 tests passed; TypeScript passed.
- Desktop: 19 Vitest files, 162 tests passed; 14 Node tests passed; TypeScript and frontend production build passed.
- On Node 24, run desktop tests as `NODE_OPTIONS=--no-experimental-webstorage npm run test:ts`, so jsdom supplies browser storage. The unmodified environment otherwise exposes Node's unavailable native localStorage to older Vitest.
- Original English prototype parity still passes.
- Native packages and device visual checks are not part of this verification. No app-store or production deployment has been made.

## Phase 2

Implemented: additive Record metadata, durable SQLite generations, Firestore transaction merging, continuation links and full authored wording ancestry. Legacy `{id, text, created_at}` readers and existing account identities remain. Concurrent offline corrections retain both wordings; equivalent early legacy projections do not create later false conflicts. Pending work survives restart, account changes and edits during a send. Permanent local removal scrubs metadata and blocks delayed resurrection.

Verification:
- Mobile: 119 Jest suites, 781 tests passed, including the real Rust → mobile SQLite round trip; TypeScript passed.
- Desktop: 22 Vitest files, 181 tests passed; 14 Node tests passed; TypeScript and production frontend build passed.
- Native desktop: 209 tests passed in both default and `--no-default-features --features mas` configurations. The installed app's local model was supplied through `MAS_MODEL_CACHE`; no model network download was required.
- Cross-runtime checks cover capture dates, correction history, child-before-parent delivery and complete Lines. Additional cases cover restart, concurrent branches, early legacy projection, CAS/acknowledgement races, queue fairness, account changes and deletion suppression.
- No signed app package or cloud deployment was made. Deployed security rules, the metadata indexing exemption and a signed iPhone ↔ Mac cloud round trip remain release checks in [Record continuity sync](record-sync.md).

The mobile export now retains every known concurrent wording in `wording_versions`. Original audio remains local. No restore/import feature is claimed in this phase.

Acceptance scenarios:
1. Capture on iPhone, continue on Mac, reopen on iPhone: both moments retain their own dates and one line.
2. Correct on either device: earlier wording remains available on the other device.
3. Concurrent offline correction: both authored wordings survive; no automatic discard.
4. A delayed metadata event cannot resurrect removed material.
5. Replayed and out-of-order events are idempotent; disconnect and restart keep pending changes.
6. Local capture and search work throughout migration and when sync is off.

Move authored structure first. Audio transfer needs separate retention, storage and bandwidth rules and must not be implicitly enabled by a metadata migration.

## Phase 3

Implemented:
- A visible `find /` action uses the existing capture field on iPhone and Mac. It preserves the exact unfinished draft, focuses the field and restores the draft with `‹ capture`, desktop Escape or Android Back. Clearing/removing a borrowed slash also restores the draft. Search queries never become saved moments.
- Returns can begin at seven full days. A real cause must be within the previous 48 hours; future captures, an anniversary or a quiet old Line alone never qualify. Triggers remain repeated language, the same source and a recent addition to an older Line.
- Shared timing: at most one open Return, at least 20 hours between new Returns, and 30 days before the same moment can return. Held/removed moments are excluded. Dismissal and open state survive remount/restart in SQLite.
- New Returns wait for a visible idle edge, after a 1.6-second settling period, and do not spend their cooldown behind Find, a draft or focus. Mobile re-evaluates after material changes. Existing open Returns are resumed and are withdrawn when the quoted evidence disappears.
- Both clients require a shared run of at least three words for repeated language. Candidate retrieval is bounded to the newest 32 acts within the cause window; desktop FTS probes at most 12 distinctive terms and retains its rare-word filter. Mobile prefilters shared triples before comparing full phrases. Retrieval ranking and source rendering still follow each platform's existing model.
- Mobile Continue now states `a new moment, dated now` before entry. Both platforms retain their correction explanation: wording changes, while the original date and earlier wording stay.

Verification:
- Mobile: 120 Jest suites, 793 tests passed; TypeScript passed. The real SQLite screen test covers a first-week Return after capture, remount, dismissal and deferral while searching/drafting.
- Desktop: 23 Vitest files, 185 tests passed; 14 Node tests passed; TypeScript and production frontend build passed.
- Native desktop: 215 tests passed. Shared JSON cases run through both the mobile model and Rust SQLite: seven-day and 48-hour boundaries, future dates, held material, insufficient overlap and absence of evidence.
- Browser story passed: multiline Cyrillic draft → visible Find → literal results → Escape → identical focused draft. Browser uses development fixture data; production selection/storage are covered by SQLite tests. No packaged app or device visual pass was run.

These thresholds are product choices under the approved phase, not evidence that every new record will have a useful Return on day seven. Silence remains valid. Face ID / Touch ID is next; no release has been deployed by this phase.

## Phase 4 Face ID and Touch ID

This phase is explicitly included following the owner's request. Build iPhone first, then Mac. It is optional and off by default.

### Access policy

- Protect reading the Record, Find results, continuations, held material, audio playback and export.
- Keep a blank capture field available while history is locked. Saving confirms success without exposing previous material. Authentication starts when the person asks to open the Record.
- Use the operating system's authentication prompt with its device-passcode/password fallback. No Chinotto PIN, biometric collection or cloud identity change.
- Authenticate before enabling or disabling protection. Failure or cancellation leaves the previous setting intact.
- Show one authentication request at a time. Cancellation, failure, unavailable hardware and lockout keep protected content hidden and offer an explicit retry.
- Relock after backgrounding and on cold launch. Authentication-prompt lifecycle transitions must not create a prompt loop.
- Preserve unsaved wording across authentication, cancellation and background transitions.

### Other surfaces

- Cover content before app-switcher snapshots; use native lifecycle handling where JavaScript cannot guarantee timing.
- Stop private audio playback on lock. A running voice capture must be safely completed or deliberately discarded, never silently lost.
- Home widgets become capture-only while protection is enabled. Clear existing preview data, reload timelines, and prevent background refresh from republishing thought text.
- Widget/deep-link routes, share-in, menu-bar capture and secondary windows must respect the same read boundary.
- Plain exports remain user-controlled copies. UI authentication is separate from database or cloud encryption, and product copy must describe it accurately.

### Implementation and verification

iOS integration: Expo SDK 55-compatible `expo-local-authentication`, with `NSFaceIDUsageDescription` in both app configuration and the committed native Info.plist. Its dependency is justified by OS-managed authentication and fallback; the existing stack has no authentication bridge for this purpose.

Mac integration: Apple LocalAuthentication through a native command; verify both the main window and tray capture. Gate private reads rather than merely drawing an overlay over readable content.

Required cases: enrollment absent, success, repeated failure, user cancel, system cancel, lockout, device fallback, concurrent requests, stale successful responses after relock, cold start, background/foreground, widget cache and export/deep-link bypass attempts. Face ID needs a signed development/device build; unit mocks cannot verify hardware authentication.

References: [Expo SDK 55 LocalAuthentication](https://docs.expo.dev/versions/v55.0.0/sdk/local-authentication/), [Apple LocalAuthentication](https://developer.apple.com/documentation/localauthentication).

## Phase 5

Start with restoring Chinotto's own export. Validate and preview before modifying the live Record; restore in a transaction and keep the existing Record recoverable. Preserve original dates and stable IDs. Import the same archive twice without duplicating moments or audio. Broader text imports follow after this round trip is reliable.

## Phase 6

Review older-text compression, functional contrast, large text and a real phone light appearance. Retain the existing visual hierarchy.

Replace the landing example with a thought that develops across time. Remove descriptions of retired Spaces and time-strand surfaces. Explain what sync carries, what remains local, and what Pro unlocks.

## Later candidates

Manual joining of existing moments, recent-line re-entry, Shortcuts/Action Button capture and local OCR stay candidates after these phases. They are not prerequisites for the current improvement cycle.

