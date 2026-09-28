# Mac App Review 3.0.0 window lifecycle audit

Review submission: `ea134ee7-5884-4c6f-a714-1a194e5c125c`

## Pre-fix build

- Audited on 2026-09-28 from the repository at `165679d`.
- The archived Apple Distribution-signed 3.0.1 app was not launchable on this Mac because its
  signature no longer verified. A fresh 3.0.2 MAS-feature build passed the repository audit, but an
  ad-hoc signature cannot launch the sandbox plus Sign in with Apple entitlement combination without
  a store provisioning profile. The same fresh MAS-feature app was therefore copied and ad-hoc
  signed without store entitlements for UI lifecycle inspection only. Store entitlements and signing
  remain part of the final package audit.

### Observed behavior

- Red close and Command-W both delivered `CloseRequested`; the handler prevented destruction,
  hid `main`, and left the application process running.
- The Chinotto application menu contained no reopen command.
- The standard Window menu contained Minimize, Zoom, and Close Window, but no reopen command.
- The tray's secondary-click menu contained `open chinotto`; selecting it called the existing
  restore helper. A normal left click opened the capture popover instead of the main window.
- Activating the application from the Dock delivered Tauri's `RunEvent::Reopen` and restored the
  main window.
- Command-Shift-K also called the restore helper before focusing capture. This worked but was not
  discoverable from either standard macOS menu.
- The app menu's Quit Chinotto and the tray's `quit` item terminated the process.

### Root cause

The lifecycle implementation itself already hid instead of destroying the main window and could
restore or recreate it. The failure was discoverability: the only visible reopen command lived in a
secondary-click status-item menu, while both standard macOS menus lacked an Open Chinotto item. A
reviewer who closed the window and inspected the application or Window menu had no standard action
to choose.

### Corrected behavior

The MAS build extends Tauri's standard macOS menu with Open Chinotto in both the Chinotto and Window
menus. Application-menu, Window-menu, tray, Dock, and global-shortcut reopen requests share one
idempotent path: create `main` only when absent, unminimize, show, activate the application, and
focus the window. The MAS tray labels are Open Chinotto and Quit Chinotto. The direct-distribution
build retains its previous default macOS menu and tray labels.

## Post-fix verification

- The direct-distribution build completed and launched with its existing menu and tray labels.
- The MAS 3.0.3 app and installer package passed the repository signing, entitlement, provisioning
  profile, bundle-version, and installer-signature audit.
- An audit-only copy made from the final MAS app was re-signed without store entitlements so its UI
  could be launched outside App Store installation. This copy is not uploadable and was used only
  for lifecycle inspection.
- Open Chinotto in both the Chinotto application menu and Window menu restored the main window after
  Command-W.
- Dock activation and Command-Shift-K restored and focused the main window after close.
- Three repeated close/reopen cycles retained a single application process.
- Quit Chinotto terminated the audit process.
- Unit coverage exercises close-to-hide, both the existing-window and missing-window reopen paths,
  repeated reopen idempotence, explicit quit, and MAS/direct menu separation.

The Apple Distribution-signed MAS app cannot be launched directly from an unpackaged local path on
this Mac; Launch Services rejects that execution mode before application code starts. Consequently,
microphone capture, transcription, native Sign in with Apple, sync, export, and account deletion were
not manually exercised in the final signed artifact. Their entitlements and package structure were
audited, and the full direct and MAS-feature automated test suites passed. No real account deletion
was attempted.
