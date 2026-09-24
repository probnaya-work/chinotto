# Privacy

Chinotto is **local-first**. Your thoughts stay on your device in a SQLite database. Capture and search work without any network connection.

## Your data

- **Entry text** is stored locally only. It is not sent to analytics or included in crash-style telemetry.
- **Voice recordings** are stored in Chinotto's local app data and are not synced. Both the Mac App
  Store and the direct-download builds transcribe only on-device: when macOS has no on-device
  recogniser for the language, or speech recognition is not allowed, the recording is kept without
  a transcript and nothing is sent anywhere. It is read back on this Mac later, once, if on-device
  recognition becomes available. Time Machine may include the app's local data.
- **Removing a voice entry** deletes its recording and transcript from this Mac a minute after
  removal, once it can no longer be brought back. Backups you make yourself and exports are yours
  and are not changed.
- **Export** — Export the record writes a ZIP of plain text and audio to a location you control.
- **Backup** — automatic local backups in `chinotto-backups/` (last 7 kept).
- **Meaning model** — the Mac App Store build includes its local meaning model. The direct-download
  build downloads the same model on first use and then keeps it in local app data.

## Optional sync

When you enable sync and sign in with Apple, entry text can sync with the Chinotto mobile app via
Firebase. Recall themes can sync too when enabled on both devices. Chinotto also stores an install
identifier and the Mac name you see in the device list. Voice audio is not synced. Sync is optional;
the app remains fully usable without it.

## Optional sharing

When you create a share link from entry detail, Chinotto uploads a **snapshot** of the selected thoughts (text and share metadata you choose) to the hosted read service at `getchinotto.app`. The link expires after the period you pick. Your full local database is not uploaded. Sharing is explicit and optional.

## Analytics (opt-in)

Analytics are **off by default**. You can enable them in Settings → **Share anonymous usage data**.

When enabled, Chinotto sends only simple event names and numbers — for example “entry created” with the text length, or “search used” with the number of results. It never sends:

- the text of your thoughts
- your search query
- personal identifiers

Analytics help understand how the app is used. You can turn them off at any time in Settings.
Events include the app version, a per-run random session identifier, language, and screen size. They
are not combined with sync account or device identifiers and are not used for advertising or tracking.
