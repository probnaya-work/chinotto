# Handoff diff — working prototype → `feat/chinotto-next-desktop`

Compares the **working Claude Design prototype** (`design_handoff_chinotto_desktop/`, project
`5e00e5ee-ab7f-4897-aaf8-56a067e2ade9`) against the current implementation on this branch.

From this point the source-of-truth order is: **working prototype** → resolved product direction →
existing implementation → old Distance/design studies.

## Method

The handoff package was imported and read in full: `README.md`, the prototype's `class Component
extends DCLogic` logic (state, transitions, `STY`, `item()`, `band()`, `utilityVals()`,
`renderVals()`), its template (markup, inline styles, copy), `chinotto-data.js` (corpus + model
helpers), and `reference/Chinotto - Identity.dc.html` (asset ladder + the old→new replacement
inventory). The prototype was then run locally and walked state by state — launch, edge, record,
find, standing, focus/line, settings, sync (all six states), menu-bar panel, quiet line — and
compared against the running implementation at 1440×900.

**Where the package's own prose and its running code disagree, the running code is taken as the
prototype.** Those disagreements are listed in §1.

---

## 1. Conflicts — flagged, and how each was resolved

These are the places where following the prototype literally would break durable product semantics,
data integrity, or the package's own stated rules.

**§1.1, §1.2 and §1.3 were decided by the product owner on 20 Sep 2026** and the chosen option is
marked **DECIDED** below. §1.4 and §1.8 are readings of the sources, taken as stated. §1.5–§1.7 are
engineering constraints handled as proposed.

### 1.1 The record is unbounded in the prototype; Now is bounded here

`bandsFor()` emits every fragment. There are no per-tier caps and no "N more" affordance anywhere.
The implementation caps each tier (`TIER_CAP` in `Now.tsx`, d0 8 / d1 4 / d2 3 / d3 3) and offers
`N more ↓`, because the resolved product direction requires Now to stay the same size at twenty
fragments and at fifty thousand.

Measured on the prototype's own corpus (2 163 fragments over six years) the record renders ~266
rows — long, but finite. Scaled to a heavy record (50 fragments/day) the same rules give roughly
350 rows in D2, 3 000 in D3 and 6 000 in D4: about 9 000 DOM rows on the default surface. That is a
feed, and a rendering problem.

**Options:** (a) follow the prototype exactly and add windowing/virtualisation so the cost is
bounded even though the surface is not; (b) keep a cap but express it the prototype's way rather
than as `N more ↓`; (c) cap only the tiers that can grow without bound (D2–D4) and leave D0/D1
uncapped. **Recommendation: (a).** It is the only option that keeps the surface the prototype
draws; the cost is engineering, not design.
**DECIDED: (a)** — render every band as drawn, and window the rendering so DOM cost stays bounded.

### 1.2 `--faint` is below the contrast floor the earlier design stated

The prototype inks the capture hints (`⏎ leave it`, `esc drop it`, `◌ hold space to speak`) and the
year-density bars at `#5f5e5a` — 2.83:1 on `#141416`. The implementation deliberately lightened this
to `#878682` (5.05:1) because Distance v2 stated a floor of "nothing below 12px or 5:1" and these
are functional text, not decoration (recorded in `unspecified-decisions.md` §8.6).

The new package states no contrast floor and says colours are final. So this is no longer a
contradiction inside one source — it is the new prototype overriding an older rule.

**Options:** (a) revert to `#5f5e5a` and accept 2.83:1 on the only text that teaches the keyboard
interface; (b) keep `#878682`; (c) revert for decorative uses (year bars, prototype affordances) and
keep the lift for the capture hints only. **Recommendation: (c).** It is one token split in two and
matches the prototype everywhere the colour is not carrying functional text.
**DECIDED: (c)** — `--faint` returns to `#5f5e5a` for decorative use; a second token carries the
lifted value for the capture hints.

### 1.3 A Return must state why, but three of the four triggers cannot

The package: *"The reason is mandatory — a return that cannot show why it came back must not
appear."* The prototype's `returnFor()` only ever produces a repeated-language pair, and renders
exactly one sentence: `because at 11:05 today you wrote "…decide what a thing is before…"`.

`returns.rs` implements four triggers from the product direction — repeated language, same source,
line continued, interval. Only the first two can produce "because … you wrote/opened …". A
`line_continued` or `interval` Return has no causing utterance to quote.

**Options:** (a) drop the two triggers that cannot state a reason; (b) give each trigger its own
"because" clause in the same sentence shape (`because you opened theatlantic.com again at 16:48
today` / `because you added to this line in march`) and drop only `interval`; (c) keep all four and
relax the rule. **Recommendation: (b).** It keeps the rule literally true, keeps the triggers the
product direction asked for, and only loses `interval`, which is the one trigger that genuinely has
no reason to give.
**DECIDED: (b)** — every surviving trigger states a because-clause in the prototype's sentence
shape; `interval` is dropped.

### 1.4 Tier windows: the README table and the prototype code disagree

`README.md` says D0 today / D1 this week / D2 this month / D3 this year / D4 older. The running
`bandsFor()` says:

| Tier | Prototype code | README prose |
|---|---|---|
| D0 | `dt < 8h` | today |
| D1 | `at >= todayStart − 1d` (today or yesterday) | this week |
| D2 | `dt < 7d` | this month |
| D3 | `dt < 60d` | this year |
| D4 | `dt < 180d` | older |
| years | `≥ 180d` | beyond D4 |

Taking the code (per §Method). Flagging because the README is the document you would read later and
it will be wrong.

### 1.5 The eight-second bring-back vs. sync publish

The package's open question 2, restated here because it is a data-integrity item, not a design one:
removal must not publish for eight seconds, and `notifyEntryDeletedForSync` currently fires
immediately. Until that is deferred, `bring back` can reinstate a fragment locally that the phone has
already destroyed. **This blocks shipping the quiet line's undo as designed.**

### 1.6 "Both wordings are kept" cannot survive the legacy sync contract

The conflict surface ("one moment was worded twice · both kept") requires the earlier wording to
travel between devices. The legacy contract is `{id, text, created_at}` with last-write-wins and
mobile is still on it. The compatibility bridge must be preserved and rollback-safe.

**Proposed:** build the surface and drive it from local revision history only (the Record already
keeps `prevText`/`correctedAt`), so a correction made on this Mac against an older remote wording is
recoverable here. A genuine cross-device conflict is not detectable until the contract carries the
earlier wording — a separate, additive change to make once mobile moves. Flagging so the surface is
not mistaken for full coverage.

### 1.7 Device identity does not exist

The sync "on" surface lists devices with last-seen and a `remove` that the other device honours.
Firestore currently holds a user, not devices. This needs a new additive collection plus a revoke the
other end respects; the device rows cannot be faked, and an invented "a moment ago" would be a lie.
**Proposed:** implement device registration additively now, and render the device list only when it
has real data.

### 1.8 The launch mark's stroke

The prototype's launch lockup draws the ring at `stroke-width 1.6` in a 64 viewBox at 132px. The
Identity file and the README both say the ≥40px rung is stroke 2.5, "drawn, not scaled". At 132px
that is 3.3 device px vs 5.2 — visibly different. Following the desktop prototype (1.6) since it is
the drawn artefact, but flagging: if the intent was 2.5 everywhere, launch is the one place it was
missed.

---

## 2. Already matches

Correct as built; no work.

- **The mark's three rungs.** `Mark.tsx` geometry is identical to the prototype and the Identity
  file: `r28/2.5` + dots `8@23, 4.5@38, 2.5@47.5`; `r28/3.5` + `9@23, 5@40`; `r27/6` + `11@27`.
  Each rung drawn, not scaled. `MarkTile` radius `0.225` = 185/824, mark at 0.625 ≈ the specified
  0.62.
- **Column geometry.** 940px column, left-biased offset, 46px time gutter, 22px gap, 68px indent.
- **The ink ladder**, for the five steps that exist: `#e6e6e3 / #cfcfcc / #c9c9c6 / #b4b4b2 /
  #8f8e89`, plus `#3a3a40` rules, `rgba(230,230,227,0.16)` match, `…0.04` row hover, `…0.25`
  selection.
- **D0/D1/D2 type**: 26/1.24/−0.012em/100, 19/1.26/92 clamp 3, 14/1.3/84 clamp 2. Gaps 24/12/7.
- **Capture.** 44px, −0.02em, auto-growing textarea, field cleared *before* anything else happens,
  hint row `⏎ leave it · ⇧⏎ new line · esc drop it`, `◌ hold space to speak` right-aligned.
- **Held.** 19px `wdth 96`, 2px `#3a3a40` rail at the gutter, `held here from {day} · release`.
- **Empty record.** `type anything and press return. it lands here, and stays.` at 26px meta,
  indented past the gutter, and correctly withheld until the first read resolves.
- **Correcting.** In place, `inset 0 -2px`, with the exact sentence *changes the wording only — the
  moment keeps its date and the earlier wording* and `⏎ save` / `esc cancel`.
- **⋯ contents and order**: `correct · copy · copy link · where it came from`, `remove` pushed right.
- **Speaking bars.** Five 3px bars at 0.9 / 1.1 / 0.7 / 1.3 / 0.8s, held at full height under
  `prefers-reduced-motion` rather than frozen at `scaleY(0.4)`.
- **Tauri wiring that the prototype assumes**: `⌘⇧K` is already the registered global capture
  shortcut, `⌥space` is already registered for voice-hold, and the tray already ships with
  `icon_as_template(true)`.
- **Year bars' basic form**: 5px wide, 3px gap, 3px minimum, empty months drawn in the rule colour.

## 3. Visually close — refinement only

| What | Prototype | Here |
|---|---|---|
| Window padding | `64px 48px 200px clamp(24px,12vw,200px)` | `72px 24px 120px clamp(24px,(100vw−940)*0.4,200px)` |
| Band top margins | D0 22 · D1 36 · D2 28 · D3 22 · D4 20 · years 22 | D0 22 · D1 36 · D2 28 · D3 22 (no D4) |
| Band label sizes | 11 → 10 → 9 → 9px, `0.06em` upper, meta, **clickable** | 11 → 10 → 9px, not clickable |
| D0 row box | `margin: 0 −20px; padding: 6px 20px` so the hover wash bleeds past the text | no negative margin; wash is text-width |
| Voice chip | 14px, `1px #3a3a40`, `padding 3px 10px 3px 8px`, `vertical-align:4px`; compact tiers get an 11px variant | single size |
| Quote | 22px, 18px indent behind a 2px rail, `margin-bottom:10px` | present, spacing differs |
| Standing bar copy | `▲ today · 3 new since you left · esc` | `▲ today · 3 fragments since you left · esc` |
| About lockup | 30px mark + 28px wordmark, then `2.0.0 · up to date · checked at launch`, `the manifesto · on getchinotto.app` | `Wordmark()` is 16px mark + 17px word + `an instrument by PROBNAYA` — a strapline the package explicitly forbids on the lockup |
| Focus header | `a fragment · 12 march 2024` / `a line · 7 moments · 2019 → today` | `one moment` / `a line · N moments · {span}` |

## 4. Behaves differently

1. **The record is a sequence of bands, not a fixed tier stack.** `bandsFor()` walks the
   time-sorted list and opens a new band whenever the level changes, so the same tier can appear
   more than once with different labels and the reading order is strictly chronological. `Now.tsx`
   renders one fixed block per tier (`renderTier("d0") … renderTier("d3")`), which reorders material
   whenever a tier boundary falls mid-run.
2. **Held sits above the record, not below D0.** Prototype order: return → held → record. Here:
   D0 → held → D1…
3. **Find is not a surface.** Typing `/` turns the capture field into find and the record filters
   live underneath, with `{n} in words · also by meaning · esc` to the right of the field. Here Find
   replaces the column entirely (`surface.kind === "find"`) and has its own layout, its own caps and
   its own explanatory footer.
4. **Standing is entered by typing a date.** `march 2024`, `2019`, `today`, `now` + `⏎`, with a live
   `⏎ stand in march 2024` hint while you type. None of that parsing exists here; Standing is
   reachable only by clicking a band label, a year or a month tick. Standing also *replaces the
   capture field with the bar* rather than being a separate surface.
5. **Banding while standing is relative to where you stand** (`dm === 0 → 0, 1 → 1, ≤3 → 2, ≤8 → 3,
   ≤14 → 4, else years`), and D0's label becomes `march 2024 · 17`. `Standing.tsx` does its own
   grouping on different rules.
6. **`esc` is one global ladder**: tray → recording → correcting → verb row → surface → focus →
   draft → standing. Here each surface handles its own escape and there is no global handler, so
   e.g. `esc` with a draft present inside Standing does the wrong thing.
7. **`⋯` opens an inline verb row under the text, never a popover.** `MoreMenu.tsx` is an
   absolutely-positioned menu anchored to the click.
8. **Undo is 8 seconds in the quiet line with a live countdown and `⌘Z`.** Here it is a 10-second
   centred `role="status"` toast with an `undo` button and no shortcut.
9. **The just-saved window is 12s** with a `still yours to change · {n}s` countdown *and* a
   continuation offer (`continues yesterday's "…"? yes · no`) that joins two fragments into a line
   on accept. Here the footnote is present but there is no countdown and no continuation offer.
10. **Year bars are absolute, not relative.** Prototype: `height = 3 + min(13, count × 1.4)`, so
    bars are comparable across years. `YearRow.tsx` scales each year against its own busiest month,
    which was a deliberate decision ("the shape of a year, not a comparison between years") that the
    prototype has now superseded.
11. **The years band starts at 180 days**, not at the calendar year boundary. `RecordApp.tsx` only
    folds years strictly older than the current one.
12. **Continuation offers create lines.** `acceptSuggestion()` stamps a shared `lineId` on both
    fragments. `linkContinuation` exists in the API but nothing calls it.
13. **Fragment-focus `open(id, cont)` focuses the continuation field only when you arrived via
    `continue`.** Here `continue` and `open` both land on the same surface with the same focus.
14. **URL fragments state their offline condition**: `domain · shared, title arrives when you are
    back online`. Here the wording is close but the offline branch is not driven by connectivity.
15. **`let go` animates for 450ms before hiding** (`translateY(60px)`, fade). Here the Return
    collapses via a grid-rows transition on a different curve.

## 5. Missing

Nothing in this list exists on the branch in any form.

- **Launch.** No launch state at all. Needs the full-viewport lockup, `arrive1/2/3` → `ringclose` →
  `riseword`, hold `max(0, 2.6s − time already loading)`, a 0.52s exit with `scatter*`/`ringopen`/
  `wordout` and the record rising underneath, reduced-motion static hold of 0.5s, and the
  invisible-but-centred pre-corpus state.
- **Settings.** No settings surface exists. Ten sections: sync · from anywhere · appearance · text ·
  dock icon · the record · privacy · keys · account · about. `⌘,` is unbound.
- **Sync surface.** Six states (off / connecting / on / offline / sign-in expired / two wordings),
  device list, in-place confirms, the permanent footnote. Sync currently runs **headlessly**:
  `SyncModal.tsx` is orphaned and nothing renders it, so there is no sync UI on this branch at all.
- **The quiet line.** Fixed `bottom:22px`, `pointer-events:none` except its own words, undo → sync
  notice → update on the left, `● sync on` and `settings ⌘,` right-aligned, hidden during launch and
  while the tray panel is open.
- **Traces.** `traces · 2 seen, 1 guessed`, a 110px kind label (`same words` / `a guess`, italic for
  guesses), the snippet with the shared phrase highlighted, its date, and `why · yes · not this` for
  guesses. `sameSourceEncounters`, `findByMeaning` and `meaning.rs` exist as building blocks;
  nothing assembles them.
- **Tier D3** (13px / `wdth 80` / `#aaa9a4` / 1 line / gap 5 / — the "body dim" step). The ladder here
  is four tiers, so the current `d3` is the prototype's D4 and the real D3 is absent, along with its
  ink token.
- **Voice at the edge.** Hold `space` on an empty field → level meter + elapsed at 44px + live
  transcript at 26px italic; release under 0.8s drops silently. The three microphone states
  (not asked / denied with the system-settings sentence auto-clearing after 7s / allowed).
  `EXPERIMENTAL_VOICE_CAPTURE` is `false`.
- **Text zoom.** `⌘+` / `⌘−`, 80–140%, `as designed` at 100%.
- **Appearance control** (system / light / dark) and `lift contrast in bright light`.
- **Export the record** (zip of plain text + audio) and **back up now**, with the 4-second
  `saved to downloads · chinotto-record.zip`.
- **Account deletion** on the Mac: the two-step in-place confirm and its copy.
- **`↓` into the record from capture with `c` / `h`** — partially present (arrow keys + c/h exist in
  `Now.tsx`) but not reachable the prototype's way and not part of the `esc` ladder.
- **Year-row count and first-line run.** `YearRow` accepts `firstLines` and `count`; `RecordApp`
  passes `firstLines: []` and no count, so both are always empty.
- **Multi-paragraph disclosure** (`↓ N more paragraphs`) — the prototype expands in place; here the
  long body is rendered in full.
- **Line meta on a D0 row**: `↳ first moment of a line · continued N times` / `↳ moment N of a line ·
  since march`.

## 6. Built to a decision the prototype has since changed

| Built as | Prototype now says | Where |
|---|---|---|
| Four tiers, D0–D3 | Five, D0–D4, plus the years band | `tiers.ts`, `tokens.css`, `Now.tsx` |
| `D0_WINDOW_HOURS = 6` (inferred midpoint) | 8 hours, stated in code | `tiers.ts` |
| D3 = `wdth 76`, 12px | that is D4; D3 is 13px / `wdth 80` / `#aaa9a4` | `tiers.ts` |
| Now is bounded, overflow becomes `N more ↓` | no caps, no overflow affordance | `Now.tsx` (see §1.1) |
| Year bars relative to the year's own peak | absolute, `3 + min(13, count × 1.4)` | `YearRow.tsx` |
| Years fold at the calendar-year boundary | at 180 days | `RecordApp.tsx` |
| Find is a surface | Find is a mode of the capture field | `Find.tsx`, `RecordApp.tsx` |
| `⌘⇧C` returns to capture in-window | not in the key map; `⌘⇧K` is the only capture chord | `RecordApp.tsx` |
| Return reason on the header line, plus a dated evidence list | header is `back from {full date} · {ago}`; one `because at {time} {day} you wrote "…"` sentence with the matched phrase highlighted | `ReturnBlock.tsx` |
| Return verbs `continue · open · hold` | `continue · open the line · hold` | `ReturnBlock.tsx` |
| `⋯` as an anchored popover | an inline verb row | `MoreMenu.tsx` |
| Undo 10s, centred toast | 8s, in the quiet line, with `⌘Z` | `RecordApp.tsx` |
| `--faint` lifted to `#878682` | `#5f5e5a` (see §1.2) | `tokens.css` |
| Wordmark carries `an instrument by PROBNAYA` | lockup is mark + wordmark only, never with a tagline | `Mark.tsx` |
| Ten alternate app icons | two: dark and light | `iconVariants.ts`, `setDesktopIcon.ts` |
| `⌘⇧V` voice shortcut | removed — hold is the model (`space`, `⌥space`) | `lib.rs` |

## 7. Legacy supporting surfaces that can now be replaced

All three modal surfaces are currently **orphaned** — reachable from nothing since `App.tsx` was
deleted — so replacing them removes dead code rather than working features.

| Legacy | Replaced by | Notes |
|---|---|---|
| `SyncModal.tsx` (450 lines, QR modal, `react-qr-code`) | Sync surface | QR stays (168px), the modal does not |
| `AnalyticsOptInModal.tsx` (timed modal) | Settings → privacy, expanding in place | the package forbids the modal form outright |
| `UpdateNudge.tsx` + `appUpdater.ts` | quiet line (`chinotto 2.0.1 is out · download` → `downloading 2.0.1…` → `update ready · restart to finish`) + the about line | `appUpdater.ts` keeps its phases; only the UI goes |
| `IconVariantShowcase.tsx`, `iconVariants.ts` (10 variants) | Settings → dock icon, two 56px tiles + 32/16px previews | `setDesktopIcon.ts` stays, re-pointed |
| `ChinottoLogo.tsx` (old four-dot mark, breathe animation) | `Mark.tsx` | delete once `TrayCapturePanel` and `SyncModal` stop importing it |
| `TrayCapturePanel.tsx` | prototype menu-bar panel | **also a data change**: it writes to the legacy `entries` table via `entryApi`, so a menu-bar capture never becomes a first-class fragment with its own source. It must capture into the Record and mirror out through the bridge |
| centred `role="status"` notice + `offline` corner | the quiet line | |
| `src/index.css` (6 227 lines: Tailwind + the whole v1 app shell) | `tokens.css` + inline styles | only `.tray-capture-*` is still live |
| four unused Google font families + the `#grain` SVG filter in `index.html` | — | the Identity file lists the fonts for removal |

## 8. Identity and macOS system surfaces

**Every shipped raster is still the old mark** — a ring with four scattered dots, periwinkle
`#8a94c8` on `#0a0a0e`. The new mark is a ring with three dots receding down a column, ink
`#e6e6e3` on `#141416`. `Mark.tsx` is already correct (§2); nothing else is.

| Asset | State | Action |
|---|---|---|
| `src-tauri/icons/tray_menu_template.{svg,png}` | old four-dot mark, ring `r22/2`, 36×36 with a `scale(1.32)` optical upscale | redraw as the ≤20px rung (`r27/6`, one dot `r11@27`), pure black on transparent, **17pt glyph in a 22pt box**, shipped @1x and @2x; drop the 1.32 upscale — the glyph is drawn at final size. `icon_as_template(true)` already correct |
| `src-tauri/icons/icon.svg`, `icon.png`, `icon_1024.png`, `icon.icns`, `icon.ico`, `32`, `64`, `128`, `128@2x` | old mark, periwinkle, `scale(0.82)` safe-area hack | regenerate from one 1024 master: mark at 0.62 of an 824 square inside 1024, corner radius 185, stroke 3 |
| `src-tauri/icons/Square*Logo.png`, `StoreLogo.png` | old mark | same master, square, no rounding |
| `src-tauri/icons/ios/`, `android/` | old mark, unused in the Tauri tree | delete rather than redraw |
| `src-tauri/dmg-background.png` (+ generator) | old mark | lockup on the ink field, 22px wordmark minimum |
| `public/favicon.svg`, `favicon.ico`, `favicon-32.png` | old mark, periwinkle | 16pt rung on the ink field; the `.ico` carries 16 and 32 drawn separately |
| `docs/logo.svg` | old mark, five circles | the lockup, as the canonical file both repos reference |
| `index.html` | four unused Google font families | remove; keep the favicon links |
| `iconVariants.ts`, `setDesktopIcon.ts`, `IconVariantShowcase.tsx` | ten variants + a showcase route | two variants, dark and light; delete the showcase and its `#icon-variants` route |
| `Info.plist` microphone usage string | not the designed string | *"Chinotto" would like to access the microphone — so you can hold space and speak instead of typing. The audio stays on this mac.* This is the one place macOS draws our icon at 64pt with our name |
| `builds/*.app`, the two shipped dmgs | old icon baked in | rebuild, not editable |

The mobile half of the inventory (`chinotto-mobile`, the iOS widget, the five colour variants) is
out of scope for this branch but is listed in the Identity file and should be tracked.

## 9. Policy / heuristics inventory — what changes

`docs/unspecified-decisions.md` was written against Distance v2. After this handoff:

**Resolved by the prototype** (move out of the inventory, into implementation):
§1.1 D0 window (6h → 8h, now stated) · §2.1/2.2 tier caps (no caps — pending §1.1 above) ·
§2.3 folded-line head/tail (2/2 confirmed by `line.slice(0,2)` / `slice(-2)`) · §5b.1 the ⋯
container (an inline verb row, not a menu) · §5b.4 undo window (10s → 8s) · §8.3/8.4/8.5 motion
(the prototype names every duration and easing) · §9.1/9.2/9.3 width axes (D0–D4 100/92/84/80/76,
meta 90, moment 94 — all now stated).

**Newly unspecified** (add to the inventory):
- The prototype's `bandsFor()` has no tier caps but the product direction requires a bounded Now —
  whatever is chosen in §1.1 is a decision, not an inheritance.
- Menu-bar capture is "a source here, and only here", but the prototype's `sourceOf()` has no
  `menubar` branch and renders it as `typed`. The provenance wording for a menu-bar capture is
  undefined.
- `⌥space` speak-from-anywhere has no drawn surface — there is no window to show a level meter in.
- Export format beyond "plain text and audio, zipped": no file layout is specified.
- Backup location, retention and what "a backup is made each time chinotto opens" keeps.
- `lift contrast in bright light` has no token deltas.
- The light appearance is a token pass only; no light screens are drawn (the package says so).
- Which return triggers survive §1.3, and the "because" clause for each.

**Unchanged and still live:** §3 (return thresholds), §4 (meaning cutoffs), §5 (URL normalisation),
§5c (the legacy bridge), §6 (hold limit), §7.1/7.2 (window size), §10 (migration policy).

---

## 10. Implementation plan

Ordered so each phase is independently verifiable against the prototype, and so the riskiest
shared foundation lands first. Everything stays on `feat/chinotto-next-desktop` and uncommitted.
Backend and domain code is reused, not rewritten; the compatibility bridge and the rollback-safe
data model are preserved throughout.

**Phase 0 — foundations**
`tokens.css`: add the missing ink step (`#aaa9a4`), the five-tier scale, the prototype's window
padding, and every named animation (`settle`, `rise`, `rec`, `arrive1/2/3`, `ringclose`, `riseword`,
`scatter1/2/3`, `ringopen`, `wordout`) with its exact duration, delay and easing.
`tiers.ts`: D0–D4 with the prototype's windows, wdth, clamp, gap and indent.
New `bands.ts`: a faithful port of `bandsFor()` — band sequencing, labels (`earlier today` /
`yesterday` / day / month), the anchored variant, and years aggregation — with tests over the
hostile fixture and over the prototype's own corpus for parity.

**Phase 1 — the record column**
Rewrite `Now.tsx` to render bands rather than fixed tiers; move held above the record; clickable
band labels; year rows with absolute bar heights, count and first-line run (this needs
`RecordApp` to actually fetch first lines). `FragmentRow` at five tiers, with the D0 negative-margin
hover wash, the voice chip's two sizes, the quote rail, URL meta including the offline wording,
`↓ N more paragraphs`, line meta and the inline provenance line. Replace `MoreMenu` with the inline
verb row. Add the just-saved window: `settle`, the inset underline, the 12s countdown, and the
continuation offer wired to `linkContinuation`.
*Depends on the §1.1 decision.*

**Phase 2 — the edge**
Caret visibility rule (`showBar = !input && !inputFocused`); date-phrase anchoring with the live
`⏎ stand in …` hint; `/` turning the field into find with the record filtering underneath (deleting
the Find surface); the standing bar replacing the capture field; one global `esc` ladder.

**Phase 3 — focus and traces**
Header copy, the line rail and dots, the folded middle, the continuation field with its open square,
hover verbs, `wording corrected · show`. Then traces: assemble `sameSourceEncounters` +
`findByMeaning` + a shared-run matcher into `same words` / `a guess` rows with `why · yes · not this`,
and make `yes` join the line.

**Phase 4 — the return**
Reshape `ReturnBlock` to the prototype's four lines and verbs, with the 450ms `let go`.
Backend: `returns.rs` gains the causing fragment on the Return (additive `return_evidence` row) and
computes a shared *run* rather than a single distinctive term, so the "because …" sentence can quote
and highlight real words. *Depends on the §1.3 decision.*

**Phase 5 — the quiet line and undo**
The fixed line with its three left slots and two right slots. Move undo into it at 8s with `⌘Z` and
a countdown; delete the centred notice and the offline corner; delete `UpdateNudge`.
Backend: defer the sync tombstone by eight seconds (§1.5) — **this phase does not ship without it.**

**Phase 6 — Settings**
All ten sections, `⌘,`, `esc` back to the edge. Text zoom, appearance + sun, dock icon (two
variants), export and backup, privacy expanding in place (replacing `AnalyticsOptInModal`), the key
list, account deletion's two-step, about. Light-appearance token pass.

**Phase 7 — Sync**
The six states, `esc` back to settings, in-place confirms, the permanent footnote. Additive device
registration in Firestore so the device list is real (§1.7). The conflict surface driven from local
revision history, with its limits recorded (§1.6). Legacy contract untouched.

**Phase 8 — Launch**
The lockup, the arrival choreography, the hold that never extends a slow load, the exit, and the
record's `rise` underneath. Reduced-motion path.

**Phase 9 — menu-bar capture**
The 440px panel redesigned to the prototype; capture goes into the Record with a menu-bar source and
mirrors out through the bridge; keep the 280ms blur debounce and the 520ms auto-close.

**Phase 10 — identity and system surfaces**
One 1024 master; regenerate every raster; the tray template at 22pt/17pt @1x/@2x without the upscale;
favicons; dmg background; `docs/logo.svg`; `iconVariants` 10 → 2; delete `IconVariantShowcase`,
`ChinottoLogo` and the unused `ios`/`android` icon folders; `index.html` font cleanup; the
`Info.plist` microphone string.

**Phase 11 — voice**
`EXPERIMENTAL_VOICE_CAPTURE` on, the authorisation prompt reached from the main thread, the
entitlement shipped, the three microphone states, and the edge's recording surface. Remove `⌘⇧V`.
*Needs a signed build to verify; sequenced last for that reason.*

**Phase 12 — cleanup and verification**
Delete the orphaned legacy components; prune `index.css` to what the tray still needs; rewrite
`unspecified-decisions.md` per §9; run the Rust and TS suites; full visual pass in the running Tauri
app against the prototype, at 1440×900 and at the minimum window size, with the hostile fixture and
with a realistic multi-year record.
