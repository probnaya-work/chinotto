# Chinotto product improvement phases

Started 15 November 2026 following the product review and the owner's request to proceed in priority order. Scope: strengthen the current Record and add optional Face ID / Touch ID protection. Capture remains local, immediate and free of mandatory organisation.

Both repositories use `feat/product-phases-20261115`. Each phase produces its own implementation, regression coverage and verification evidence before the next is implemented. Shipping remains a separate release step. This document tracks work; it does not schedule a background worker.

| Order | Phase | Current state | Completion criterion |
|---|---|---|---|
| 1 | Trustworthy search and word evidence | Implemented and checked locally | No unrelated fallback suggestions; Unicode word evidence; accurate labels and highlights; existing tests pass |
| 2 | Record continuity between iPhone and Mac | Implemented and checked locally; cloud release checks pending | Continuation edges and wording history survive a two-device round trip, reconnect and concurrent edits; older clients remain compatible |
| 3 | First-week value and discoverable Find | Implemented and checked locally | One visible entrance to existing Find; evidence-backed Returns can be experienced during the first week; consistent policy on both platforms |
| 4 | Optional Face ID / Touch ID protection | Implemented and checked locally; hardware acceptance pending | Private history, widgets and app-switcher previews remain hidden until successful system authentication; new capture remains available |
| 5 | Restore and import | Own-archive restore implemented locally; device acceptance pending | A portable Record archive restores dates, stable IDs, histories, links and audio with a preview, validation and duplicate handling |
| 6 | Readability and public product explanation | Implemented locally; device visual acceptance pending | Phone reading works in daylight and enlarged text; website, manifesto and store describe the current Record |

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

### Phase 4 local result — 15 November 2026

Implemented on iPhone and Mac, off by default. OS authentication is explicit, uses the device-code/password fallback, and is required to change the setting. Backgrounding or closing the Mac window relocks the Record. Native session generations reject late authentication and private read results. Capture stays available and confirms saving without showing history; suspended Find, Correct and Continue wording is retained.

On iPhone, the surface receives a guarded Record store. UIKit covers the window before snapshots and hides its private accessibility tree; native audio stops on background while the existing recorder safely finalizes. The app-group preference clears widget preview data, reloads timelines and rejects queued preview writes. The widget itself also checks the flag. Share-in does not look up earlier encounters while locked. Export authenticates before preparation and is cancelled on relock.

On Mac, a default-deny native invoke boundary covers current and legacy commands in every window. New capture and write-only sync maintenance remain available. Commands returning existing Record text, including outgoing sync snapshots, wait until unlock; sync listeners restart on the access transition. AppKit covers windows before deactivation/minimization and hides the underlying accessibility tree until a matching safe render. The tray clears Return/count signals and keeps capture available. Exports remain temporary until their read session is rechecked, preserving any earlier export on cancellation.

Verification:
- iPhone: 121 Jest suites / 809 tests; TypeScript; unsigned Debug iOS Simulator build passed. The real Rust → mobile SQLite fixture is included. Targeted open-handle diagnostics also passed.
- Mac frontend: 25 Vitest files / 192 tests plus 14 Node tests; TypeScript and production frontend build passed. Browser fixture checked saving while locked and truthful protection copy.
- Mac native: 221 Rust tests in both direct and Mac App Store configurations passed. No packaged Mac app or release was produced.
- Physical-device acceptance remains required: Face ID / Touch ID, device-code/password fallback, no enrollment and lockout, cancelling a system prompt, leaving the app during a prompt, actual app-switcher snapshots, WidgetKit refresh, and main-window/tray behavior. Simulator builds and mocks do not establish those results.

Protection controls access through Chinotto. It does not encrypt SQLite, existing backups, exported copies or the cloud, and does not add or change a sync identity. The setting is local to each device.

## Phase 5

Start with restoring Chinotto's own export. Validate and preview before modifying the live Record; restore in a transaction and keep the existing Record recoverable. Preserve original dates and stable IDs. Import the same archive twice without duplicating moments or audio. Broader text imports follow after this round trip is reliable.

### Phase 5 local result — 15 November 2026

Settings on iPhone and Mac now offers a system file picker, validated preview and explicit
restore. New Mac exports include the full Record envelope alongside readable text. Imports
merge absent IDs, preserve capture dates/history/links/original M4A and CAF audio, skip current
or removed IDs, and retain a pre-import SQLite copy. A transaction and audio journal protect
against partial writes. Reimporting the same archive adds nothing. Outgoing sync generations
remain durable; accounts, device protection and app preferences are not imported.

The [archive contract and acceptance checklist](record-archive.md) documents compatibility,
resource limits, recovery locations and remaining device checks. Earlier iPhone version-1
archives are supported; older Mac text-only exports require a future plain-text import flow.

Local verification: 836 Jest tests; 195 Vitest and 14 Node tests; 229 Rust tests for direct and
Mac App Store configurations; both TypeScript checks; desktop frontend build; SDK 55 iOS
unsigned simulator build with ExpoDocumentPicker. Fresh iPhone → native Mac → iPhone archives
were parsed/restored in real SQLite, including wording hashes, chronology, history, links,
M4A and CAF metadata. Browser inspection confirmed the Settings entrance. Native picker and
physical playback/device authentication acceptance remain pending. No packaged Mac app or
release was produced.

## Phase 6

Review older-text compression, functional contrast, large text and a real phone light appearance. Retain the existing visual hierarchy.

Replace the landing example with a thought that develops across time. Remove descriptions of retired Spaces and time-strand surfaces. Explain what sync carries, what remains local, and what Pro unlocks.

### Phase 6 local result — 15 November 2026

The light appearance now has a warm mineral field, graphite material, stone rules and subdued
word evidence. The same palette is recorded in both repositories. Functional text uses at least
4.5:1 against the light field; low-contrast faint ink remains decorative. The ordinary dark
palette and baseline geometry are retained, with readable status and correction-history labels.

On iPhone, Settings offers dark/light and lifted contrast. The choice is local and persistent;
loading it never gates capture. Keyboard, status bar, Find, focus, history, sync, restore and
other overlays follow it without remounting the capture input or discarding a draft. At system
font scales of 1.3 and above, old moments use a wider face and no line clamp. Action targets can
grow and wrap. Text scaling remains enabled without a maximum multiplier.

On Mac, system/light/dark remain available. At 130–140% text size, older rows expand instead of
being squeezed back into a single line. Native window appearance/background follows the main
Record; the transparent capture panel stays transparent and refreshes its palette on opening.
The system-light contrast override no longer darkens a system-dark window. Native window APIs
are documented by [Tauri](https://v2.tauri.app/reference/javascript/api/namespacewindow/).

The website draft is in the separate `product-web-20261115` worktree. The maintained landing
specimen already develops a thought across two original dates, so its wording, dates and live
elapsed intervals are preserved. Current Record copy replaces retired strand/Spaces/shared-link
claims and explains continuity, cloud storage of words and their structure, device-local original audio and Pro for iPhone
sync (Mac sync remains free). The in-app iPhone manifesto matches. Each app has a release-ready
[store description draft](store-description.txt); none has been submitted or published.

Verification:
- iPhone: 127 Jest suites, 840 passed / 1 existing skipped; TypeScript; unsigned iOS Simulator
  Debug build passed. Theme tests retain the same capture instance and multiline draft across
  a switch, restore the choice after remount, reject late preference hydration, and cover old-text
  colour/large-text behaviour. Physical iPhone daylight, keyboard and largest accessibility-size
  visual acceptance remain pending.
- Mac: 28 Vitest files / 213 tests plus 14 Node tests; TypeScript and production frontend build
  passed. Native theme sequencing and transparent-host boundaries are mocked, not a signed app
  acceptance result. Browser inspection checked light settings/Record, dark baseline, long links
  and old text at 130%, with screenshots in the sibling `product-phase6-artifacts` directory.
- Website: production build, site/API TypeScript and four share API tests passed. Browser
  checks at 375px and 1440px found no horizontal overflow; the Cyrillic specimen and original
  dates remain intact. The source website's existing uncommitted changes were left untouched.

Screenshot impact: retain ordinary dark reading captures where their copy is still current.
Replace light-theme screenshots and iPhone Settings shots that need to show the new controls;
large-text shots deliberately show a different reading density. This is local implementation,
not a release, store submission or website deployment.

## Later candidates

Manual joining of existing moments, recent-line re-entry, Shortcuts/Action Button capture and local OCR stay candidates after these phases. They are not prerequisites for the current improvement cycle.

