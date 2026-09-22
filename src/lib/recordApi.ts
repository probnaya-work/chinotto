/**
 * Typed client for the Record's Tauri commands.
 *
 * Mirrors `src-tauri/src/record_commands.rs`. Nothing here decides what capture, Continue
 * or Correct mean — those rules live in Rust so they hold however a call arrives. This file
 * only names the calls and their shapes.
 */

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import * as dev from "./recordDevData";
import { isFirebaseSyncConfigured } from "./firebaseConfig";
import { pushEntryUpsertToFirestore } from "./desktopFirestoreSync";
import { IS_MAC_APP_STORE } from "./distribution";

/**
 * Outside the Tauri shell there is no database, so in a dev browser the calls are served
 * from `recordDevData` instead. This is a development affordance only: in the packaged app
 * `isTauriShell()` is always true and every call below goes to SQLite.
 */
function devOnly(): boolean {
  return import.meta.env.DEV && !dev.isTauriShell();
}

interface DevStore {
  fragments: Fragment[];
  held: HeldFragment[];
  /** fragment id -> the ordered chain it belongs to. */
  devLines: Record<string, string[]>;
  /** fragment id -> earlier wordings, oldest first. */
  devRevisions: Record<string, string[]>;
  /** Removed but not destroyed, so `bring back` works here as it does in the app. */
  devRemoved: Record<string, Fragment>;
}
let devState: DevStore | null = null;
function devStore(): DevStore {
  const existing = devState;
  if (existing) return existing;
  const created: DevStore = {
    fragments: dev.devFragments(),
    held: dev.devHeld(),
    devLines: dev.devLines(),
    devRevisions: {},
    devRemoved: {},
  };
  devState = created;
  return created;
}

const invoke = tauriInvoke;

/**
 * Carries a body change out to the legacy sync protocol.
 *
 * Rust has already mirrored the fragment into `entries`; this is the push half. It lives
 * here, at the one boundary every write passes through, so a new kind of write cannot
 * silently fail to sync — which is exactly what happened to corrections the first time.
 *
 * Never awaited by a caller: capture is local and complete, and a failed push is the sync
 * layer's problem, not the person's.
 */
function carryToSync(f: Fragment): void {
  if (devOnly() || !isFirebaseSyncConfigured()) return;
  if (!f.body.trim()) return;
  void pushEntryUpsertToFirestore({
    id: f.id,
    text: f.body,
    created_at: f.capturedAt,
  }).catch(() => {});
}

export type CaptureMethod = "typed" | "voice" | "url" | "shared" | "imported";

export interface Fragment {
  id: string;
  body: string;
  /** RFC3339. Immutable: correcting a fragment never moves it in time. */
  capturedAt: string;
  captureMethod: CaptureMethod;
  captureOrigin: string | null;
  correctedAt: string | null;
  /** Always equal to the number of earlier wordings actually held. */
  correctionCount: number;
  /**
   * How many times v1 overwrote this text in place. Those wordings are gone and cannot be
   * shown; this is why a migrated fragment can read as never corrected.
   */
  legacyEditCount: number;
}

export interface Revision {
  body: string;
  supersededAt: string;
  revisionIndex: number;
}

export interface LineMoment {
  fragment: Fragment;
  position: number;
  isContinuation: boolean;
}

export interface HeldFragment {
  fragment: Fragment;
  /** When it was put here — "held here from wed". */
  heldAt: string;
}

export interface FindHit {
  fragment: Fragment;
  /** "body" | "transcript" | "source" — Find must be able to say which material answered. */
  matchedIn: "body" | "transcript" | "source";
}

/**
 * A fragment carried over from v1 has no recorded capture method, so the interface shows no
 * method label rather than asserting one it does not know.
 */
export function hasKnownCaptureMethod(f: Fragment): boolean {
  return f.captureMethod !== "imported";
}

/** Leave a fragment in the Record. */
export function captureFragment(
  body: string,
  captureMethod: CaptureMethod = "typed",
  captureOrigin = "desktop",
): Promise<Fragment> {
  if (devOnly()) {
    const f: Fragment = {
      id: `dev-new-${Math.random().toString(36).slice(2)}`,
      body,
      capturedAt: new Date().toISOString(),
      captureMethod,
      captureOrigin,
      correctedAt: null,
      correctionCount: 0,
      legacyEditCount: 0,
    };
    devStore().fragments.unshift(f);
    return Promise.resolve(f);
  }
  return invoke<Fragment>("capture_fragment", { body, captureMethod, captureOrigin }).then((f) => {
    carryToSync(f);
    return f;
  });
}

/** A new dated moment continuing an earlier one. The earlier one is not touched. */
export function continueFragment(
  continuesId: string,
  body: string,
  captureMethod: CaptureMethod = "typed",
  captureOrigin = "desktop",
): Promise<Fragment> {
  if (devOnly()) {
    const st = devStore();
    const f: Fragment = {
      id: `dev-cont-${Math.random().toString(36).slice(2)}`,
      body,
      capturedAt: new Date().toISOString(),
      captureMethod,
      captureOrigin,
      correctedAt: null,
      correctionCount: 0,
      legacyEditCount: 0,
    };
    st.fragments.unshift(f);
    const existing = st.devLines[continuesId] ?? [continuesId];
    const chain = [...existing, f.id];
    for (const memberId of chain) st.devLines[memberId] = chain;
    return Promise.resolve(f);
  }
  // A continuation is its own entry on the legacy side — never appended to the earlier one.
  return invoke<Fragment>("continue_fragment", {
    continuesId,
    body,
    captureMethod,
    captureOrigin,
  }).then((f) => {
    carryToSync(f);
    return f;
  });
}

/** "yes, that continues yesterday's note" — links without re-dating the capture. */
export function linkContinuation(fragmentId: string, continuesId: string): Promise<void> {
  return invoke<void>("link_continuation", { fragmentId, continuesId });
}

/** Repair wording. Keeps the wording it replaced; does not move the moment in time. */
export function correctFragment(id: string, body: string): Promise<Fragment | null> {
  if (devOnly()) {
    const st = devStore();
    const f = st.fragments.find((x) => x.id === id);
    if (f && f.body !== body) {
      (st.devRevisions[id] ??= []).push(f.body);
      f.body = body;
      f.correctedAt = new Date().toISOString();
      f.correctionCount += 1;
    }
    return Promise.resolve(f ?? null);
  }
  return invoke<Fragment>("correct_fragment", { id, body }).then((f) => {
    carryToSync(f);
    return f;
  });
}

/** Every wording this fragment has had, oldest first, ending with the current one. */
export function fragmentHistory(id: string): Promise<Revision[]> {
  if (devOnly()) {
    const st = devStore();
    const prior = st.devRevisions[id] ?? [];
    const current = st.fragments.find((f) => f.id === id);
    const out = prior.map((body, i) => ({ body, supersededAt: "", revisionIndex: i }));
    if (current) out.push({ body: current.body, supersededAt: current.correctedAt ?? "", revisionIndex: out.length });
    return Promise.resolve(out);
  }
  return invoke<Revision[]>("fragment_history", { id });
}

/** The whole Line containing this fragment, first moment forward. */
export function lineFor(id: string): Promise<LineMoment[]> {
  if (devOnly()) {
    const st = devStore();
    const chain = st.devLines[id] ?? [id];
    const moments = chain
      .map((fid) => st.fragments.find((f) => f.id === fid))
      .filter((f): f is Fragment => Boolean(f))
      .map((fragment, i) => ({ fragment, position: i, isContinuation: i > 0 }));
    return Promise.resolve(moments);
  }
  return invoke<LineMoment[]>("line_for", { id });
}

/** How much may be kept present at once. Bounded by intent, not by storage. */
export const MAX_HELD = 5;

/**
 * Keep present. Returns `[heldCount, taken]`; `taken === false` means the bound was reached
 * and nothing changed — nothing is auto-released to make room.
 */
export function holdFragment(id: string): Promise<[number, boolean]> {
  if (devOnly()) {
    const st = devStore();
    const f = st.fragments.find((x) => x.id === id);
    if (!f) return Promise.resolve([st.held.length, false]);
    if (st.held.some((h) => h.fragment.id === id)) return Promise.resolve([st.held.length, true]);
    if (st.held.length >= MAX_HELD) return Promise.resolve([st.held.length, false]);
    st.held.push({ fragment: f, heldAt: new Date().toISOString() });
    return Promise.resolve([st.held.length, true]);
  }
  return invoke<[number, boolean]>("hold_fragment", { id });
}

/** Release lets material recede into the Record. It is not a delete. */
export function releaseFragment(id: string): Promise<void> {
  if (devOnly()) {
    const st = devStore();
    st.held = st.held.filter((h) => h.fragment.id !== id);
    return Promise.resolve();
  }
  return invoke<void>("release_fragment", { id });
}

export function heldFragments(): Promise<HeldFragment[]> {
  if (devOnly()) return Promise.resolve([...devStore().held]);
  return invoke<HeldFragment[]>("held_fragments");
}

export function recentFragments(limit = 50): Promise<Fragment[]> {
  if (devOnly()) return Promise.resolve(devStore().fragments.slice(0, limit));
  return invoke<Fragment[]>("recent_fragments", { limit });
}

/**
 * How much of the Record the surface holds at once.
 *
 * The record arranges *everything* by distance, and standing in a month re-measures that
 * distance from there — so the surface cannot work from a recent page. It needs the whole
 * thing. Rows are bodies and timestamps, so a very large record is megabytes rather than
 * gigabytes, and rendering is windowed rather than bounded by this number.
 *
 * The cap exists only so a pathological record cannot exhaust memory before anything is
 * drawn. It is a safety limit, not a design one. See docs/unspecified-decisions.md.
 */
export const RECORD_CAP = 50_000;

/** The whole Record, newest first. */
export function allFragments(): Promise<Fragment[]> {
  return recentFragments(RECORD_CAP);
}

/** One page of the Record, walking backwards. Cursor-based, for traversal at scale. */
export function fragmentsBefore(before: string | null, limit = 100): Promise<Fragment[]> {
  return invoke<Fragment[]>("fragments_before", { before, limit });
}

/** Material inside a half-open time range, oldest first. Backs standing in a month. */
export function fragmentsBetween(from: string, to: string, limit = 500): Promise<Fragment[]> {
  if (devOnly()) {
    return Promise.resolve(
      devStore()
        .fragments.filter((f) => f.capturedAt >= from && f.capturedAt < to)
        .sort((a, b) => (a.capturedAt < b.capturedAt ? -1 : 1))
        .slice(0, limit),
    );
  }
  return invoke<Fragment[]>("fragments_between", { from, to, limit });
}

/** Exact textual retrieval. Find's primary answer. */
export function findFragments(query: string, limit = 200): Promise<FindHit[]> {
  if (devOnly()) {
    // Mirrors the Rust behaviour closely enough to exercise the surface: every whitespace
    // token must be present, matching is case-insensitive, and nothing is ranked by cleverness.
    const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
    if (terms.length === 0) return Promise.resolve([]);
    return Promise.resolve(
      devStore()
        .fragments.filter((f) => terms.every((t) => f.body.toLowerCase().includes(t)))
        .sort((a, b) => (a.capturedAt < b.capturedAt ? 1 : -1))
        .slice(0, limit)
        .map((fragment) => ({ fragment, matchedIn: "body" as const })),
    );
  }
  return invoke<FindHit[]>("find_fragments", { query, limit });
}

/** Where material exists across a year's months — not an analysis of it. */
export function monthDensity(year: number): Promise<number[]> {
  if (devOnly()) return Promise.resolve(dev.devMonthDensity(year));
  return invoke<number[]>("month_density", { year });
}

export interface Encounter {
  id: number;
  fragmentId: string;
  /** Exactly as received, before any canonicalisation. */
  urlRaw: string;
  urlKey: string;
  /** Only when the OS/share path actually supplied one. */
  sourceApp: string | null;
  sharedAt: string;
  /** What the source itself supplied, distinct from the person's words. */
  selectedText: string | null;
  // --- derived; any of these may be absent, stale, or never arrive ---
  urlCanonical: string | null;
  domain: string | null;
  title: string | null;
  siteName: string | null;
  enrichmentState: "pending" | "ok" | "failed";
  fetchedAt: string | null;
  failure: string | null;
  timesMet: number;
}

export interface VoiceCapture {
  fragmentId: string;
  audioPath: string;
  durationMs: number;
  recordedAt: string;
  /** The audio was looked for and is not there. The words remain. */
  audioMissing: boolean;
  // --- derived ---
  machineTranscript: string | null;
  transcriptState: "pending" | "ok" | "failed";
  transcribedAt: string | null;
  failure: string | null;
  /** The body no longer matches what the machine heard. */
  transcriptCorrected: boolean;
}

export interface Materials {
  encounters: Map<string, Encounter>;
  voices: Map<string, VoiceCapture>;
}

/** Material for a page of fragments, in one pass rather than one query per row. */
export async function materialsFor(ids: string[]): Promise<Materials> {
  if (ids.length === 0) return { encounters: new Map(), voices: new Map() };
  const [encounters, voices] = devOnly()
    ? dev.devMaterials(ids)
    : await invoke<[Encounter[], VoiceCapture[]]>("materials_for", { ids });
  return {
    encounters: new Map(encounters.map((e) => [e.fragmentId, e])),
    voices: new Map(voices.map((v) => [v.fragmentId, v])),
  };
}

export function captureEncounter(
  urlRaw: string,
  body = "",
  sourceApp: string | null = null,
  selectedText: string | null = null,
): Promise<Fragment> {
  if (devOnly()) {
    const st = devStore();
    const f: Fragment = {
      id: `dev-enc-${Math.random().toString(36).slice(2)}`,
      body,
      capturedAt: new Date().toISOString(),
      captureMethod: sourceApp ? "shared" : "url",
      captureOrigin: "desktop",
      correctedAt: null,
      correctionCount: 0,
      legacyEditCount: 0,
    };
    st.fragments.unshift(f);
    return Promise.resolve(f);
  }
  return invoke<Fragment>("capture_encounter", {
    urlRaw,
    body,
    sourceApp,
    selectedText,
    captureOrigin: "desktop",
  }).then((f) => {
    carryToSync(f);
    return f;
  });
}

export function encountersAwaitingEnrichment(limit = 8): Promise<[number, string][]> {
  if (devOnly()) return Promise.resolve([]);
  return invoke<[number, string][]>("encounters_awaiting_enrichment", { limit });
}

export function recordEnrichment(
  encounterId: number,
  found: { title?: string | null; urlCanonical?: string | null; siteName?: string | null } | null,
  failure: string | null,
): Promise<void> {
  return invoke<void>("record_enrichment", {
    encounterId,
    title: found?.title ?? null,
    urlCanonical: found?.urlCanonical ?? null,
    siteName: found?.siteName ?? null,
    failure,
  });
}

export function sameSourceEncounters(fragmentId: string): Promise<[string, string, string | null][]> {
  if (devOnly()) return Promise.resolve([]);
  return invoke<[string, string, string | null][]>("same_source_encounters", { fragmentId });
}

export interface Guess {
  fragment: Fragment;
  /** Cosine similarity. Deliberately never rendered: it is a tie-breaker, not a reason. */
  score: number;
}

/** Guesses. Separate call from findFragments so exact retrieval never waits on the model. */
export function findByMeaning(query: string, exclude: string[], limit = 5): Promise<Guess[]> {
  if (devOnly()) return Promise.resolve(dev.devGuesses(devStore().fragments, query, exclude, limit));
  return invoke<Guess[]>("find_by_meaning", { query, exclude, limit });
}

/** "not this" — a judgement, kept in canonical material. */
export function rejectGuess(fragmentId: string, relatedId: string): Promise<void> {
  if (devOnly()) return Promise.resolve();
  return invoke<void>("reject_guess", { fragmentId, relatedId });
}

/** Embeds a slice of whatever still needs it. Opportunistic; never blocks a surface. */
export function embedPending(limit = 32): Promise<number> {
  if (devOnly()) return Promise.resolve(0);
  return invoke<number>("embed_pending", { limit });
}

export interface ReturnEvidence {
  kind: string;
  detail: string;
  occurredAt: string | null;
  relatedId?: string | null;
}

export interface ReturnValue {
  id: number;
  fragment: Fragment;
  reason: string;
  evidence: ReturnEvidence[];
  because?: Fragment | null;
}

/** At most one Return, or null. Null is the ordinary answer — silence is valid. */
export function selectReturn(): Promise<ReturnValue | null> {
  if (devOnly()) return Promise.resolve(dev.devReturn(devStore().fragments));
  return invoke<ReturnValue | null>("select_return");
}

/** 'opened' | 'continued' | 'let_go' | 'expired'. Letting go is not a delete. */
export function recordReturnOutcome(id: number, outcome: string): Promise<void> {
  if (devOnly()) return Promise.resolve();
  return invoke<void>("record_return_outcome", { id, outcome });
}

/** Soft in the Record, hard on the legacy row, tombstoned for sync. */
export function removeFragment(id: string): Promise<void> {
  if (devOnly()) {
    // Soft, exactly as the real one is: the material is set aside, not destroyed, or the
    // dev browser would quietly contradict the eight seconds the product promises.
    const st = devStore();
    const f = st.fragments.find((x) => x.id === id);
    if (f) st.devRemoved[id] = f;
    st.fragments = st.fragments.filter((x) => x.id !== id);
    st.held = st.held.filter((h) => h.fragment.id !== id);
    return Promise.resolve();
  }
  return invoke<void>("remove_fragment", { id });
}

/** Undo, for a removal made on this device. */
export function restoreFragment(id: string): Promise<void> {
  if (devOnly()) {
    const st = devStore();
    const f = st.devRemoved[id];
    if (f) {
      delete st.devRemoved[id];
      st.fragments = [...st.fragments, f].sort((a, b) =>
        a.capturedAt < b.capturedAt ? 1 : -1,
      );
    }
    return Promise.resolve();
  }
  return invoke<void>("restore_fragment", { id });
}

/** Catch-up for anything the bridge still owes the legacy table. */
export function mirrorPendingFragments(limit = 200): Promise<number> {
  if (devOnly()) return Promise.resolve(0);
  return invoke<number>("mirror_pending_fragments", { limit });
}

/** Pulls anything that arrived from mobile into the Record. */
export function projectEntriesIntoRecord(): Promise<number> {
  if (devOnly()) return Promise.resolve(0);
  return invoke<number>("project_entries_into_record");
}

/** First and last capture times, or null when the Record is empty. */
export function recordSpan(): Promise<[string, string] | null> {
  if (devOnly()) return Promise.resolve(dev.devRecordSpan());
  return invoke<[string, string] | null>("record_span");
}

// ---- this mac ---------------------------------------------------------------------------

/**
 * The whole Record as plain text and audio. Direct builds preserve the existing Downloads
 * destination; sandboxed App Store builds ask for a destination so macOS grants access.
 *
 * Plain text on purpose: an export exists so the record can outlive this program, and a
 * format only this program reads is not an escape hatch.
 */
export async function exportRecord(): Promise<string | null> {
  if (!IS_MAC_APP_STORE) {
    return invoke<string>("export_record", { path: null });
  }
  const path = await save({
    defaultPath: "chinotto-record.zip",
    filters: [{ name: "ZIP archive", extensions: ["zip"] }],
  });
  if (!path) return null;
  return invoke<string>("export_record", { path });
}

/** When the last automatic backup was taken, or null if there has never been one. */
export function lastBackupAt(): Promise<string | null> {
  return invoke<string | null>("last_backup_at");
}

export function createBackup(): Promise<void> {
  return invoke<void>("create_backup");
}

/** System Settings › Privacy › Microphone. The product cannot grant itself the microphone. */
export function openMicrophoneSettings(): Promise<void> {
  return invoke<void>("open_microphone_settings");
}

/**
 * The menu bar catches up with the Record.
 *
 * `today · n` and the glyph's waiting modifier are read in Rust; whether sync is configured
 * is a build-time frontend fact, so it is carried across rather than guessed at. Called
 * after every write, from whichever surface made it.
 */
export function refreshTray(syncOn?: boolean): Promise<void> {
  if (devOnly()) return Promise.resolve();
  return invoke<void>("refresh_tray", { syncOn: syncOn ?? null });
}

/** Shows the menu-bar panel, for settings' "try it". */
export function openTrayCapture(): Promise<void> {
  return invoke<void>("open_tray_capture");
}

/**
 * The menu-bar panel has changed size; the window around it follows.
 *
 * Both numbers are logical points, measured from the drawn panel. The backend clamps the
 * height against what is left of the screen below the menu bar, and re-centres the window
 * so a width change does not walk it out from under the glyph.
 */
export function fitCapturePopover(width: number, height: number): Promise<void> {
  if (devOnly()) return Promise.resolve();
  return invoke<void>("fit_capture_popover", { width, height });
}

// ---- devices and two wordings -----------------------------------------------------------

/** How many fragments have not reached the other devices yet. Read-only. */
export function fragmentsAwaitingMirror(limit = 500): Promise<number> {
  if (devOnly()) return Promise.resolve(0);
  return invoke<number>("fragments_awaiting_mirror", { limit });
}

/** This install's own `[id, name]`, created once and stable thereafter. */
export function thisDevice(): Promise<[string, string]> {
  return invoke<[string, string]>("this_device");
}

export interface WordingConflict {
  fragmentId: string;
  remoteText: string;
  localText: string;
  noticedAt: string;
  shows: "local" | "remote";
}

/** Moments worded in two places while the devices were apart, still unsettled. */
export function openWordingConflicts(): Promise<WordingConflict[]> {
  if (devOnly()) return Promise.resolve([]);
  return invoke<WordingConflict[]>("open_wording_conflicts");
}

/** Chooses which wording shows. The other stays under the moment as earlier wording. */
export function resolveWordingConflict(
  fragmentId: string,
  shows: "local" | "remote",
): Promise<void> {
  return invoke<void>("resolve_wording_conflict", { fragmentId, shows });
}

// ---- voice ------------------------------------------------------------------------------

/**
 * What one recording left behind.
 *
 * `transcript` may be null, and that is a complete outcome rather than a failure: the
 * recording is the material, the transcript is a machine's reading of it, and a reading can
 * be absent, late or wrong without the recording being any of those things.
 */
export interface VoiceCaptureResult {
  audioPath: string;
  durationMs: number;
  transcript: string | null;
  /** Why there are no words, when the mac said. Never a reason the audio failed. */
  transcriptFailure: string | null;
}

/** Hold to speak. Records to a file, and transcribes it if it can. */
export function recordVoice(maxMs?: number): Promise<VoiceCaptureResult> {
  return invoke<VoiceCaptureResult>("run_native_speech_recognition", { maxMs });
}

/** The hold was released. `maxMs` is only a ceiling; this is what normally ends a recording. */
export function stopVoiceCapture(): Promise<void> {
  return invoke<void>("stop_voice_capture");
}

/** The recording becomes a fragment. Called whether or not any words came back. */
export function captureVoice(
  audioPath: string,
  durationMs: number,
  captureOrigin = "desktop",
  /** When the recording ended. Omitted by the live path, which is already there. */
  endedAt?: string,
): Promise<Fragment> {
  return invoke<Fragment>("capture_voice", {
    audioPath,
    durationMs,
    captureOrigin,
    endedAt: endedAt ?? null,
  }).then((f) => {
    carryToSync(f);
    return f;
  });
}

/** A recording on disk that no fragment claims. */
export interface OrphanedRecording {
  audioPath: string;
  /** RFC3339, from the file itself: the moment the recording stopped. */
  endedAt: string;
  durationMs: number;
}

/** Recordings the Record has lost sight of — see `orphaned_recordings` in `lib.rs`. */
export function orphanedRecordings(): Promise<OrphanedRecording[]> {
  if (devOnly()) return Promise.resolve([]);
  return invoke<OrphanedRecording[]>("orphaned_recordings");
}

/** Attaches what the machine heard. Derived material: it never moves or replaces the audio. */
export function recordTranscript(
  fragmentId: string,
  transcript: string | null,
  failure?: string | null,
): Promise<void> {
  return invoke<void>("record_transcript", {
    fragmentId,
    transcript,
    model: "apple-speech",
    failure: failure ?? null,
  });
}

/** The audio was looked for and is not there. The words, if any, remain. */
export function markAudioMissing(fragmentId: string): Promise<void> {
  return invoke<void>("mark_audio_missing", { fragmentId });
}
