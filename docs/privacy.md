# Privacy

Chinotto is **local-first**. Your thoughts stay on your device in a SQLite database. Capture and search work without any network connection.

## Your data

- **Record text** is stored locally by default. With optional sync, text, earlier wordings and continuation links are also stored in the synced copy. It is not sent to analytics or included in crash-style telemetry.
- **Voice recordings** are stored in Chinotto's local app data and are not synced. Both the Mac App
  Store and the direct-download builds transcribe only on-device: when macOS has no on-device
  recogniser for the language, or speech recognition is not allowed, the recording is kept without
  a transcript and nothing is sent anywhere. It is read back on this Mac later, once, if on-device
  recognition becomes available. Time Machine may include the app's local data.
- **Removing a moment** erases its wording, history, relationships and derived material after
  the eight-second undo period. A voice moment's local recording and transcript go too. Backups you make yourself and exports are yours
  and are not changed.
- **Export** — Export the record writes a portable ZIP with readable text, history and original audio to a location you control. Its Record can be restored on iPhone or Mac; accounts and device settings are not included.
- **Backup** — automatic local backups in `chinotto-backups/` (last 7 kept).
- **Meaning model** — the Mac App Store build includes its local meaning model. The direct-download
  build downloads the same model on first use and then keeps it in local app data.

## Optional sync

When you enable sync and sign in with Apple, Record text, earlier wordings and continuation links can sync with Chinotto on iPhone via
Firebase. Chinotto also stores an install
identifier and the Mac name you see in the device list. Voice audio is not synced. Sync is optional;
the app remains fully usable without it.

Removing a synced moment marks the cloud copy as removed; its text and Record metadata remain there until the cloud
account is deleted. Removing the cloud account deletes the synced copy and leaves local records on
your devices.

## Analytics (opt-in)

Analytics are **off by default**. You can enable them in Settings → privacy → **anonymous usage**.

When enabled, Chinotto sends only simple event names and numbers — for example “entry created” with the text length, or “search used” with the number of results. It never sends:

- the text of your thoughts
- your search query
- personal identifiers

Analytics help understand how the app is used. You can turn them off at any time in Settings.
Events include the app version, a per-run random session identifier, language, and screen size. They
are not combined with sync account or device identifiers and are not used for advertising or tracking.


## Optional device protection

You can protect reading the Record with Face ID / Touch ID or your device code on iPhone,
and Touch ID or your Mac password on Mac. Protection is off by default. Changing the setting
requires system authentication; Chinotto does not collect biometric data or store a separate PIN.
The Record locks on cold launch and after you leave the app. You can still capture a new thought.
The setting is local to each device and does not change your sync account.

Protected history, search and playback stay hidden until authentication. iPhone widgets show
capture only while protection is enabled. Chinotto covers private content for background previews.
On Mac, outgoing sync reads wait until the protected Record is reopened; new captures are kept
locally. This access control does not encrypt the database, backups, exports or cloud storage.
An export is a plain copy under your control and can be read outside Chinotto.


## Restoring an archive

Archives are verified locally and previewed before any moments are added. Existing or previously
removed IDs are skipped. A consistent database recovery copy is kept on this device before a
nonempty restore; it is not uploaded or encrypted by device authentication. Temporary archive
staging is removed after use or retried on launch. The file picker may download a ZIP from your
chosen Files provider. Chinotto itself does not send archive contents to a server. If sync is
already enabled, restored text/history/links join its normal sync path; original audio stays
local. See [portable archives](record-archive.md) for recovery locations and compatibility.
