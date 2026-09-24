# GitHub release notes (technical)

These sections feed the **GitHub Release** description in CI (see `.github/workflows/release.yml`). Use **factual / technical** bullets: short **topic label**, colon, then details (backticks for paths, crates, env names). **User-facing marketing copy** belongs on the website, not here.

Add **`## vX.Y.Z`** before you push tag **`vX.Y.Z`**.

## Unreleased — Mac App Store 3.0.0 (build 3.0.2); direct-download version not yet decided

- **Voice (privacy):** speech is recognised on the device only, in both the direct and Mac App Store builds — every task goes through `on_device_task` in `speech.rs`, which requires `supportsOnDeviceRecognition` and sets `requiresOnDeviceRecognition`; the direct build no longer falls back to Apple's speech service
- **Voice (retry):** recordings kept without words are read back on the device once each when local recognition becomes available (`transcript_retry.rs`, Tauri `retry_transcripts`); transcripts are labelled `apple-on-device` only when the capture reported it
- **Voice (removal):** a removed voice fragment's `.caf`, transcript, wordings, embedding, search entry and quoting Traces/Returns are erased a minute after removal (`db/erasure.rs`, Tauri `erase_removed_voice`); `restore_fragment` refuses past that point

## v3.0.0

- **Home stream:** bounded home layout with depth zone, time strand week river, and memory-echo resurfacing card (`TimeStrand`, `MemoryEcho`, `homeStreamPartition.ts`, `timeStrandWeek.ts`)
- **Entry detail:** read mode, continuation marking, expanded writing overlay, thought-trail strip navigation, related-thoughts rails, and share entry point (`EntryDetail.tsx`, `ThoughtTrailStrip.tsx`, `detailExpandedWriting`)
- **Readable text:** plain-text rendering for URLs and line breaks in stream and detail (`readablePlainText.ts`, continuation columns in `schema.sql`)
- **Thought trails:** improved keyword ranking, shared-term highlighting, trail marks in stream rows, and related-thoughts selection in share dialog (`keywords.rs`, `trailSharedTermHighlight.ts`, `thought_trail` commands)
- **Share threads:** local `share_threads` registry, HTML export, hosted snapshot upload to `getchinotto.app/t/{token}` (`shareThreadHtml.ts`, `shareThreadUpload.ts`, `ShareThreadDialog.tsx`, Tauri `create_share_thread` / `revoke_share_thread`)
- **Recall themes:** user theme CRUD, manual picker in detail, search theme chips, link-based classification, settings editor (`user_themes` table, `themeRepository` paths in `db/mod.rs`, `themeSettings.ts`)
- **Sync (themes):** push/pull `users/{uid}/user_themes` and optional entry `theme` field over Firestore when recall themes are enabled (`desktopFirestoreSync.ts`, `firestoreSyncTheme.ts`, `userThemeFlush.ts`, `themeSyncBackfill.ts`)
- **UI:** compact stream row typography; jump calendar as sheet; intro screen simplification; semantic surface tokens for chrome and search
- **Docs:** maintainer specs removed from the public git tree (`docs/internal/` local-only via `.gitignore`); README, architecture, and privacy updated for themes, sharing, and sync scope

## v2.2.3

- **UI (background):** reduce visible banding in the ambient cosmic glow (`--bg-fade`, `oklab` gradient interpolation, eased radial stops in `src/index.css`); remove ineffective grain overlay on `.app-bg`

## v2.2.2

- **Thought trails (keywords):** keep two-letter acronyms such as `AI` in tokenizer output (`MIN_LEN` 2 in `src-tauri/src/keywords.rs`) so related entries can link on shared short tokens; two-letter grammatical noise stays in `STOPWORDS`

## v2.2.1

- **UI (entries):** add explicit delete confirmation before removing a thought (`confirmDeleteThought` in `src/lib/deleteEntryConfirmation.ts`, wired in `App.tsx`) for both row delete click and keyboard delete flow
- **Related thoughts (indexing):** lazy backfill of missing embeddings during `find_similar_entries` (`ensure_embeddings_for_related_search` in `src-tauri/src/lib.rs`) so older rows can appear in related results without manual reindex
- **UI (zoom):** persist and restore webview zoom with shortcut controls (`src/lib/uiZoom.ts`, startup `applyStoredUiZoom` in `App.tsx`)
- **Tests:** add coverage for delete confirmation copy/confirm decisions (`src/lib/deleteEntryConfirmation.test.ts`) and keep `uiZoom` behavior covered (`src/lib/uiZoom.test.ts`)

## v2.1.1

- **Sync (release):** CI requires `VITE_FIREBASE_API_KEY` and `VITE_FIREBASE_PROJECT_ID` Actions secrets before building; embeds all `VITE_FIREBASE_*` env in the packaged SPA so `isFirebaseSyncConfigured()` is true (see `docs/internal/sync.md`)
- **DMG (launch):** keep `**Chinotto.developer-id.entitlements`** for notarized Developer ID builds (no SIWA in signed plist — avoids **AMFI / error 163**)
- **Sync (DMG):** packaged **Continue with Apple** opens **Firebase Hosting `/chinotto-oauth`** in the system browser and **form-POSTs** credential to `**127.0.0.1` bridge** (not `**native_apple_sign_in`**; requires current `**deploy:hosting`**)
- **Sync (DMG):** one-shot upload of pre-sync local SQLite entries to Firestore on first sign-in per uid (`startLocalEntriesFirestoreUploadOnAuth` in `desktopFirestoreSync.ts`) so mobile ingest receives desktop history
- **Sync (UX):** clearer sync start errors in `useAppleSyncOAuth.ts`

## v2.1.0

- **Spaces:** stream lens tabs (All, Inbox, Work, Personal) with per-scope capture destination, `space_id` on entries, detail lens picker, and `list_spaces` / filtered `list_entries` / search / jump-to-date in `src-tauri/src/db/mod.rs` and `App.tsx`
- **Ambience:** per-space cool–warm slider (`src/lib/spaceAmbience.ts`, `SpaceAtmosphereAffordance` / `SpaceAtmospherePopover`); room tone on `--bg` and ambient CSS vars; neutral plateau and symmetric cool pole; `chinotto.spaceAmbience` localStorage (migrates legacy `chinotto.spacePalette`)
- **Empty stream:** full onboarding only on All before first save; scoped lenses use quiet one-line empty copy (`src/lib/emptyStreamLensMessage.ts`)
- **Jump to date:** show **Back to now** when the stream is scrolled below the top (`useStreamBackToNowVisible`, `jumpContextScroll.ts`); sticky pill layout without shifting the stream
- **UI:** remove hardcoded per-scope theme colors from CSS; lens chrome and jump-date / ambience popovers reposition on resize and scroll

## v2.0.0

- **Sync (optional):** bidirectional Firestore sync with Chinotto mobile when `VITE_FIREBASE_*` is set — `src/lib/desktopFirestoreSync.ts`, `SyncModal.tsx`, `useAppleSyncOAuth.ts`, `firebaseConfig.ts`, SQLite tombstone outbox + ingest paths in `src-tauri/src/db/mod.rs` / `schema.sql`; contract in `docs/internal/sync.md`
- **Auth:** packaged macOS **Continue with Apple** via `native_apple_sign_in` (`src-tauri/src/native_apple_sign_in.rs`) + Firebase; dev OAuth loopback + `OAuthBridge`, `oauth_dev_bridge.rs`
- **Sync UX:** desktop sync gate / header CTA (`useDesktopSyncHeaderCta.ts`), saved-entry text push after edits (`syncSavedEntryTextToRemote.ts`), session teardown on lost Firestore access (`invalidateFirebaseSyncAfterRemoteSessionLost`, `clear_sync_tombstone_outbox_all`)
- **Firebase Hosting:** static landing + OAuth surface (`firebase.json`, `public/index.html`, `HostingDesktopOnly.tsx`)
- **Distribution:** App Store listing URL `CHINOTTO_MAC_APP_STORE_URL` (`src/lib/chinottoLinks.ts`), README / AGENTS copy for end-user links
- **Docs:** maintainer-only MAS/TestFlight runbook removed from public tree; README oriented for observers; `docs/internal/sync.md` packaged OAuth + troubleshooting (**RBS / Launchd job spawn failed**) notes
- **Notarized DMG (Developer ID):** `tauri.conf.json` → `bundle.macOS.entitlements` = `**Chinotto.developer-id.entitlements`** (microphone + audio-input only; no `**com.apple.developer.applesignin`** in the shipped plist — avoids AMFI refusing launch on some macOS versions while `**spctl**` still passes). Full SIWA plist stays in `**Chinotto.entitlements**` for `**scripts/codesign-macos-dev.sh**`. `**scripts/sign-macos.sh**` uses the same `**Chinotto.developer-id.entitlements**` for adhoc `-` signing.
- **macOS builds:** MAS merge entitlements helper and scripts (`scripts/build-mas-testflight.sh`, `Chinotto.mas.entitlements`, `tauri.mas-build.json`); codesign dev path (`scripts/codesign-macos-dev.sh`)
- **Tooling:** `.cargo/config.toml` macOS deployment target 12.0; tray/popover and main-window behavior retained from 1.x

## v1.3.1

- **Desktop lifecycle:** closing `main` via window chrome calls `prevent_close()` and hides the window (`src-tauri/src/lib.rs`) — process keeps running so tray capture stays available; Dock reopen (`RunEvent::Reopen`) and global capture shortcut recreate or focus `main` via `ensure_main_window_focus`
- **Main window:** desktop `setup` ensures `main` is unminimized, visible, and focused after launch (`src-tauri/src/lib.rs`) — restores expected visibility after in-app updater relaunch on macOS
- **Updater / macOS:** Tauri feature `process-relaunch-dangerous-allow-symlink-macos` in `src-tauri/Cargo.toml` — default relaunch guard skips spawning when cached executable paths include symlink ancestors (in-app “Restart” after update could exit without reopening); see `docs/internal/updater.md`
- **Empty onboarding / showcase:** `StreamFlowPanel` visuals clipped to panel bounds (`overflow: hidden`), softer blurred blobs and glass, tuned SVG stroke gradient stops (`src/index.css`, `src/components/StreamFlowPanel.tsx`); slightly reduced showcase card glow (`--chinotto-glow-`* under `.stream-showcase-overlay`)
- **Dev-only:** preview empty-stream onboarding without deleting data — `src/lib/devPreviewEmptyStream.ts`, `refresh()` short-circuit when flag set; Developer menu + header control in `App.tsx` (stripped in production builds)
- **Icons / bundle:** `icon.svg` uses `clipPath` for the outer rounded plate (`rx=22`) so the raster is not a solid square tile; `scripts/flatten_icon_png.py` masks `qlmanage` white corners with the same plate radius and mates fringe onto `#0a0a0e`; regenerated `icon.icns`, `macos/AppIcon.appiconset`, and PNGs via `scripts/generate-macos-app-icons.sh`; `docs/internal/app-icon.md` updated

## v1.3.0

- **Thought detail editing:** in-detail text editing on by default; debounced `update_entry` from `App.tsx`; continuation line break handling in `EntryDetail.tsx` (`beforeinput` / `setRangeText`); back-only exit from detail when editing
- **Persistence:** `entries.updated_at` column + migration in `src-tauri/src/db/mod.rs` / `schema.sql`; `update_entry_text` sets `text`, `updated_at`, and `edit_count`
- **Stream UI:** single-line row preview with ellipsis; domain badge omitted in stream row variant where applicable (`src/index.css`, `EntryStream.tsx`)
- **Analytics:** `entry_text_saved` with `source` (`detail`  `stream`) and `text_length` after successful saves (`src/lib/analytics.ts`, `App.tsx`, `docs/internal/analytics-design.md`)
- **Product chrome:** removed beta symbol from header (`App.tsx`, `src/index.css`)
- **Tests:** `update_entry_text` coverage in `src-tauri/src/db/mod.rs`

## v1.2.0

- **Menu bar tray polish:** improved capture popover stability by debouncing hide on focus loss and keeping main stream refresh behavior reliable after tray saves (`src/features/entries/TrayCapturePanel.tsx`, `src/App.tsx`)
- **Jump to date:** added calendar jump flow with `jump_dates_in_month` / `jump_anchor_for_local_date` commands, month dots for local dates with entries, and stream scroll-to-date in `src/features/entries/JumpToDatePopover.tsx`, `src/features/entries/entryApi.ts`, and `src/App.tsx`
- **Stream grouping:** replaced older catch-all grouping with per-day section headers (Today, Yesterday, and explicit calendar dates) in `src/features/entries/EntryStream.tsx`
- **Jump context UX:** added sticky right-aligned jump context with `Back to now`, auto-collapse label timing, and auto-clear when returning near top or switching modes (`src/App.tsx`, `src/lib/useJumpContextAutoClear.ts`, `src/index.css`)
- **Jump scroll polish:** aligned date-jump scrolling to section containers with explicit top offset via `src/lib/scrollJumpSectionIntoView.ts` and `.stream-section` `scroll-margin-top` in `src/index.css`; disabled jump-calendar click affordance for current day when already at stream top
- **Analytics:** added opt-in jump funnel events (`jump_to_date_calendar_opened`, `jump_to_date_completed` with `days_ago`, `jump_to_date_back_to_now`) and docs in `src/lib/analytics.ts` / `docs/internal/analytics-design.md`
- **Tests:** added jump date coverage for scroll-dismiss logic and analytics day metric (`src/lib/jumpContextScroll.test.ts`, `src/lib/useJumpContextAutoClear.test.tsx`, `src/lib/jumpDateDaysAgoMetric.test.ts`) plus Rust DB tests for jump date commands

## v1.1.0

- **Menu bar tray:** added native tray integration in `src-tauri/src/tray_capture.rs` with left-click quick capture toggle and template icon handling on macOS
- **Quick capture popup:** shipped dedicated `capture-popover` UI (`src/features/entries/TrayCapturePanel.tsx`, `src/main.tsx`, `src/index.css`) as a compact single-row input with secondary open-app action
- **Platform wiring:** enabled tray/macOS private API features, added transparent popover window config in `src-tauri/tauri.conf.json`, and updated `core:window` permissions in `src-tauri/capabilities/default.json`

## v1.0.0

- **Empty stream / onboarding:** `StreamFlowPanel` + progressive empty-stream onboarding in `EntryStream.tsx`; `hasEverSavedThought` / `streamOnboarding.ts`; capture `onDraftChange` wiring; intro gradient headline, proceed hint, press-any-key copy; design-system + recall guardrail doc updates
- **Search / catalog:** `entryCatalogPresence.ts` consolidates full-list load and search-trigger rules; hide inline ⌘K control when the catalog is empty; capture row layout stable when search UI toggles; search placeholder uses “thoughts” wording; tests for `getSearchFeedback` and shortcut visibility
- **Resurface:** `mayAttemptResurface` / session guards — resurface when the main view is ready (not tied to save); `docs/internal/recall-guardrails.md` aligned; Rust `get_resurfaced_entry` reason copy softened (`temporal_reason_anchor`, `format_ago` → “From …” style)
- **Welcome preview:** `StreamShowcaseModal` replays welcome-style stream preview; links to manifesto / changelog; analytics `stream_showcase_opened`
- **UI:** Modal / showcase card glow tuned in `index.css`; header `chinotto-logo` hover styling; β only on header title, not on Settings footer line (`App.tsx`, `ChinottoCard.tsx`)
- **Shortcuts / keys:** `src/lib/keyboardLabels.ts` — `ENTER_KEY_GLYPH` (↵) in Settings (`ChinottoCard`) and empty onboarding hint; empty hint adds ⌘N / Ctrl+N (via `userAgent`) to focus capture; `Enter` a11y via `aria-label` / `title` where the glyph is used alone
- **Late edit:** `EntryStream` textarea `onBlur` ends late edit when focus leaves the row (discard like Esc) — fixes odd ⌘E / focus after double-click then click-away
- **Analytics:** `app_version` included on event payloads (`src/lib/analytics.ts`)
- **Tauri / Rust:** Unused `Db` query helpers removed from `src-tauri/src/db/mod.rs`; `thought_trail` compiled only under `#[cfg(test)]` in `lib.rs`; tests unchanged for recall / thought-trail ranking
- **Icon / bundle:** Opaque full-canvas icon + optional Pillow flatten (`scripts/flatten_icon_png.py`, `scripts/generate-macos-app-icons.sh`); macOS `AppIcon.appiconset` / `icon.icns` (`docs/internal/app-icon.md`); light `dmg-background.png` + `scripts/generate-dmg-background.py`; `bundle.macOS.dmg.background` in `tauri.conf.json`
- **Thought detail & copy:** Related-thoughts block hierarchy and empty state (**No connections yet**); back control; primary UI uses **thoughts** in aria/search/stream (`EntryDetail`, `EntryStream`, `SearchInput`, `searchOverlayFeedback.ts`); “Related thoughts” vs thought-trail docs alignment
- **Tooling / docs:** `.gitignore` `/target/` for repo-root Cargo output; `docs/github-release-notes.md` + `scripts/extract-release-notes.py` release-body flow; `docs/internal/updater.md`; `fix-dmg-bundler` / `DMG_BUNDLING_FIX.md` (DMG script default name from `tauri.conf.json`)
- **Tests:** `streamOnboarding`, `EntryStream` empty onboarding branches, extended `mayAttemptResurface` cases

## v0.2.1

- **Updater:** `@tauri-apps/plugin-updater` + `plugin-process`, `check()` on prod launch, `UpdateNudge` UI; `tauri.conf.json` endpoints + minisign pubkey; `createUpdaterArtifacts` for signed bundles
- **CI:** `.github/workflows/release.yml` — tagged aarch64 macOS build, `TAURI_SIGNING_PRIVATE_KEY` required; optional Developer ID + App Store Connect API key for codesign/notarization; `workflow_dispatch` for manual runs
- **Release packaging:** release body from `docs/github-release-notes.md` via `scripts/extract-release-notes.py`; stable download asset `Chinotto_macOS_aarch64.dmg` uploaded alongside versioned DMG (`docs/internal/updater.md`)

## v0.2.0

- **Quick capture & feedback:** global shortcut (⌘⇧K), in-app feedback entry
- **Entries stream:** hide empty-state when only pinned entries are present
- **Settings:** edit shortcut listed on shortcuts screen
- **Entries interaction:** double-click prefers edit over open
- **Settings UI:** shortcut tile hover contrast
- **Release & docs:** version 0.2.0 bump; macOS signing/notarization guide + `docs/release-macos` gitignore churn

## v0.1.0

- **Initial macOS build:** capture, reverse-chronological stream, SQLite + FTS search, minimal shell