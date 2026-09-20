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
| 0.3 | `--faint` split in two: the prototype's `#5f5e5a` for texture, a lifted `#878682` for the capture hints | — | `tokens.css` | **agreed** (handoff-diff §1.2) |
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
| 0.23 | QR colours | fixed `#141416` on `#e6e6e3` in both appearances | `Sync.tsx` | invented |

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
| 0.24 | The popover window's size | `480 × 168` — the 440px panel plus room for its shadow | `tauri.conf.json` | invented |
| 0.25 | What happens when a menu-bar save fails | the panel stays open, keeps the words, and says so | `TrayCapture.tsx` | invented |
| 0.26 | How the main window learns about a menu-bar capture | the existing `chinotto-tray-entry-saved` event | same | inherited |

0.25 is the one exception to "capture never waits". Everywhere else the field clears and the
panel closes because the save cannot fail; here it can — the panel is a second webview and a
command can genuinely be unavailable in a stale build. Closing anyway would destroy words
somebody typed, so the panel holds them and says what happened. It is still not a dialog and
still not blocking: `esc` closes it and loses only what the person chooses to lose.

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
