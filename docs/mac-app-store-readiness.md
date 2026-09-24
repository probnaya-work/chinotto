# Mac App Store release readiness

This is the repository-side release gate for Chinotto's macOS App Store build. It does not authorize
an upload, App Store Connect metadata changes, TestFlight distribution, or App Review submission.

## Distribution variants

Direct distribution remains the default Cargo feature and `npm run tauri` path. It retains the transparent tray popover,
GitHub updater, loopback Apple OAuth fallback, first-use meaning-model download, and Developer ID
release path.

The `mas` Cargo feature plus `src-tauri/tauri.mas-build.json` is the store variant. It:

- compiles without Tauri's `macos-private-api` and symlink-tolerant updater relaunch features;
- replaces the direct build's updater/process/loopback IPC grants with a store-only capability set;
- makes the tray capture window opaque instead of using private WebKit transparency APIs;
- enables App Sandbox with only outgoing network, user-selected file, microphone/audio-input, and
  Sign in with Apple entitlements;
- uses native AuthenticationServices instead of opening a loopback network listener;
- disables GitHub update checks and updater actions in the UI;
- exports through a save panel;
- permits speech transcription only when macOS supports on-device recognition (as the direct build
  now does too); and
- loads the meaning model only from the signed app bundle.

## Repository gate

Before a credentialed package is considered upload-ready, all of the following must pass:

1. `npm run typecheck`
2. `npm test`
3. `cargo check --no-default-features --features mas` in `src-tauri/`
4. `npm run build:mas`
5. `scripts/audit-mas-bundle.sh <app> <pkg>`

The audit verifies identifiers and versions, minimum OS/category/export compliance, arm64, privacy
manifest and notices, the pinned model checksum, absence of Tauri's private WebKit markers, App
Sandbox entitlements, absence of incoming-network and debug entitlements, app code signature, icon,
and installer signature.

`npm run build:mas:prepare` performs the same build with an ad-hoc signature when distribution
credentials are unavailable. That archive is for inspection and sandbox testing only; it cannot be
uploaded.

## Capabilities and behavior audit

| Area | Store disposition |
| --- | --- |
| Bundle ID | `app.chinotto`; must match the existing App Store record and profile. |
| Marketing version | Kept aligned across npm, Tauri, Cargo, and Info.plist. |
| Build version | Defaults to the repository version; override with `MAS_BUILD_NUMBER` for replacement uploads. It must increase relative to App Store Connect. |
| Architectures | arm64, minimum macOS 12.0. Universal Intel support is not currently promised. |
| Sandbox | Required and enabled only in the MAS entitlements. Direct builds remain unsandboxed. |
| Network client | Required for optional Firebase sync, sharing, analytics, URL enrichment, and Apple services. |
| Network server | Removed from MAS; the native Apple sign-in flow does not bind localhost. |
| Files | Database, backups, audio, and preferences stay in the app container. Export uses a save-panel grant. |
| Microphone | Both sandbox and hardened-runtime audio entitlements are present. The app shows a recording surface and requests permission only on use. |
| Speech | Usage description is present. Recognition is on-device-only in both builds (`speech.rs` `on_device_task`); recordings remain available when transcription is unsupported, and are read back locally later. |
| Updates | MAS UI and runtime do not invoke Tauri's GitHub updater. Store updates must come only from the Mac App Store. |
| Meaning model | Pinned model data is embedded before signing; the MAS runtime refuses a missing bundle cache instead of downloading it. |
| Menu bar | NSStatusItem behavior is public API. Close hides the main window; Dock reopen restores it; the tray menu provides Quit. Verify global shortcuts and the opaque popover in a signed sandbox smoke test. |
| Icons | Complete 16–1024 macOS icon set and ICNS are present. Final appearance still needs human review in Finder, Dock, permission prompts, and App Store artwork. |
| Privacy | Usage strings, public privacy document, and `PrivacyInfo.xcprivacy` are present. App Store Connect privacy answers remain a separate manual task. |
| Encryption | `ITSAppUsesNonExemptEncryption` is false; only exempt standard platform/TLS cryptography is intended. Confirm this legal classification before submission. |

## External and product decisions still required

- Install valid Apple Distribution and Mac Installer Distribution identities and provide a current
  Mac App Store Connect profile for `app.chinotto`. The profile must authorize Sign in with Apple.
- Confirm the next unused build number in App Store Connect before creating the package.
- Run the signed app from a clean macOS user/container and test first launch, close/reopen/quit,
  tray capture, both global shortcuts, microphone allow/deny/re-enable, on-device transcription,
  native Apple sign-in, offline core capture/search, sync, sharing, URL enrichment, save-panel export,
  backup retention, icon variants, account deletion, and update wording.
- Decide the direct-to-store data migration experience. App Sandbox moves data to
  `~/Library/Containers/app.chinotto`; an existing direct installation's database is not imported
  automatically. The current safe path is export from the direct build, but Chinotto does not yet
  import that archive.
- Sync entitlement model confirmed: the desktop app enables sync from Sign in with Apple alone and
  has no purchase flow and no dependency on any mobile-established entitlement (Guideline 3.1.3(b)
  does not apply — Mac never gates a feature behind an out-of-app purchase). Core capture/search and
  optional sync are both free on Mac; Chinotto Pro is an iOS-only purchase that unlocks sync on iOS.
- Reconcile App Store Connect privacy answers with the manifest: optional linked email/user ID,
  linked install/device information, linked user content for sync/sharing, and unlinked opt-in product
  interaction analytics. Confirm Firebase, hosting, Umami, and Apple speech-processing practices.
- Supply or confirm the privacy-policy URL, support URL, category, age rating, screenshots, app
  description, review notes, test account/instructions if required, and encryption answers.
- Include review notes explaining the menu-bar lifecycle, global shortcuts, microphone recording
  indicator, local-first/offline behavior, optional sync, and how to reach account deletion.
- Validate the signed `.pkg` with Apple's current tooling. Do not upload it or alter App Store Connect
  without explicit approval.
