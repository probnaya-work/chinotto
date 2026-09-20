# Decisions the final design did not specify

**Updated after the desktop handoff (20 Sep 2026).** The working prototype is now the UI and
interaction source of truth, and it states a great deal that Distance v2 left open. Entries it
has since answered are struck from the inventory below and noted in §0; entries it opened are
added. `docs/handoff-diff.md` has the full comparison.

Every value here is an **implementation or product-policy decision made during the build**,
not something the design sources or the resolved product direction stated. They are collected so they can be reviewed as decisions rather than quietly inherited
as if they were part of the approved design.

Nothing in this file is settled. Each entry names where it lives, what the sources actually
said, and what was chosen instead.

Status key — **provisional**: explicitly agreed as temporary · **inferred**: read from what
the design draws, but never stated as a rule · **invented**: no basis in either source, chosen
because the code needed a number.

---

## 0. What the handoff resolved, and what it opened

### Resolved — no longer decisions

| Was | Now stated by the prototype |
|---|---|
| §1.1 D0 window, 6h inferred | **8 hours**, in `bandsFor()`'s `level()` |
| §2.1/2.2 rows per tier before a count | **no caps at all** — the record shows everything (see §0.1 below) |
| §2.3 folded-line head/tail | **2 and 2**, drawn as `line.slice(0,2)` / `slice(-2)` |
| §5b.1 the ⋯ container | **an inline verb row under the text**, never a popover |
| §5b.4 undo window, 10s | **8 seconds**, in the quiet line, with `⌘Z` |
| §8.3/8.4/8.5 motion | every duration, delay and easing is named in the prototype's stylesheet |
| §9.1/9.2/9.3 width axes | **100 / 92 / 84 / 80 / 76** for D0–D4, meta 90, a line's moments 94 |

Four tiers became five: the old `d3` is the prototype's **D4**, and a real D3 (13px, `wdth 80`,
`#aaa9a4`) was missing. The years band now begins at **180 days**, not at the turn of the year.

### Newly open — decisions this build had to make

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 0.1 | The record is unbounded, so rendering is windowed instead | chunks of `50` rows, mounted within `1400px` of the viewport, windowing above `100` rows | `src/features/record/Windowed.tsx` | **agreed** (handoff-diff §1.1) |
| 0.2 | How much of the record is held in memory at once | `50_000` fragments | `recordApi.ts` `RECORD_CAP` | invented |
| 0.3 | `--faint` split in two: the prototype's `#5f5e5a` for texture, a lifted `#878682` for the capture hints | — | `tokens.css` | **superseded — see §16.1** (the split is gone; `◌ hold space to speak` is now a quiet verb) |
| 0.4 | Light-mode `--ink-dim`, the new D3 step | `#52514e` | `tokens.css` | invented — the light appearance is a token pass, not drawn |
| 0.5 | Which Return triggers survive "the reason is mandatory" | a because-clause per trigger; `interval` dropped | `returns.rs` | **agreed** (handoff-diff §1.3) |
| 0.6 | Continuation offer: lookback and threshold | `3` days, score ≥ `2` with a shared run counting double | `continuation.ts` | ported from the prototype |
| 0.7 | How many D0 rows are asked for their line membership | `40` | `RecordApp.tsx` | invented |
| 0.8 | Provenance wording for a menu-bar capture | `typed from the menu bar` in `⋯ where it came from`, `menu bar` as the source on a moment | `FragmentRow.tsx`, `FragmentFocus.tsx` | **decided (phase 9)** |
| 0.9 | Standing in a month: which month a bare year lands you in | the last month that holds anything | `anchors.ts` | ported from the prototype |

0.1 is the cost of the decision in handoff-diff §1.1. The surface is exactly what the prototype
draws — every band, no caps — and the only thing bounded is how much of it is mounted. The three
numbers are tuned for "a chunk is always measured before it can be seen"; none is drawn anywhere.

0.2 is a safety limit, not a design one: the record arranges *everything* by distance and
standing re-measures from where you stand, so the surface cannot work from a recent page.

0.8 was the one place the prototype's prose and its code disagreed about behaviour rather
than about numbers: the README says "quick capture from the menu bar is a source here, and
only here", while `sourceOf()` has no branch for it and renders `typed`. **Resolved in favour
of the prose**, because the code's behaviour is indistinguishable from the branch simply not
having been written — and because a source the product names and then does not show is the
kind of gap that is never noticed again. The origin is recorded as `menubar` either way, so
changing the wording later costs nothing.

### Newly open — settings (phase 6)

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 0.10 | What "lift contrast in bright light" actually changes | raises `--ink-far`, `--ink-dim`, `--meta`, `--faint`, `--faint-text` only | `tokens.css` | invented |
| 0.11 | Text-size range and step | `80–140%`, step `10` | `appearance.ts` | from the prototype's own knob |
| 0.12 | How the whole column scales | `zoom` on `:root`, one number | `tokens.css` | invented |
| 0.13 | Export layout inside the zip | `chinotto-record/record.txt` + `chinotto-record/audio/<id>.<ext>` | `lib.rs` `export_record` | invented |
| 0.14 | What a fragment's line in `record.txt` carries | timestamp · method · origin · "wording corrected", then the body | same | invented |
| 0.15 | The backup line with no backup yet | `never` | `RecordApp.tsx` `backupLine` | invented |
| 0.16 | Which sections the quiet line silences on a utility surface | sync state and the offline notice; undo and update still speak | `QuietLine.tsx` | inferred |

0.10 is the consequential one. The prototype names the behaviour and draws its control but
never states a palette for it, so the values are chosen to raise only the quiet end of the
ladder: ink, the rules and the washes are untouched, and the ordering between tiers — the
thing that carries distance — survives. If it is meant to be a full second ramp rather than
a lift, this is the entry to revisit.

0.13/0.14 are a format the product will be judged on for as long as anyone keeps an export.
Plain text and one file per record rather than one per fragment, because the argument for
the export is that the record outlives the program; a directory of thousands of files is
harder to read with anything else, not easier.

### Newly open — sync and devices (phase 7)

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 0.17 | Where devices live | `users/{uid}/devices/{deviceId}`, additive beside the legacy entries | `syncDevices.ts` | invented |
| 0.18 | How often a device says it is here | `60s` | same, `HEARTBEAT_MS` | invented |
| 0.19 | `remove` revokes rather than deletes the row | — | same | **product rule** |
| 0.20 | What a conflict is detectable from | the two texts differ; which came first is unknown | `bridge.rs` `notice_remote_wordings` | forced by the legacy contract |
| 0.21 | Which wording the record shows until asked | the local one | same | inferred |
| 0.22 | Last-seen wording | `now` · `a moment ago` (<3 min) · `N minutes ago` (<1 h) · day + time | `Sync.tsx` `lastSeen` | invented |
| 0.23 | QR colours | fixed `#141416` on `#fbf9f4` in both appearances | `Sync.tsx` | invented — still appearance-independent, because a camera reads it; the ground moved with the palette (§16) |

0.19 is the one that looks like an implementation detail and is not. A deleted device row is
indistinguishable from a device that never registered, so the removed device would re-register
on its next heartbeat and reappear. Revoking leaves something for it to find.

0.20 is the honest limit of this phase. The legacy contract carries `{id, text, created_at}`
and no wording history, so the bridge can see that a moment now reads differently on the two
sides but cannot know which wording is later. That is exactly why the surface asks rather than
resolving, and why nothing is auto-discarded. Once mobile carries the earlier wording this
becomes a real three-way comparison; until then it is a two-way question.

0.23 because a camera has to read it, and `fill` is a presentation attribute that would not
resolve a CSS variable in any case.

### Newly open — the menu-bar panel (phase 9)

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 0.24 | The popover window's size | `480` wide — the 440px panel plus room for its shadow — and a height that follows the panel, `121` at rest | `tauri.conf.json`, `tray_capture.rs` | invented |
| 0.25 | What happens when a menu-bar save fails | the panel stays open, keeps the words, and says so | `TrayCapture.tsx` | invented |
| 0.26 | How the main window learns about a menu-bar capture | the existing `chinotto-tray-entry-saved` event | same | inherited |
| 0.27 | The panel's type scale | the identity file's drawn panel read literally — caret `3 × 26`, field `21px / −0.012em`, hint `11px`, gaps `12` and `18` — with only the width taken from its caption | `tokens.css` `--tray-*` | **from the identity file** |
| 0.28 | Which caret stands in the resting panel | the drawn bar, with the field's own caret held back while it is empty | `TrayCapture.tsx` | invented |

0.24 was a fixed `480 × 168` and clipped the second line of anything longer than one: the
field grows with the words and the window did not. The panel now measures itself and the
window follows, clamped in `tray_capture.rs` against what is left of the screen below the
menu bar. `121` is the resting height, so the first frame is already right.

0.27: the identity file draws the panel at 340px and captions it "440pt of it". The inner
values are literal, not a reduction — the padding is `18px 20px 14px` in both readings — so
the caption gives the width and the drawing gives everything inside it.

0.28 is the edge's own rule (`showBar = !hasText && !focused`) carried to a surface that is
always focused. Applying it unchanged would mean the resting panel the identity file draws
never appears; drawing the bar beside a live caret would mean two of them. The field's caret
is transparent while it is empty, and the first character hands over.

0.25 is the one exception to "capture never waits". Everywhere else the field clears and the
panel closes because the save cannot fail; here it can — the panel is a second webview and a
command can genuinely be unavailable in a stale build. Closing anyway would destroy words
somebody typed, so the panel holds them and says what happened. It is still not a dialog and
still not blocking: `esc` closes it and loses only what the person chooses to lose.

### Newly open — speaking, the Return, and the menu (phase 9b)

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 0.29 | Which surface the voice chord speaks into | the capture popover when it is open, the window otherwise; the window is never brought forward for the panel's sake | `lib.rs` `voice_handler` | invented |
| 0.30 | What `esc` does to a recording | ends the hold and keeps nothing — the same branch a release under `DROP_UNDER_MS` takes | `useVoice.ts` `drop` | invented |
| 0.31 | How the glyph knows a Return is waiting | `return_waiting`, a read-only query for an unanswered row; never `select_return` | `db/returns.rs` | **required by the Return's own rules** |
| 0.32 | What draws the waiting modifier | the same template, in a box widened to hold a dot beside the glyph — monochrome, not periwinkle | `generate-identity.py` `tray_template_waiting` | **forced** |
| 0.33 | When the panel asks for a Return | as it opens, never when the app launches | `TrayCapture.tsx` | invented |
| 0.34 | What `continue` does in the panel | arms the field; `⏎` calls `continue_fragment`, a spoken one is linked with `link_continuation`, and either records the outcome `continued` | same | invented |
| 0.35 | What the menu's lines are allowed to say | `today · n` from `count_between`, `sync on`/`sync off` from the webview's own `isFirebaseSyncConfigured()` | `tray_capture.rs` | **real state only** |
| 0.36 | The menu's accelerators | none | same | **forced** |
| 0.37 | When the menu's labels are recomputed | on every write and every toggle, not when the menu opens | same | **forced** |

0.29 is the one rule the menu bar cannot break: a panel that hauls the main window in front
of what you were doing is not a menu-bar capture. `⌥space` is emitted to the popover while it
is open and to the window otherwise; the release goes to both, because only the surface that
started is holding a recording and the other one's `stop` is a no-op. The panel's own gesture
is a bare `⌥` on an empty field, which needs no chord because the panel already has focus.

0.30 is not a deletion and does not touch the stance that a kept recording is the material.
Nothing was kept: the native side is stopped and the result never becomes a fragment, exactly
as a hold released inside 800ms already behaved. The file the recorder already wrote stays
where it is, and `orphaned_recordings` is what finds it again if it mattered after all.

The edge draws `esc to drop` in its own speaking row and had never wired it — the key did
nothing there. It now takes the same `drop`, at the position the design's ladder gives it:
`tray → recording → correcting → verb row → surface → focus → draft → standing`, so a
recording is stepped out of before a correction is. The two surfaces word it differently
(`esc to drop` at the edge, `esc drop it` in the panel) because each is drawn that way.

0.31 is the whole reason a second query exists next to `select_return`. Selecting *surfaces*
a Return: it inserts a row and spends the twenty-hour cooldown. The glyph is refreshed on
every write, so asking that question there would mean the menu bar deciding, on a timer and
with nobody present, that older material came back. `return_waiting` only reports. The
regression test for it is `asking_whether_a_return_waits_never_creates_one`.

0.32 is forced by the asset, not chosen. The identity file draws the modifier in periwinkle
and, two sections earlier, requires this glyph to be a real macOS template image with "no
periwinkle" in it. A template is a mask: macOS keeps its alpha and throws its colour away, so
the hue cannot survive and the dot is drawn in the same black as the glyph, tinting with the
bar exactly as the glyph does. The box grows to the right rather than the dot moving inward,
because a 22pt box holds a 17pt glyph with 2.5pt to spare and a badge laid over the ring at
that size is a damaged mark rather than a marked one. The drawing supports this: its row is
`[glyph][gap 14][dot]` with the dot pulled back 9, so the dot sits five points *beside* the
glyph, not on it.

0.33 follows from 0.31. This webview mounts when the app launches, so asking as it mounts
would surface a Return nobody came for. A Return is something you arrive at, and opening the
panel is arriving.

0.35: `today · n` is counted in Rust because the menu has to be right while no window is
open. Whether sync is configured is a build-time `VITE_` variable that only the webview can
read, so it is carried across rather than guessed at — and until it has been, the line says
`sync` rather than inventing an answer. The identity file draws `synced · just now` with a
live dot; the product tracks no last-sync time and `sync on` is the wording it already uses
for this fact in the quiet line, so that is what the menu says.

0.36 is forced. The identity file draws `⌘⇧C` and `⌘,` beside two lines. This app has no menu
bar of its own: `⌘⇧K` — not `⌘⇧C`, which is bound to nothing — belongs to the global-shortcut
plugin, and `⌘,` is handled inside the webview. An `NSMenuItem` key equivalent would take
either key away from its owner while the app is frontmost. The keys keep working and the menu
does not claim them.

0.37 is forced by Tauri, which proxies tray events through the event loop: a right-click
handler runs *after* AppKit has already popped the menu, so there is no "about to open" to
rebuild in. The labels are kept current as the state changes instead, which costs two counts.

**`set_icon` un-templates the glyph.** `tray-icon` hands the status item a fresh `NSImage` and
`NSImage.template` is a property of the image, so an icon set without re-asserting it is a
mask drawn as artwork: pure black on a dark menu bar, and no longer inverting under a light
one. `draw_glyph` is one function for exactly this reason.

---

## 1. Time and tiers

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 1.1 | ~~Length of D0~~ | `8` hours | `src/design/tiers.ts` `D0_WINDOW_HOURS` | **resolved by the prototype** |
| 1.2 | How often `now` advances, so material ages out of a tier while the window sits open | `60s` | `src/features/record/useNow.ts` | invented |

Distance v2 stated "D0 is the last few hours, not the calendar day" and gave no duration; six
was the midpoint of what its screens implied. The prototype states it outright — `dt < 8 * MS_H`
— so the constant is now transcribed rather than inferred. It stays isolated as one named
constant because the rule it encodes ("hours, not the calendar day") is worth being able to
point at.

## 2. How much the record shows

**Withdrawn.** There are no caps. The prototype's `bandsFor()` emits every fragment, Find is a
filter over the same bands rather than a surface with its own limits, and the `N more ↓`
affordance does not exist anywhere in the product.

The product direction's requirement — that the surface stay the same size at twenty fragments
and at fifty thousand — is now met by compression and windowing rather than by truncation: a
fragment six months old is one ellipsised 12px line, and past that a whole year is one row. See
§0.1, and handoff-diff §1.1 for why this was chosen over keeping a cap.

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 2.3 | Moments kept at each end of a folded Line | `2` head, `2` tail | `src/features/record/FragmentFocus.tsx` | **resolved by the prototype** |

## 3. Return

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 3.1 | Minimum gap between any two Returns | `20` hours | `src-tauri/src/db/returns.rs` | invented |
| 3.2 | Minimum gap before the *same* fragment may return | `30` days | same | invented |
| 3.3 | Minimum age before anything counts as "returning" | `30` days | same | invented |
| 3.4 | A capture can only pull something back if it is this recent | `24` hours | same | invented |
| 3.5 | A word is "distinctive" if it appears in this many fragments | `2`–`6` | same | invented |
| 3.6 | Minimum word length to count as distinctive | `5` characters | same | invented |
| 3.7 | Stopword list | 34 words | same, `STOPWORDS` | invented |
| 3.8 | A Line must have this many moments to return | `3` | same | invented |
| 3.9 | …and have been quiet this long | `60` days | same | invented |
| 3.10 | Trigger precedence | repeated language → same source → line continued → interval | same | inferred |

The product direction names the triggers and says Returns must be sparse, cooled down and
silent by default. It sets no thresholds. Every number above is a first guess at "sparse",
and they are the most likely thing here to need tuning against a real record — they are all
in one file for that reason.

3.10 is ordered by how directly the ground is grounded in something the person just did.
The direction lists the triggers but does not rank them.

## 4. Meaning / inferred Traces

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 4.1 | Similarity below which "close in meaning" is not worth claiming | `0.45` cosine | `src-tauri/src/db/meaning.rs` `MIN_SIMILARITY` | invented |
| 4.2 | Embedding model | `AllMiniLML6V2` (fastembed) | same | inherited from v1 |
| 4.3 | Fragments embedded per background pass | `32` (clamp 1–512) | same / `record_commands.rs` | invented |
| 4.4 | Guesses returned | `5` (max 20) | `record_commands.rs` | invented |

The direction requires inferred results to be distinguishable and explainable, and requires
that an embedding score is never the stated reason. Both hold. It says nothing about where
the cutoff sits.

## 5. URLs and external material

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 5.1 | What `url_key` normalises away | scheme, `www.`, trailing `/`, `#fragment`, case; query params sorted | `src-tauri/src/db/material.rs` `url_key` | invented |
| 5.2 | Query parameters treated as campaign rather than identity | 17 names (`utm_*`, `gclid`, `fbclid`, `ref`, …) | same, `TRACKING_PARAMS` | invented |
| 5.3 | Metadata fetch timeout | `8000` ms | `src/features/record/enrichment.ts` | invented |
| 5.4 | Bytes of HTML read before giving up | `512` KiB | same | invented |
| 5.5 | Title truncation | `500` characters | same | invented |
| 5.6 | Retry policy after a failed fetch | **none** — a failure is terminal until something asks again explicitly | `material.rs` queue is `state = 'pending'` only | invented |
| 5.7 | Background pass interval | `60s` | `src/features/record/RecordApp.tsx` | invented |
| 5.8 | Bundle id → readable app name | 8 known ids, anything else shown verbatim | `Material.tsx` `friendlyApp` | invented |
| 5.9 | HTML parsed by regex, not a DOM | — | `enrichment.ts` | invented |

5.1/5.2 are the riskiest entries in this file: a normalisation that is too aggressive will
merge two genuinely different pages into one "source", and repeat detection reads this key.
It is deliberately conservative (only well-known tracking parameters are dropped, and any
other query parameter is kept and participates in identity), but the list is a judgement call.

5.6 means a link shared while offline will not acquire a title on its own once the machine is
back online, because the failure has already been recorded. That is a real limitation, chosen
over a retry loop; a "try again" affordance on the fragment would resolve it.

5.9 has a known trade-off, covered by a test: a `<title>` appearing inside a `<script>` will
win over the real one. Accepted rather than parsing untrusted HTML into a DOM.

## 5b. The ⋯ menu

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 5b.1 | ~~The menu's container~~ | an inline verb row under the text | `src/features/record/FragmentRow.tsx` | **resolved by the prototype** |
| 5b.2 | "copy link" appears only when there is a link | — | same | invented |
| 5b.3 | "where it came from" expands in place rather than opening a screen | — | same | invented |
| 5b.4 | Removal is announced with undo rather than confirmed with a dialog | **8s**, in the quiet line, with `⌘Z` | `RecordApp.tsx` / `QuietLine.tsx` | **resolved by the prototype** |

Distance v2 named the items, their order and their wording exactly but never drew the menu, so
the container was the one invented object here. The prototype draws it: **not a container at
all**, but a row of words that appears under the fragment, in the same meta register as
everything else attached to it. The instinct was right — the least designed thing possible —
and the prototype goes one step further than "undesigned container" to "no container".

## 5c. The legacy bridge

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 5c.1 | Incoming mobile entries get `capture_method = 'imported'`, `origin = 'mobile'` | — | `src-tauri/src/db/bridge.rs` | invented |
| 5c.2 | A Continue mirrors as its own legacy entry, never appended to the earlier one | — | same | **product rule** |
| 5c.3 | A remote delete soft-removes here rather than destroying | — | same | invented |
| 5c.4 | A local removal is soft here, hard on the legacy row | — | same | invented |
| 5c.5 | Catch-up mirrors at most 500 fragments per pass | `500` | `RecordApp.tsx` | invented |
| 5c.6 | A typed URL becomes an encounter when the body contains exactly one | — | `RecordApp.tsx` | invented |

5c.1 is the same honesty rule as the migration: the sync payload is `{id, text, created_at}`
and says nothing about how a thing was captured, so the Record records that it does not know.

5c.6 means a fragment with two links stays plain text. One link is unambiguous; two is a
judgement about which one the fragment is *about*, and the design does not make it.

## 6. Hold

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 6.1 | Maximum fragments held at once | `5` | `src-tauri/src/db/record.rs` `MAX_HELD` | invented |
| 6.2 | What happens at the limit | refuse, and say so; never auto-release | same | invented |

The direction says Keep present is "deliberately small and bounded" with "no category or
ordering system", and the design draws exactly one held item. Neither gives a number. Five
matches what v1's pins allowed, so migrated pins always fit.

6.2 is the more consequential half: auto-releasing the oldest would be an ordering system,
which the direction rules out, so the limit refuses instead and asks for an explicit release.

## 7. Window and layout

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 7.1 | Default window size | `1200 × 820` | `src-tauri/tauri.conf.json` | provisional |
| 7.2 | Minimum window size | `680 × 480` | same | provisional |
| 7.3 | Left offset of the content column | `clamp(24px, (100vw − 940px) × 0.4, 200px)` | `RecordApp.tsx` | inferred |

The design draws a single 1440px frame with the column at left 200px, width 940px. 7.3
reproduces that exactly at 1440 and degrades predictably; the design says nothing about any
other width. 7.1/7.2 are agreed provisional pending the resize pass.

## 8. Colour and motion the design does not draw

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 8.0 | Light-mode `--meta` darkened to satisfy the design's own 5:1 floor | `#6b6a66` → `#686763` | `src/design/tokens.css` | **contradiction resolved** |
| 8.1 | Light-mode `--faint` / `--faint-text` | both mapped to `--meta` | `src/design/tokens.css` | invented |
| 8.6 | Dark `--faint` split rather than lifted wholesale | `--faint` `#5f5e5a` (the prototype's) · `--faint-text` `#878682` | same | **superseded — see §0.3** |
| 8.2 | Light-mode `--row-hover` | ink at 4% | same | inferred |
| 8.3 | Capture settle duration | `220` ms | same, `--settle` | invented |
| 8.4 | Recede duration | `260` ms | same, `--recede` | invented |
| 8.5 | Easing | `cubic-bezier(0.2, 0, 0, 1)` | same, `--ease` | invented |

**8.0 is a contradiction in the sources, not a gap.** The design states a floor — "nothing
below 12px or 5:1" — and its own light palette inks the floor tier with `#6b6a66`, which
measures 4.79:1 on `#f2f1ec`. The stated rule was treated as the intent and the sampled
colour as the artifact, so the token is darkened by three points to 5.01:1. The change is
imperceptible and reverting it is one line. **This is the one place the implementation
deliberately does not match a colour the design draws.**

**8.6 was a contradiction inside Distance v2; the handoff turned it into a real choice.** The
working prototype restates neither the 5:1 floor nor an exception to it, so `#5f5e5a` is no
longer an artifact of a screen — it is the colour the source of truth specifies.

The token is therefore **split in two** rather than lifted wholesale. `--faint` is the
prototype's `#5f5e5a` exactly, and inks everything the colour is used for as texture: the
year-density ticks, fourth-level detail, the quiet line at rest. `--faint-text` carries the
lift to `#878682` (5.05:1) and inks only the capture hints — `⏎ leave it`, `esc drop it`,
`◌ hold space to speak` — which are how the keyboard interface is discoverable at all.
Deliberately still below `--meta` (5.61:1), so faint reads quieter than meta either way.

Reverting is one line: point `--faint-text` at `--faint`.

Measured contrast for every token pair, after both corrections:

| token | dark on `#141416` | light on `#f2f1ec` |
|---|---|---|
| `ink` | 14.71 | 15.21 |
| `ink-near` | 11.78 | 12.31 |
| `ink-verb` | 11.09 | 9.28 |
| `ink-far` | 8.86 | 9.28 |
| `meta` | 5.61 | 5.01 *(was 4.79)* |
| `faint` | 2.83 *(the prototype's)* | 5.01 |
| `faint-text` | 5.05 | 5.01 |
| `ink-dim` | 7.53 | 7.30 |

8.0 is a one-line revert if the design would rather keep its sampled value.

No light capture row is drawn anywhere, so neither source states a light `faint`. Both light
tokens are marked `UNSPECIFIED` in `tokens.css` itself as well as here.

**8.3–8.5 are withdrawn.** The prototype names every animation it has, with its own duration,
delay and easing — `settle` 0.35s ease-out, `rise` 0.5–0.6s, `let go` 0.45s ease-in, the launch
choreography's `arrive1/2/3`, `ringclose`, `riseword` and their reverses on
`cubic-bezier(.16,.84,.3,1)`, and the five recording bars at 0.7–1.3s. All are transcribed into
`tokens.css` rather than invented.

## 9. Typography read from the design rather than stated by it

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 9.1 | `wdth` is fixed per tier, never interpolated | 100 / 92 / 84 / 80 / 76 | `src/design/tiers.ts` | **resolved by the prototype** |
| 9.2 | Meta sits at one width regardless of tier | `90` | same | **resolved by the prototype** |
| 9.3 | A moment in a Line other than the focused one | 19/94 vs 26 | `FragmentFocus.tsx` | **resolved by the prototype** |

These were read off Distance v2's markup, never stated. The prototype's `STY` table and its
focus surface state all three outright, so they are now transcribed. They remain fixed per-tier
tokens rather than a continuous function of age — which the prototype also confirms.

## 10. Migration policy

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 10.1 | `capture_method` for v1 material | `imported`, never guessed as `typed` | `migrate.rs` | agreed |
| 10.2 | v1 edit counts do not become corrections | `legacy_edit_count` | same | agreed |
| 10.3 | v1 in-text continuation offsets create no Line | provenance only | same | agreed |
| 10.4 | v1 tables are read, never dropped | — | same | agreed |
| 10.5 | `transcript corrected once` on voice only | — | `FragmentFocus.tsx` | agreed |

Listed for completeness: these were raised during the build and confirmed, unlike the rest of
this file.

---

## 11. Identity across the system surfaces (phase 10)

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 11.1 | Every raster is drawn, not rasterised from SVG | supersampled 4× in Pillow | `scripts/generate-identity.py` | invented |
| 11.2 | Each icon size picks its own rung | ≥40px mark → three dots, 24–39 → two, ≤20 → one | same, `rung_for` | **from the identity spec** |
| 11.3 | The `.ico` is one drawing downscaled | — | same | accepted limitation |
| 11.4 | The tray icon ships at @2x | `tray_menu_template@2x.png`, 44px | `tray_capture.rs` | invented |
| 11.5 | The DMG background is browser-rendered | 1320×800 (@2x of 660×400) | `scripts/generate-dmg-background.py` | **forced** |
| 11.6 | The microphone and speech usage strings | product voice, not legal register | `Info.plist` | inferred from the identity file |

11.2 is the rule the identity spec calls the only way to get this wrong. The 1024 master is
*not* downscaled to 16px: `icon_16x16.png` is drawn at the ≤20px rung, `icon_32x32@2x.png`
at the 24–39px rung, and so on. Downscaling the three-dot drawing turns the ring into a
hairline and loses the far dots, which is exactly the failure the ladder exists to prevent.

11.3: per-size artwork inside an `.ico` has to be assembled by hand, and this product does
not ship on Windows. Recorded rather than quietly pretended otherwise.

11.5 is a real constraint, not a preference. `@fontsource-variable/archivo` ships woff2
only, Pillow cannot read woff2, and no Archivo TTF is installed — so the Python generator
would set the wordmark in Helvetica. It now refuses rather than substituting a typeface in
the one window someone sees before the product has said anything. The committed PNG was
rendered in the app's own webview with the real variable font. Dropping an `Archivo-Medium.ttf`
into `scripts/` makes the generator self-sufficient again.

---

## 12. Voice (phase 11)

The canonical rule — **retained audio is the source, the transcript is derived from it** —
was not what the code did. The old pipeline fed the microphone to the recogniser, kept the
string and discarded the audio, so a fragment's material was a machine's reading of
something that no longer existed. The tap now writes an `AVAudioFile` as the buffers
arrive, and the recogniser is a second consumer of the same buffers.

What follows from that, and is now true: a recogniser that is unauthorised, fails, times
out or hears nothing costs the words and never the recording. `run_capture` returns the
path and duration regardless, `capture_voice` runs before `record_transcript`, and a failed
transcript is recorded as a failed *reading* rather than a failed capture.

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 12.1 | Audio format | the input's own format, `.caf` | `speech.rs` | invented |
| 12.2 | Where recordings live | `<app data>/audio/<uuid>.caf` | `lib.rs` `audio_dir` | invented |
| 12.3 | Ceiling on one recording | `120s` | `lib.rs` | invented |
| 12.4 | A release shorter than this is a slip | `800ms` | `useVoice.ts` `DROP_UNDER_MS` | **from the prototype** |
| 12.5 | How a hold ends | an atomic flag polled every 16ms; `max_ms` is only a ceiling | `speech.rs` | invented |
| 12.6 | Microphone state is only ever learned by trying | never probed, never assumed | `RecordApp.tsx` | inferred |
| 12.7 | `⌘⇧V` is removed | hold is the only voice gesture | `lib.rs` | **from the prototype** |

12.1: the input format rather than a re-encode. This is the source, and resampling on the
way in would mean the thing we kept is already a derivation. `.caf` because it is what
`AVAudioFile` writes natively and it survives arbitrary sample rates.

12.6 is a small honesty point with a visible consequence: there is no API that reports
microphone permission without asking for it, so settings says "the mac hasn't been asked
yet" until a recording actually succeeds or is refused. It never claims to know an answer
it has not been given.

**Not done, and why.** Re-transcribing an existing recording ("try again" on a failed
transcript, in `Material.tsx`) is wired to a callback that nothing supplies yet: it needs a
file-input recognition path rather than the live-buffer one, which is a separate piece of
Speech-framework work. The audio is retained, so this can be added later against material
that already exists — which is the point of keeping the source. Until it is, a failed
transcript offers "type it" only.

---

## 13. Cleanup and accessibility (phase 12)

| # | Decision | Value | Where | Status |
|---|---|---|---|---|
| 13.1 | Every text verb is a real `<button>` | focus, Enter/Space, an accessible name | `Verb.tsx` | invented |
| 13.2 | Focus ring | 2px ink, 2px offset, `:focus-visible` only | `tokens.css` | inferred |
| 13.3 | Material itself stays a click target rather than a button | rows, bodies, trace snippets | various | inferred |
| 13.4 | Tailwind and the v1 stylesheet are removed entirely | `index.css` 6 121 → 48 lines | `index.css` | invented |

13.1 is a correctness fix, not a polish pass. The product's controls are words — `release`,
`let go ↓`, `bring back`, `show this one instead` — and every one of them was a `<span
onClick>`: unreachable by keyboard, unannounced by a screen reader, and invisible to the
`esc`/tab model the rest of the product is built on. They are now buttons stripped of
everything a button normally brings except the part that matters.

13.3 is the deliberate exception. A fragment's own body is material, not a control, and
making every row a button would flood the tab order with the record itself. Material is
reached by `↓` into the record and opened with `⏎`, which is the keyboard path the design
specifies; the buttons are for the verbs *around* it.

13.4: nothing used a Tailwind class. The stylesheet was the v1 app shell — space lenses,
the intro screen, the stream, the overlays — and went with the surfaces it styled, along
with `tailwindcss`, `postcss`, `tailwind-merge` and `@fontsource/open-sauce-one`. The
shipped CSS bundle is 7.8 kB.

## 14. What a real record made visible (post-dogfood)

The first run against a real database — 194 fragments, none newer than 22 days — surfaced
four defects and one design gap. The defects are fixed here; the gap is §14.5, and it is
not mine to close.

| # | Decision | Chosen behaviour | Where | Kind |
| - | -------- | ---------------- | ----- | ---- |
| 14.1 | Every Tauri command runs off the main thread | `#[tauri::command(async)]` on all but two | `lib.rs`, `record_commands.rs` | correctness fix |
| 14.2 | A failed model load is remembered for the run | one attempt, then meaning is simply absent | `embeddings.rs` | invented |
| 14.3 | The model's weights live in the app's data directory | `<app data>/models`, not `./.fastembed_cache` | `embeddings.rs`, `lib.rs` | correctness fix |
| 14.4 | Fixed-position chrome sits outside the animated column | `QuietLine` and the error line are siblings of it | `RecordApp.tsx` | correctness fix |
| 14.5 | Distance is measured from the record's own edge, not from the clock | `referenceFor()`, clamped to now | `tiers.ts`, `bands.ts` | decided by the owner |
| 14.6 | Every hover rule is `!important`, and a `--ink-bright` register is added | the hover language works at all; row verbs reach white | `tokens.css`, `Verb.tsx` | correctness fix |

**14.1.** `#[tauri::command]` without `(async)` runs on the app's main thread. All 106
commands were plain, so reading the whole Record, selecting a Return, transcribing speech
and running the meaning model each stopped the window from answering for as long as they
took. The macro compiles a sync function marked `(async)` onto a thread pool, so the bodies
are unchanged; only the thread they run on is. Two stay on the main thread on purpose:
`set_app_icon`, which needs `MainThreadMarker` for AppKit, and `native_apple_sign_in`, which
was already async and dispatches to the main thread itself.

The cost is that two commands can now interleave, where before the main thread serialised
them. Every `Db` method takes the connection lock for its own work, so no single statement
can tear; what is no longer atomic is a command's *pair* of steps — a write and its mirror
into `entries`, or a read of pending embeddings and the write that follows. Both are
idempotent upserts keyed by id, so the worst case is repeated work, never a lost wording.
Noted rather than hidden: a genuinely compound command added later needs its own
transaction, not the main thread's accidental mutex.

**14.2 / 14.3.** `fastembed` caches weights in `.fastembed_cache` *relative to the working
directory*. A mac app opened from Finder has `/` for a working directory, which it cannot
write — so the packaged app fetched ~90 MB, failed to store it, and started again on the
next call. `embed_pending(24)` did that twenty-four times a minute, forever, on the main
thread, and every click that opened a fragment did it once more through `find_by_meaning`.
Every fragment stayed permanently pending: all 188 imported rows carry `body_hash = ''`, so
the queue could never drain. The cache now lives beside the database, and the load is
attempted once per run — a mac that cannot load the model has word-based traces, Find and
the whole Record, and no guesses. That is a missing opinion, not a missing feature, and it
is not worth stalling for twice.

**14.4.** `position: fixed` resolves against the nearest ancestor with a transform, and the
record column carries `chinotto-rise`. The quiet line — `● sync on`, `settings ⌘,`, the undo
offer — was therefore pinned to the bottom of the *record* rather than the window, landing
on top of the last rows and scrolling away with them. Nothing in its own styling said so;
`offsetParent` did. It now renders outside the column, where `bottom: 22px` means what it
says.

**14.5.** `levelForRecency` measured from the wall clock, exactly as the prototype does. On
a record left alone for three weeks that put *everything* at the floor: 170 fragments at D4,
4 at D3, nothing at D0, D1 or D2 — a whole record at 12 px, `wdth` 76, one ellipsised line
each. The prototype cannot show this, because its corpus is generated relative to its own
`NOW` and so always has today's material in it; run its own `bandsFor` against a real stale
record and it collapses the same way.

Raised as a semantics question rather than fixed, because it is about what distance *means*,
and answered by the owner: distance is measured from the record's own edge — the newest
fragment it holds — clamped so that a date in the future cannot drag the record forward.
`referenceFor()` in `tiers.ts`; `levelForRecency` is unchanged and still takes its reference
as an argument.

Two things it deliberately does not change. The **words** still come from the clock: a band
that is D1 for this record can still be three weeks old, and `labelFor` picks its wording
from the true recency so the column never answers "earlier today" about august. And a row's
gutter was already keyed on the actual date rather than the tier, so a D0 row on a stale
record reads `29 aug`, not a clock time.

What it costs: a fragment can move back *toward* D0 as the material around it ages, so
"distance only ever recedes" is no longer true of an individual row — only of its position
relative to the edge. On a record with anything in it today the reference is now and the
behaviour is the prototype's, unchanged.

What it does *not* buy, measured on the same real record: `d0(2) · d3(11) · d4(170) ·
years(11)`. The ladder has a top again, but 170 rows are still at the floor — because the
windows (8 h / yesterday / 7 d / 60 d / 180 d) are sized for someone writing many times a
day, and this record averages roughly one fragment every other day. Whether the windows
should scale with how often a person actually writes is a separate open question, and a
larger one.

**14.6.** The product's only pointer feedback is ink brightening by a step, and none of it
worked below D0. A row's resting colour comes from `tierStyle()` and a verb's from
`metaStyle()` — both write `color` into the element's own `style` attribute, because the
value is decided per tier at render time — and an inline declaration outranks any class
rule. So `.chinotto-compact-row:hover`, `.chinotto-verb:hover` and
`.chinotto-band-label:hover` were all computed and then discarded on every element they
named. Hovering anything in the record did nothing at all.

Fixed with `!important` on the hover rules rather than by moving every resting colour into
CSS: the colours genuinely are per-tier render-time values, and a class per tier would put
the same number in two files. `:not(:disabled)` carries the one exception — a disabled verb
is a statement, not a control, and keeps its own colour.

While the rules were dead their *values* had drifted from the prototype unnoticed, so they
are re-transcribed here from `style-hover` on each element:

| element | prototype | register |
| ------- | --------- | -------- |
| compact row, year label, `↓ N more paragraphs`, Return verbs, `release`, `not this`, `‹ back to the edge` | `#e6e6e3` | `--ink` |
| a row's own verbs (`continue`, `hold`, `⋯`), the ⋯ menu, a moment's verbs in focus | `#fff` | `--ink-bright` *(new)* |
| band label, quiet line, `wording corrected · show` | `#c9c9c6` | `--ink-verb` |
| `◌ hold space to speak` | `#8f8e89` | `--meta` |

`--ink-bright` is the one addition to the ink ladder: the design spends pure white on the
controls that sit *on top of* material rather than beside it, and the existing ladder tops
out at `--ink`. In the light appearance — which the prototype does not draw — it is `#000`,
the same step past `--ink` in the other direction.

Not carried over: the voice chip's `border-color:#8f8e89` on hover, which is a fifth
register for one element.

**SUPERSEDED by the finalized colour system (§16).** The four registers above collapse into
two ranks, and a verb is now coloured *at rest* rather than only under the pointer: full
agency lifts to `--agency-hover` with an underline, the quiet rank lifts to `--agency` without
one, and an ambient clickable word takes agency's colour and weight only while the pointer is
on it. `--ink-bright` is now `--agency-hover` and `--ink-verb` is now `--ink-quoted`, which
inks no verb at all.

## 15. Voice, the first time it was actually held (post-dogfood)

Three symptoms, four causes. The audio was never at risk in any of them — every recording
made it to disk, which is what the pipeline was rebuilt for — but two of them left it there
with nothing pointing at it.

| # | Decision | Chosen behaviour | Where | Kind |
| - | -------- | ---------------- | ----- | ---- |
| 15.1 | The stop flag is armed when a capture is *asked for* | `arm_stop()` before the command is queued | `lib.rs`, `speech.rs` | correctness fix |
| 15.2 | The release is listened for on the window | `keyup` (space), `mouseup`, `blur` | `useVoice.ts` | correctness fix |
| 15.3 | Speech permission is asked for, not pointed at | `requestAuthorization` on the main thread, not awaited | `speech.rs`, `lib.rs` | correctness fix |
| 15.4 | A missing transcript carries the mac's own reason | `transcript_failure` through to `voice_transcripts.failure` | `speech.rs` → `useVoice.ts` | invented |
| 15.5 | A recording on disk that no fragment claims is adopted on launch | its own end time, its real duration, a transcript that says why there is none | `lib.rs`, `RecordApp.tsx` | invented |

**15.1.** `run_capture` cleared `STOP_REQUESTED` *after* creating the recogniser and starting
the audio engine — which, the first time, is also when the mac puts up its microphone
prompt. That is exactly when somebody lets go. Their release set the flag, the clear wiped
it, and the recording ran to the 120-second ceiling. A release is never early; it is only
ever ahead of the machine. The flag is now armed before the command is even queued, so
everything after that point belongs to the capture being started.

Compounding it: until §14.1 every command ran on the main thread, and
`run_native_speech_recognition` blocks for the length of the recording — so
`stop_voice_capture` could not be dispatched *at all* while one was running. The recording
was literally unstoppable, by construction.

**15.2.** Both gestures that start a recording are presses, and both listened for the
release on the element that took the press: `onKeyUp` on the textarea, `onMouseUp` on
`◌ hold space to speak`. Slide the pointer off the words before letting go, or let the field
lose focus mid-hold, and the release lands somewhere else and is never seen. The press stays
on the element; the release is the window's. `blur` counts as one, because a window that is
no longer frontmost will not be told when the key comes up.

**15.3.** Speech recognition is a separate permission from the microphone. On "not
determined" the code returned an error telling the person to enable Chinotto under System
Settings › Privacy & Security › Speech Recognition — but **an app that has never called
`requestAuthorization` is not in that list**, so the only route out of "not determined" was
the one thing the code would not do. It asks now, on the main thread, and does not wait:
the prompt is the person's to answer in their own time, this recording keeps its audio and
has no words, and the next one has both.

**15.4.** "not transcribed" is true and useless. The reason is the only actionable part, and
it is the mac's, not ours — *not allowing*, *being asked*, *nothing was heard* — so it is
carried from `run_capture` through to `voice_transcripts.failure` rather than flattened on
the way. Never a reason the *audio* failed: those are different events and the product's
whole claim is that they are.

**15.5.** Both stuck recordings left a complete `.caf` on disk and no fragment anywhere,
because the only way to end them was to quit the app. The audio survived, which is what the
pipeline was rebuilt for — into a directory the Record could not see, which is not.

Written as a rule rather than as a one-off repair of those two files. The one-off would have
meant hand-writing rows into a live database, and it would have left the next interrupted
recording orphaned in exactly the same way. `orphaned_recordings` lists what is in the audio
directory that `voice_captures` does not point at; the surface adopts each one through the
same calls a live capture makes, so a recording becomes material by one path and not two.

Four things it is careful about:

- **The time is the file's, not the moment it was noticed.** `captured_at` is immutable, so
  it gets exactly one chance to be right. The end of the recording is the file's last write,
  which is also when a live capture would have become a fragment. Checked against the two
  that were found: 31 s and 47 s of wall clock between creation and last write, against
  30.2 s and 47.3 s of audio.
- **The duration is read from the file**, not inferred from its size, and a file that cannot
  be opened is left alone rather than adopted with a guessed length.
- **`DROP_UNDER_MS` still applies.** Being interrupted must not put something into the
  Record that holding the key for the same time would have thrown away.
- **The transcript says what happened**: state `failed`, reason "found on disk after the app
  stopped · no words were ever taken from it". Not "pending", which would promise words that
  nothing is going to fetch.

What it does not record is that a fragment arrived this way rather than live. There is no
field for it — `capture_method` is `voice` and `capture_origin` is `desktop`, both true —
and the transcript's reason is where the story is. A fragment that is later transcribed by
hand loses that line, and with it the only trace; noted rather than solved.

## 16. The finalized colour system (post-handoff)

The handoff package now ends with a **Colour and affordance** table, and it is the source of
truth for colour. It supersedes the ink ladder §8 was written against. Four families, which
do not borrow from each other: **material** (your words and Chinotto's voice, lightness only),
**agency** (verbs, filled primaries, the single recovery in any error — lightness *and* weight,
`wght 540`), **evidence** (a ground plus an ink that lifts above material), and **live**
(`#9fb79f`, the only hue, and never on anything pressable).

What actually moved, in the dark appearance:

| token | was | is | why |
|---|---|---|---|
| `--ink` (D0, caret, mark) | `#e6e6e3` | `#d4d3ce` | material's top step; 12.27:1 |
| `--ink-near` (D1, utility body) | `#cfcfcc` | `#c0bfba` | |
| `--ink-verb` → `--ink-quoted` | `#c9c9c6` | `#c9c9c6` | value kept, role narrowed: it no longer inks a verb, only material Chinotto quotes back inside its own sentence |
| `--ink-bright` → `--agency-hover` | `#ffffff` | `#ffffff` | it was never "a step past material"; it is a verb under the pointer |
| `--mark` → `--evidence-ground` | `…0.16` | `…0.15` | and its ink is now `#f4f2ee`, above material, rather than `--ink` |
| `--mark-trace` → `--evidence-ground-trace` | `…0.14` | `…0.13` | |
| `--row-hover` | `…0.04` | `…0.045` | |
| `::selection` | `…0.25` | `…0.26` | |
| `--correction-rule` | `--meta` | `--agency-quiet` | the edit window is an affordance, not a label |
| `--rule-present` (a Return's rail) | `--ink` | `--agency-quiet` | |
| the app icon's ink | `#e6e6e3` | `#d4d3ce` | material, not agency: an icon is a mark, not a control |
| accent (the sync dot) | periwinkle | `--live` `#9fb79f` | the one hue, and it moved hue entirely |

New: `--agency` `#fbf9f4`, `--agency-quiet` `#e9e7e0`, `--agency-weight` `540`,
`--agency-ground`, `--inert-ink` / `--inert-border`, `--evidence-rail` `#5c5a52` and
`--evidence-rail-inferred` `#2a2a2e`, `--live`, `--row-selected`, `--selection`,
`--placeholder`, `--surface-deep`, `--chip-border` / `--chip-border-inert`.

Retired, with no replacement needed: `--faint-text`.

### 16.1 `--faint-text` is gone, and the contrast question it answered has changed shape

§0.3 / §1.2 / §8.6 split `--faint` and lifted the functional half to `#878682` (5.05:1),
because Distance v2 stated a floor of "nothing below 12px or 5:1" and the prototype inked the
capture hints at `#5f5e5a` (2.83:1).

The finalized system states no floor, draws `#5f5e5a` at every one of those sites, and
resolves the underlying problem differently: **`◌ hold space to speak`** — the one hint that
teaches an interface you cannot otherwise discover — is now a quiet verb at `--agency-quiet`,
**14.87:1**, rather than a dim label. What stays at `#5f5e5a` is the row that merely restates
keys already pressed (`⏎ leave it · ⇧⏎ new line · esc drop it`), the quiet line at rest, and
the year-density bars.

So the split is removed and the design's own value stands. **This is still a contrast
regression on that hint row**, and it is recorded here rather than argued away: reverting is
one token (`--faint`), and the sites that would come with it are listed above.

The inert filled primary (`--inert-ink`, 2.83:1) is deliberate and is not the same question:
a control that is not operable is exempt from the contrast minimum, and the design draws it.

### 16.2 The light appearance is still a token pass

The finalized system gives the light appearance its field and its ink (`#f2f1ec` / `#1b1b1d`,
and only on the settings icon tiles). Everything else is **derived, not sampled**: each token
is placed at the contrast ratio its dark counterpart carries against `#141416`, which keeps
the ladder's shape and the two agency ranks' spacing without inventing a second design. The
material ladder below D0 is unchanged — the design's light endpoints did not move.

Two light values have no basis in the design at all and are flagged as such:

- **`--live` `#3d473d`** — the sage held and darkened until it carries the same 8.5:1 on
  `#f2f1ec` that `#9fb79f` carries on `#141416`. `#9fb79f` itself measures about 2:1 on the
  light field and would disappear.
- **the agency steps** `#0f0f11` / `#050507` / `#000000` — past the given ink toward black, at
  the dark side's spacing.

### 16.3 States the design does not draw, and what each was derived from

| state | derived from | value |
|---|---|---|
| keyboard focus ring | agency — a focus ring says where your next action lands | `2px solid var(--agency)` |
| keyboard-selected row | selection-as-structure, like the current moment's dot | `inset 2px 0 var(--ink)` |
| a voice chip whose audio is not on this device | not a control at all, so it leaves agency | `--meta`, dashed, `--chip-border-inert` |
| the quiet line's transient notice (`held at capacity`) | the neutral tone the design gives the quiet line's own sync sentence | `--ink-far` |
| the hosted sign-in bridge and the "Mac app only" page | the field, which the design states is universal | `--surface` / `#d4d3ce`, everything else untouched |

### 16.4 Where the prototype and the handoff's prose disagree

The prose says an ambient clickable element "lifts only on hover, where it also gains an
underline". The prototype gives the underline to **full-rank verbs** (`#ffffff` +
`text-underline-offset: 4px`) and gives ambient elements a lift to `--agency` with **no**
underline — consistently, at every one of the sites the prose names (band label, year row,
sync status, fold). The prototype is followed, per this repository's standing rule that it is
the visual source of truth.

Two more places where the prototype is internally inconsistent and each site was matched
exactly rather than normalised: Find's meaning-guess dismissal `not this` rests at
`--agency-quiet`, while the traces block's `yes` / `not this` rest at `--agency`; and the
continuation offer's `no` rests at `--meta`, below either rank.

### 16.5 Mobile: the weight is a family

React Native's `fontWeight` takes hundreds only, and with the `wdth` axis already resolved
ahead of time the weight has to be carried by the family too. `Archivo-540-{90,92,94}` are
generated alongside the existing instances, and a verb asks for `weight: 540` rather than
setting `fontWeight` — which would silently round to 500 and drop half of what marks a verb.
