/**
 * The Record application shell.
 *
 * Nothing here is a container. There is no header, no sidebar, no tabs and no navigation:
 * there is one column of material, and the states below replace its contents rather than
 * wrapping it. `esc` always steps back toward the edge, one level at a time.
 *
 * The states are the edge (which is also find, and also standing in a month), focus on a
 * fragment or a line, and the utility surfaces. Only one is visible at a time.
 */

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "../../lib/recordApi";
import type { Fragment, HeldFragment } from "../../lib/recordApi";
import { Record } from "./Record";
import { Capture } from "./Capture";
import { FragmentFocus } from "./FragmentFocus";
import { ReturnBlock } from "./ReturnBlock";
import { QuietLine } from "./QuietLine";
import { Settings, type MicrophoneState } from "./Settings";
import { Sync, type SyncState } from "./Sync";
import { Launch, useLaunch } from "./Launch";
import { DROP_UNDER_MS, useVoice } from "./useVoice";
import { useNow } from "./useNow";
import { useAppleSyncOAuth } from "@/lib/useAppleSyncOAuth";
import { D0_WINDOW_HOURS, metaStyle } from "../../design/tiers";
import { suggestContinuation, type ContinuationOffer } from "./continuation";
import { resolveAnchor, type ParsedAnchor } from "./anchors";
import { MONTHS, clockLabel, dayLabel, firstLineOf } from "./format";
import { runEnrichmentPass } from "./enrichment";
import {
  flushSyncTombstoneOutbox,
  startDesktopFirestoreIngest,
  startLocalEntriesFirestoreUploadOnAuth,
} from "@/lib/desktopFirestoreSync";
import { isFirebaseSyncConfigured } from "@/lib/firebaseConfig";
import { deleteCloudAccount, isSignedInForSync } from "@/lib/desktopFirestoreSync";
import {
  HEARTBEAT_MS,
  isThisDeviceRevoked,
  registerThisDevice,
  removeDevice,
  subscribeDevices,
  type SyncDevice,
} from "@/lib/syncDevices";
import { parseTextWithUrls } from "@/lib/urlInText";
import {
  applyAppearance,
  applyTextScale,
  clampTextScale,
  readAppearance,
  readLiftContrast,
  readTextScale,
  writeAppearance,
  writeLiftContrast,
  writeTextScale,
  ZOOM_STEP,
  type Appearance,
} from "@/lib/appearance";
import { isOptIn, setOptIn } from "@/lib/analytics";
import { getStoredIconVariantId, setStoredIconVariantId } from "@/lib/iconVariants";
import { setDesktopIcon } from "@/lib/setDesktopIcon";
import { APP_VERSION } from "@/lib/appVersion";
import { useAppUpdater } from "@/lib/appUpdater";
import { IS_MAC_APP_STORE } from "@/lib/distribution";
import "../../design/tokens.css";
import { Verb } from "./Verb";

/**
 * How long a fragment stays "still yours to change" after it lands.
 *
 * Not an undo window and not a grace period before saving — the save already happened,
 * locally and instantly. It is the few seconds in which correcting is still part of
 * writing rather than a separate act of revision.
 */
const EDIT_WINDOW_SECONDS = 12;
const UNDO_WINDOW_SECONDS = 8;

/**
 * "today 09:12", "yesterday 22:04", "3 sep 08:40" — when the last backup was taken.
 *
 * "never" is a real answer on a first run and is said plainly rather than hidden: a backup
 * line that claims a time it does not have is worse than one that admits there is none.
 */
function backupLine(iso: string | null, now: Date): string {
  if (!iso) return "never";
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return "never";
  return `${dayLabel(at, now)} ${clockLabel(at)}`;
}

/**
 * The one sentence the edge says about the microphone, and the verb that answers it.
 *
 * The product cannot grant itself the microphone, so when the mac has refused the only
 * honest affordance is the door to where the answer lives.
 */
function micNotice(
  notice: "ask" | "denied" | "failed" | null,
  onOpenSettings: () => void,
): ReactNode {
  if (!notice) return null;
  if (notice === "denied") {
    return (
      <>
        chinotto can’t hear — the mac isn’t allowing the microphone. system settings ›
        privacy › microphone, then hold space again.{" "}
        <Verb onClick={onOpenSettings} style={{ cursor: "pointer" }}>
          open system settings ›
        </Verb>
      </>
    );
  }
  if (notice === "ask") {
    return <>the mac will ask once whether chinotto may hear you.</>;
  }
  return <>that recording didn’t start. nothing was lost; hold space again.</>;
}

/**
 * Settings' one-sentence summary of sync.
 *
 * It names what is actually known and nothing more. With no device list yet it says how
 * many devices there are rather than inventing their names, and offline says how much is
 * waiting rather than implying something is wrong — being offline is a fact, not an error.
 */
function settingsSyncLine(
  state: SyncState,
  offline: boolean,
  devices: SyncDevice[] | null,
  pending: number,
): string {
  if (state === "error") return "stopped · this mac’s sign-in expired.";
  if (state === "off") return "off · the record is only on this mac.";
  if (state === "connecting") return "connecting this mac…";
  if (offline) return `on · offline, ${pending} waiting.`;
  const others = devices?.filter((d) => !d.isThisDevice) ?? [];
  if (others.length === 1) return `on · this mac and ${others[0].name}.`;
  if (others.length > 1) return `on · this mac and ${others.length} other devices.`;
  return "on · only this mac so far.";
}

/** Where you are standing, and what was here when you left the edge. */
interface Anchored extends ParsedAnchor {
  arrivedAt: number;
  /** How much the record held when you left, so the way back can say what is waiting. */
  countOnLeaving: number;
}

export function RecordApp() {
  const now = useNow();
  const updater = useAppUpdater();

  const [fragments, setFragments] = useState<Fragment[]>([]);
  const [held, setHeld] = useState<HeldFragment[]>([]);
  const [materials, setMaterials] = useState<api.Materials>({
    encounters: new Map(),
    voices: new Map(),
  });
  const [returned, setReturned] = useState<api.ReturnValue | null>(null);
  const [retLeaving, setRetLeaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /**
   * Until the first read returns we do not know whether the Record is empty. Rendering the
   * empty state meanwhile would flash "type anything and press return" on every launch and
   * then replace it with the record — so the surface waits instead of guessing.
   */
  const [loaded, setLoaded] = useState(false);

  // ---- where you are ------------------------------------------------------------------
  /** What is in the field. `/` makes it find; a date phrase makes it a destination. */
  const [input, setInput] = useState("");
  /** Standing in a month, or at the edge. */
  const [anchor, setAnchor] = useState<Anchored | null>(null);
  /** Focused on a fragment or the line it belongs to. */
  const [focusId, setFocusId] = useState<string | null>(null);
  /**
   * Whether focus was reached by `continue` rather than by opening the fragment.
   *
   * It decides one thing — whether the continuation field takes the caret — and that one
   * thing is the whole difference between the two verbs: `continue` means you already know
   * what you want to add, `open` means you came to read.
   */
  const [focusContinuing, setFocusContinuing] = useState(false);
  /** Whether the keyboard is in the record rather than at the edge. */
  const [keyboardInRecord, setKeyboardInRecord] = useState(false);
  /** A utility surface, which replaces the column rather than covering it. */
  const [surface, setSurface] = useState<"settings" | "sync" | null>(null);

  // ---- what this mac looks like ---------------------------------------------------------
  const [appearance, setAppearance] = useState<Appearance>(() => readAppearance());
  const [liftContrast, setLiftContrast] = useState(() => readLiftContrast());
  const [textScale, setTextScale] = useState(() => readTextScale());
  const [iconVariant, setIconVariant] = useState<"dark" | "light">(() =>
    getStoredIconVariantId() === "light" ? "light" : "dark",
  );
  const [analyticsOn, setAnalyticsOn] = useState(() => isOptIn());
  const [exportNote, setExportNote] = useState("");
  const [backupAt, setBackupAt] = useState<string | null>(null);
  /**
   * What the mac has said about the microphone.
   *
   * It only ever moves away from "ask" because something actually happened: a recording
   * that worked, or one that was refused. The product cannot read the answer without
   * asking, so it does not claim to know it.
   */
  const [microphone, setMicrophone] = useState<MicrophoneState>("ask");

  // ---- sync ------------------------------------------------------------------------------
  const [devices, setDevices] = useState<SyncDevice[] | null>(null);
  const [conflicts, setConflicts] = useState<api.WordingConflict[]>([]);
  const [pendingToSync, setPendingToSync] = useState(0);
  /** Set by "already done on the phone?", which is the manual way past step one. */
  const [phoneReady, setPhoneReady] = useState(false);
  const [signingIn, setSigningIn] = useState(false);
  /** This mac was removed from the record by another device. */
  const [revoked, setRevoked] = useState(false);

  // The existing Apple/Firebase path, reused rather than rebuilt: this phase adds device
  // identity to sync, it does not redesign how signing in works.
  const appleAuth = useAppleSyncOAuth({ active: surface === "sync" || surface === "settings" });
  const signedIn = appleAuth.user != null || isSignedInForSync();
  const syncState: SyncState = signingIn || appleAuth.busy
    ? "connecting"
    : appleAuth.error
      ? "error"
      : revoked || !signedIn
        ? "off"
        : "on";

  // ---- what the record is doing --------------------------------------------------------
  const [justSaved, setJustSaved] = useState<{
    id: string;
    at: number;
    suggestion: ContinuationOffer | null;
  } | null>(null);
  const [editing, setEditing] = useState<{ id: string; text: string } | null>(null);
  const [meaningOn, setMeaningOn] = useState(false);
  const [guesses, setGuesses] = useState<api.Guess[]>([]);
  const [rejectedGuesses, setRejectedGuesses] = useState<Set<string>>(() => new Set());
  /** One short line, in the quiet line, that clears itself. Never a dialog. */
  const [notice, setNotice] = useState<string | null>(null);
  /** The last removal, so it can be brought back from the quiet line. */
  const [undo, setUndo] = useState<{ fragment: api.Fragment; at: number } | null>(null);
  const [online, setOnline] = useState(() =>
    typeof navigator === "undefined" ? true : navigator.onLine,
  );
  /** Ticks the edit window's countdown; the value is derived from `justSaved.at`. */
  const [, setTick] = useState(0);

  /**
   * Where the edge was when you left it.
   *
   * "esc returns you to the edge where you were" is a promise the product makes in
   * writing. Coming back to the top of the record would break it, and would lose your
   * place in a corpus that is meant to be traversed.
   */
  const edgeScroll = useRef(0);
  const retLeavingRef = useRef(false);

  const leaveEdge = useCallback((go: () => void) => {
    edgeScroll.current = document.scrollingElement?.scrollTop ?? 0;
    go();
  }, []);

  /**
   * Launch holds only for what is left of its window after the record has loaded, so a
   * slow first read is never made slower. `loaded` is the real signal, not a timer.
   */
  const launch = useLaunch(loaded);

  const isFind = input.startsWith("/");
  const query = isFind ? input.slice(1).trim().toLowerCase() : "";
  const atEdge = focusId === null;

  const reload = useCallback(async () => {
    try {
      const [all, holds, ret] = await Promise.all([
        // The whole record: standing in a month re-measures distance from there, so the
        // surface cannot work from a recent page.
        api.allFragments(),
        api.heldFragments(),
        api.selectReturn(),
      ]);
      setReturned((current) => (retLeavingRef.current ? current : ret));
      setFragments(all);
      setHeld(holds);
      // One pass for everything, rather than a query per row.
      setMaterials(
        await api.materialsFor([...all.map((f) => f.id), ...holds.map((h) => h.fragment.id)]),
      );
      setError(null);
      setLoaded(true);
      // The menu bar states `today · n` and wears its waiting modifier whether or not this
      // window is open, so it is told whenever the Record has been re-read.
      void api.refreshTray(isFirebaseSyncConfigured()).catch(() => {});
    } catch (e) {
      setLoaded(true);
      // Reading the Record can fail; capture cannot. Surfacing this quietly rather than
      // blocking the surface keeps that distinction true.
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  // Stable on purpose: a fresh arrow here gives `voice` a new identity on every render,
  // and the ⌥space effect below then tears down and re-registers two native listeners each
  // time — ten times a second while a recording's timer is running.
  const onVoiceCaptured = useCallback(() => void reload(), [reload]);
  const voice = useVoice(onVoiceCaptured);

  useEffect(() => {
    if (voice.notice === "denied") setMicrophone("denied");
  }, [voice.notice]);
  useEffect(() => {
    if (voice.recording) setMicrophone("granted");
  }, [voice.recording]);

  /**
   * Recordings that are on disk with nothing pointing at them.
   *
   * The voice pipeline's one claim is that the audio is the material and outlives whatever
   * happens to the words. It did — into a directory the Record could not see. Quitting the
   * app mid-recording, which is what somebody does when a recording will not stop, left the
   * file complete and no fragment anywhere.
   *
   * They are adopted through the same calls a live capture makes, carrying the time the
   * file says the recording ended rather than the time they were noticed: `captured_at` is
   * immutable, so it has exactly one chance to be right. Each one is a voice fragment with
   * its audio, and a transcript that says plainly it was never taken.
   *
   * Runs once, after the first read, and never blocks anything.
   */
  const adopted = useRef(false);
  useEffect(() => {
    if (!loaded || adopted.current) return;
    adopted.current = true;
    void (async () => {
      let found: api.OrphanedRecording[];
      try {
        found = await api.orphanedRecordings();
      } catch {
        return;
      }
      // Shorter than a hold: a slip of the hand, not a thought. The same threshold the
      // live path drops on, so being interrupted cannot make something the product would
      // not have kept anyway.
      const keep = found.filter((r) => r.durationMs >= DROP_UNDER_MS);
      if (keep.length === 0) return;
      for (const r of keep) {
        try {
          const f = await api.captureVoice(r.audioPath, r.durationMs, "desktop", r.endedAt);
          await api.recordTranscript(
            f.id,
            null,
            "found on disk after the app stopped · no words were ever taken from it",
          );
        } catch {
          // One that cannot be adopted stays on disk and is offered again next launch.
        }
      }
      await reload();
      setNotice(
        keep.length === 1
          ? "a recording was found on disk and put back in the record"
          : `${keep.length} recordings were found on disk and put back in the record`,
      );
    })();
  }, [loaded, reload]);

  /**
   * Recordings still waiting for words are read back on this Mac when it can recognise
   * locally (`retry_transcripts`), which asks nothing of the person and backs off by itself.
   * It is offered after the first read and whenever the window comes back to the front.
   */
  useEffect(() => {
    if (!loaded) return;
    const retry = () =>
      void api
        .retryTranscripts()
        .then((r) => {
          if (r.transcribed > 0) void reload();
        })
        .catch(() => {});
    const first = setTimeout(retry, 4000);
    window.addEventListener("focus", retry);
    return () => {
      clearTimeout(first);
      window.removeEventListener("focus", retry);
    };
  }, [loaded, reload]);

  const changeTextScale = useCallback((delta: number) => {
    setTextScale((current) => writeTextScale(clampTextScale(current + delta)));
  }, []);

  // The window's own appearance, applied before anything is read from it.
  useEffect(() => {
    applyAppearance(appearance, liftContrast);
  }, [appearance, liftContrast]);

  useEffect(() => {
    applyTextScale(textScale);
  }, [textScale]);

  useEffect(() => {
    void api.lastBackupAt().then(setBackupAt).catch(() => setBackupAt(null));
  }, []);

  /**
   * This mac says it is here, and keeps saying so.
   *
   * Without a device row the sync surface has nothing truthful to list and `remove` has
   * nothing to revoke — so registering is not a nicety, it is what makes the surface able
   * to tell the truth at all.
   */
  useEffect(() => {
    if (!signedIn) return;
    let stopped = false;
    const beat = () => {
      void registerThisDevice().catch(() => {});
      void isThisDeviceRevoked()
        .then((r) => {
          if (!stopped) setRevoked(r);
        })
        .catch(() => {});
    };
    beat();
    const id = setInterval(beat, HEARTBEAT_MS);
    const stopDevices = subscribeDevices((d) => {
      if (!stopped) setDevices(d);
    });
    return () => {
      stopped = true;
      clearInterval(id);
      stopDevices();
    };
  }, [signedIn]);

  /** What the bridge still owes the other devices, and what was worded twice. */
  const refreshSyncFacts = useCallback(async () => {
    try {
      setConflicts(await api.openWordingConflicts());
    } catch {
      setConflicts([]);
    }
    try {
      setPendingToSync(await api.fragmentsAwaitingMirror());
    } catch {
      setPendingToSync(0);
    }
  }, []);

  useEffect(() => {
    void refreshSyncFacts();
    const id = setInterval(() => void refreshSyncFacts(), 30_000);
    return () => clearInterval(id);
  }, [refreshSyncFacts]);

  const bringBack = useCallback(async () => {
    const u = undo;
    if (!u) return;
    await api.restoreFragment(u.fragment.id);
    setUndo(null);
    await reload();
  }, [undo, reload]);

  // Before paint, so returning never flashes at the top and then jumps.
  useLayoutEffect(() => {
    if (!atEdge) return;
    const el = document.scrollingElement;
    if (el) el.scrollTop = edgeScroll.current;
  }, [atEdge]);

  /**
   * Enrichment and embedding both run after the fact, on a timer, and neither is allowed to
   * be in the way of anything. A pass that fails is simply a pass that did nothing.
   */
  useEffect(() => {
    let stopped = false;
    async function pass() {
      if (stopped) return;
      try {
        const enriched = await runEnrichmentPass(5);
        await api.embedPending(24);
        if (enriched > 0 && !stopped) await reload();
      } catch {
        // Background work never surfaces an error; the Record is readable either way.
      }
    }
    void pass();
    const id = setInterval(() => void pass(), 60_000);
    return () => {
      stopped = true;
      clearInterval(id);
    };
  }, [reload]);

  /**
   * `esc`, once, globally — so it always steps back by exactly one level.
   *
   * The order is the order the levels were entered in. Handling it per surface is how you
   * end up with an `esc` that drops a draft from inside a month, or closes two things at
   * once because both were listening.
   */
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const mod = e.metaKey || e.ctrlKey;
      if (mod && !e.shiftKey && (e.key === "z" || e.key === "Z")) {
        if (!undo) return;
        e.preventDefault();
        void bringBack();
        return;
      }
      if (mod && e.key === ",") {
        e.preventDefault();
        // A toggle, not a push: pressing it again from settings puts you back at the edge.
        setSurface((s) => (s ? null : "settings"));
        setFocusId(null);
        return;
      }
      if (mod && (e.key === "=" || e.key === "+")) {
        e.preventDefault();
        return changeTextScale(ZOOM_STEP);
      }
      if (mod && e.key === "-") {
        e.preventDefault();
        return changeTextScale(-ZOOM_STEP);
      }
      if (e.key !== "Escape") return;
      // A recording is the innermost thing you can be in the middle of, which is why the
      // design's ladder puts it first: tray → recording → correcting → … It keeps nothing,
      // exactly as a hold released inside `DROP_UNDER_MS` already kept nothing — the field
      // has been offering `esc to drop` since the speaking surface was drawn, and this is
      // the half that was missing.
      if (voice.recording) return voice.drop();
      if (editing) return setEditing(null);
      // A utility surface is deeper than focus: sync steps back to settings, settings to
      // the edge, and only then does the edge's own ladder start.
      if (surface === "sync") return setSurface("settings");
      if (surface) return setSurface(null);
      if (focusId) {
        setFocusContinuing(false);
        return setFocusId(null);
      }
      if (input) return setInput("");
      if (anchor) return setAnchor(null);
      if (keyboardInRecord) return setKeyboardInRecord(false);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [
    editing,
    focusId,
    input,
    anchor,
    keyboardInRecord,
    undo,
    bringBack,
    surface,
    changeTextScale,
    voice.recording,
    voice.drop,
  ]);

  /**
   * `⌥space` anywhere on the mac. The window is brought forward by the Rust side before
   * this fires, so the recording is always visible while it is happening.
   */
  useEffect(() => {
    const started = listen("chinotto-voice-hold-start", () => voice.start());
    const stopped = listen("chinotto-voice-hold-stop", () => voice.stop());
    return () => {
      void started.then((f) => f());
      void stopped.then((f) => f());
    };
    // The two verbs, not the whole hook: `seconds` ticks while recording, and re-registering
    // the shortcut on every tick both costs a round trip and can drop a release.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [voice.start, voice.stop]);

  /**
   * Something was captured from the menu bar while this window was elsewhere.
   *
   * The panel writes straight into the Record, so this is only about the window catching
   * up — it is not part of the save, and a listener that never fires cannot lose anything.
   */
  useEffect(() => {
    const unlisten = listen("chinotto-tray-entry-saved", () => {
      void reload();
    });
    return () => {
      void unlisten.then((f) => f());
    };
  }, [reload]);

  /**
   * `settings` in the menu-bar menu. The same surface `⌘,` opens, reached from the one
   * place the product has that is not a window.
   */
  useEffect(() => {
    const unlisten = listen("chinotto-open-settings", () => {
      setSurface("settings");
      setFocusId(null);
    });
    return () => {
      void unlisten.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (!notice) return;
    const id = setTimeout(() => setNotice(null), 4000);
    return () => clearTimeout(id);
  }, [notice]);

  useEffect(() => {
    if (!undo) return;
    const id = setInterval(() => {
      if (Date.now() - undo.at > UNDO_WINDOW_SECONDS * 1000) {
        setUndo(null);
        void flushSyncTombstoneOutbox();
      } else {
        setTick((t) => t + 1);
      }
    }, 1000);
    return () => clearInterval(id);
  }, [undo]);

  useEffect(() => {
    const up = () => setOnline(true);
    const down = () => setOnline(false);
    window.addEventListener("online", up);
    window.addEventListener("offline", down);
    return () => {
      window.removeEventListener("online", up);
      window.removeEventListener("offline", down);
    };
  }, []);

  /**
   * Sync, carried across from the surface this replaced.
   *
   * The Record is canonical here, but the phone and the sync protocol still speak legacy
   * `entries`, so the bridge runs at both ends of every exchange: anything that arrives is
   * projected into the Record, and anything the Record wrote that has not reached the
   * legacy table yet is mirrored out. Sync's own UI is pending, but the capability must
   * not disappear with it.
   */
  useEffect(() => {
    if (!isFirebaseSyncConfigured()) return;

    let stopped = false;
    const stopIngest = startDesktopFirestoreIngest(() => {
      // Something arrived from another device; make it part of the Record.
      void (async () => {
        try {
          await api.projectEntriesIntoRecord();
          if (!stopped) await reload();
        } catch (e) {
          console.warn("[bridge] projecting after ingest failed", e);
        }
      })();
    });
    const stopUpload = startLocalEntriesFirestoreUploadOnAuth();

    // Catch up on anything captured while this was not running, e.g. across the upgrade.
    void (async () => {
      try {
        await api.mirrorPendingFragments(500);
        await api.projectEntriesIntoRecord();
        await flushSyncTombstoneOutbox();
        if (!stopped) await reload();
      } catch (e) {
        console.warn("[bridge] catch-up failed", e);
      }
    })();

    return () => {
      stopped = true;
      stopIngest();
      stopUpload();
    };
  }, [reload]);

  // ---- leaving a fragment ---------------------------------------------------------------

  const handleLeave = useCallback(async (body: string) => {
    // A link typed or pasted into capture is an encounter, not a string that happens to
    // look like one — so it gets a source row, a durable url_key and the chance of a
    // title later. The URL is stored exactly as written; nothing is canonicalised here.
    const urls = parseTextWithUrls(body).segments.filter((s) => s.type === "url");
    const left =
      urls.length === 1
        ? await api.captureEncounter(
            urls[0].value,
            // Only the person's own words; if the fragment is just the link, there are none.
            body.trim() === urls[0].value.trim() ? "" : body,
            // No source app: nothing handed us one. Typing is not a share.
            null,
            null,
          )
        : await api.captureFragment(body, "typed", "desktop");

    setFragments((prev) => {
      setJustSaved({
        id: left.id,
        at: Date.now(),
        // Looking back over material already in memory: an offer must never be the reason
        // a capture takes a millisecond longer than it has to.
        suggestion: suggestContinuation(left, prev, new Date()),
      });
      return [left, ...prev];
    });
  }, []);

  /**
   * The edit window, counting itself down and then clearing itself.
   *
   * One interval for both the countdown and the expiry, so the number on screen and the
   * moment the offer disappears can never disagree.
   */
  useEffect(() => {
    if (!justSaved) return;
    const id = setInterval(() => {
      if ((Date.now() - justSaved.at) / 1000 > EDIT_WINDOW_SECONDS) setJustSaved(null);
      else setTick((t) => t + 1);
    }, 1000);
    return () => clearInterval(id);
  }, [justSaved]);

  const justSavedView = justSaved
    ? {
        id: justSaved.id,
        secondsLeft: Math.max(
          0,
          EDIT_WINDOW_SECONDS - Math.floor((Date.now() - justSaved.at) / 1000),
        ),
        // `settle` runs once, on arrival — not on every tick of the countdown.
        landing: Date.now() - justSaved.at < 1200,
        suggestion: justSaved.suggestion,
      }
    : null;

  const acceptSuggestion = useCallback(() => {
    const s = justSaved;
    if (!s?.suggestion) return;
    setJustSaved({ ...s, suggestion: null });
    void api
      .linkContinuation(s.id, s.suggestion.id)
      .then(() => reload())
      // Declining to link is recoverable; saying it failed is not worth a surface.
      .catch(() => {});
  }, [justSaved, reload]);

  // ---- acting on a fragment -------------------------------------------------------------

  /**
   * Keeping present is bounded. When the bound refuses, the surface says so once, quietly,
   * and asks for a release — it does not drop something the person chose to keep.
   */
  const handleHold = useCallback(async (f: Fragment) => {
    const [, taken] = await api.holdFragment(f.id);
    if (!taken) {
      setNotice(`${api.MAX_HELD} things are already held · release one to keep this`);
      return;
    }
    setNotice(null);
    setHeld(await api.heldFragments());
  }, []);

  const handleRelease = useCallback(async (f: Fragment) => {
    await api.releaseFragment(f.id);
    setNotice(null);
    setHeld(await api.heldFragments());
  }, []);

  const handleOpen = useCallback(
    (f: Fragment) =>
      leaveEdge(() => {
        setFocusContinuing(false);
        setFocusId(f.id);
      }),
    [leaveEdge],
  );

  const handleContinue = useCallback(
    (f: Fragment) =>
      leaveEdge(() => {
        setFocusContinuing(true);
        setFocusId(f.id);
      }),
    [leaveEdge],
  );

  const startCorrecting = useCallback((f: Fragment) => {
    setEditing({ id: f.id, text: f.body });
  }, []);

  const saveCorrection = useCallback(async () => {
    const e = editing;
    if (!e) return;
    const body = e.text.trim();
    setEditing(null);
    if (!body) return;
    const updated = await api.correctFragment(e.id, body);
    if (updated) setFragments((prev) => prev.map((f) => (f.id === e.id ? updated : f)));
  }, [editing]);

  const handleRemove = useCallback(
    async (f: Fragment) => {
      await api.removeFragment(f.id);
      setUndo({ fragment: f, at: Date.now() });
      setNotice(null);
      if (focusId === f.id) {
        setFocusId(null);
        setFocusContinuing(false);
      }
      await reload();
    },
    [reload, focusId],
  );

  const copyText = useCallback(async (text: string) => {
    try {
      await navigator.clipboard?.writeText(text);
    } catch {
      // A clipboard that refuses is not worth a surface; the words are still on screen.
    }
  }, []);

  // ---- standing ---------------------------------------------------------------------------

  const stand = useCallback(
    (parsed: ParsedAnchor | null) => {
      if (!parsed) {
        setAnchor(null);
        return;
      }
      const resolved = resolveAnchor(
        parsed,
        fragments.map((f) => f.capturedAt),
      );
      setAnchor({ ...resolved, arrivedAt: Date.now(), countOnLeaving: fragments.length });
      setFocusId(null);
      window.scrollTo({ top: 0 });
    },
    [fragments],
  );

  // ---- find --------------------------------------------------------------------------------

  const wordHits = useMemo(() => {
    if (!query) return 0;
    return fragments.filter((f) => f.body.toLowerCase().includes(query)).length;
  }, [fragments, query]);

  useEffect(() => {
    if (!meaningOn || !query || wordHits > 0) {
      setGuesses([]);
      return;
    }
    let stale = false;
    void api
      .findByMeaning(query, [], 3)
      .then((g) => {
        if (!stale) setGuesses(g.filter((x) => !rejectedGuesses.has(x.fragment.id)));
      })
      .catch(() => setGuesses([]));
    return () => {
      stale = true;
    };
  }, [meaningOn, query, wordHits, rejectedGuesses]);

  // ---- the line a fragment belongs to, for its meta line -------------------------------------

  const [lineMeta, setLineMeta] = useState<Map<string, string>>(() => new Map());
  useEffect(() => {
    /*
      Only D0 draws this line, so only D0 is asked about.

      Both halves of that matter. Asking for the newest forty regardless of distance meant a
      record whose newest fragment is weeks old — which is every record that has been left
      alone for a while — spent forty round trips on rows that will never show the answer.
      And asking for them one after another made it forty round trips in series, on every
      single reload. The cutoff is read from the clock once rather than from `now`, so this
      does not re-run on the minute tick.
    */
    const edge = Date.now() - D0_WINDOW_HOURS * 60 * 60 * 1000;
    const recent = fragments
      .filter((f) => new Date(f.capturedAt).getTime() >= edge)
      .slice(0, 40);
    if (recent.length === 0) {
      setLineMeta((m) => (m.size === 0 ? m : new Map()));
      return;
    }
    let stale = false;
    void (async () => {
      const lines = await Promise.all(
        recent.map((f) =>
          // A line that cannot be read is simply not annotated.
          api.lineFor(f.id).catch(() => []),
        ),
      );
      const out = new Map<string, string>();
      recent.forEach((f, n) => {
        const line = lines[n];
        if (line.length < 2) return;
        const i = line.findIndex((m) => m.fragment.id === f.id);
        if (i < 0) return;
        const first = new Date(line[0].fragment.capturedAt);
        out.set(
          f.id,
          i === 0
            ? `↳ first moment of a line · continued ${line.length - 1} times`
            : `↳ moment ${i + 1} of a line · since ${MONTHS[first.getMonth()]} ${first.getFullYear()}`,
        );
      });
      if (!stale) setLineMeta(out);
    })();
    return () => {
      stale = true;
    };
  }, [fragments]);

  // ---- the column -----------------------------------------------------------------------------

  const sinceYouLeft = anchor ? fragments.length - anchor.countOnLeaving : 0;
  const drafting = Boolean(input) && !isFind;

  return (
    <div
      style={{
        minHeight: "100vh",
        background: "var(--surface)",
        color: "var(--ink)",
        fontFamily: "'Archivo Variable', system-ui, sans-serif",
        fontVariationSettings: "'wdth' 100",
        WebkitFontSmoothing: "antialiased",
        boxSizing: "border-box",
        paddingTop: "var(--window-pad-top)",
        paddingRight: "var(--window-pad-right)",
        paddingBottom: "var(--window-pad-bottom)",
        paddingLeft: "var(--window-pad-left)",
      }}
    >
      <div
        style={{
          maxWidth: "var(--column-width)",
          display: "flex",
          flexDirection: "column",
          // The record rises into place underneath the lockup as it goes, rather than
          // being revealed by it. One gesture, not two.
          animation: launch === "mark" ? "none" : "chinotto-rise 0.6s ease-out both",
        }}
      >
        {surface === "sync" ? (
          <Sync
            state={syncState}
            offline={!online}
            pending={pendingToSync}
            devices={devices}
            conflicts={conflicts}
            now={now}
            phoneReady={phoneReady}
            qrUrl="https://getchinotto.app/sync"
            onAlreadyDone={() => setPhoneReady(true)}
            onContinueWithApple={() => {
              setSigningIn(true);
              void appleAuth
                .onContinueApple()
                .then(() => registerThisDevice())
                .catch(() => {})
                .finally(() => setSigningIn(false));
            }}
            onRemoveDevice={(id) => {
              void removeDevice(id).catch(() => {
                setNotice("that device could not be removed · try again when online");
              });
            }}
            onStopSyncing={() => {
              void Promise.resolve(appleAuth.onSignOut())
                .then(() => {
                  setDevices(null);
                  setSurface("settings");
                })
                .catch(() => {});
            }}
            onResolveConflict={(fragmentId, shows) => {
              void api
                .resolveWordingConflict(fragmentId, shows)
                .then(() => refreshSyncFacts())
                .then(() => reload())
                .catch(() => {});
            }}
            onOpenFragment={(fragmentId) => {
              setSurface(null);
              setFocusId(fragmentId);
            }}
            expiredWhen={appleAuth.error ? "recently" : "recently"}
            onLeave={() => setSurface("settings")}
          />
        ) : surface === "settings" ? (
          <Settings
            version={APP_VERSION}
            syncLine={settingsSyncLine(syncState, !online, devices, pendingToSync)}
            syncVerb={syncState === "off" ? "set up" : syncState === "error" ? "fix" : "manage"}
            onOpenSync={() => setSurface("sync")}
            hasAccount={isSignedInForSync()}
            microphone={microphone}
            onOpenSystemSettings={() => {
              void api.openMicrophoneSettings().catch(() => {});
            }}
            onTryMenuBar={() => {
              void api.openTrayCapture().catch(() => {});
            }}
            appearance={appearance}
            onAppearance={(a) => {
              writeAppearance(a);
              setAppearance(a);
            }}
            liftContrast={liftContrast}
            onLiftContrast={(on) => {
              writeLiftContrast(on);
              setLiftContrast(on);
            }}
            textScale={textScale}
            onTextScale={(p) => setTextScale(writeTextScale(clampTextScale(p)))}
            iconVariant={iconVariant}
            onIconVariant={(v) => {
              setStoredIconVariantId(v);
              setIconVariant(v);
              void setDesktopIcon(v).catch(() => {});
            }}
            onExport={() => {
              void api
                .exportRecord()
                .then((name) => {
                  if (!name) return;
                  setExportNote(
                    IS_MAC_APP_STORE ? `saved · ${name}` : `saved to downloads · ${name}`,
                  );
                  setTimeout(() => setExportNote(""), 4000);
                })
                .catch(() => {
                  setExportNote("the export could not be written");
                  setTimeout(() => setExportNote(""), 4000);
                });
            }}
            exportNote={exportNote}
            backupLine={backupLine(backupAt, now)}
            onBackUpNow={() => {
              void api
                .createBackup()
                .then(() => api.lastBackupAt())
                .then(setBackupAt)
                .catch(() => {});
            }}
            analyticsOn={analyticsOn}
            onAnalytics={(on) => {
              setOptIn(on);
              setAnalyticsOn(on);
            }}
            updateLine={
              IS_MAC_APP_STORE
                ? "updates arrive through the Mac App Store"
                : updater.phase === "available" && updater.version
                ? `${updater.version} is out.`
                : updater.phase === "downloading"
                  ? "downloading…"
                  : updater.phase === "ready"
                    ? "downloaded."
                    : "up to date · checked at launch"
            }
            updateVerb={
              IS_MAC_APP_STORE
                ? null
                : updater.phase === "available"
                ? "download"
                : updater.phase === "ready"
                  ? "restart"
                  : null
            }
            onUpdate={() => {
              if (updater.phase === "available") void updater.download();
              else if (updater.phase === "ready") void updater.installAndRestart();
            }}
            onDeleteAccount={async () => {
              await deleteCloudAccount();
            }}
            onLeave={() => setSurface(null)}
          />
        ) : atEdge ? (
          <>
            {anchor ? (
              /*
                Standing replaces the field rather than sitting above it. There is nothing
                to capture into from inside a past month — a fragment left there would have
                to claim a date it does not have.
              */
              <div
                style={{
                  ...metaStyle(),
                  display: "flex",
                  justifyContent: "space-between",
                  marginBottom: "22px",
                }}
              >
                <Verb
                  quiet
                  onClick={() => setAnchor(null)}
                  style={{ cursor: "pointer", whiteSpace: "nowrap" }}
                >
                  ▲ today · {sinceYouLeft > 0 ? `${sinceYouLeft} new since you left` : "the edge"}{" "}
                  · esc
                </Verb>
                <span>
                  you are in {MONTHS[anchor.month]} {anchor.year}
                </span>
              </div>
            ) : (
              <div onFocusCapture={() => setKeyboardInRecord(false)}>
                <Capture
                  value={input}
                  onChange={setInput}
                  onLeave={handleLeave}
                  onStand={stand}
                  onEnterRecord={() => setKeyboardInRecord(true)}
                  findCount={wordHits}
                  meaningOn={meaningOn}
                  onToggleMeaning={() => setMeaningOn((m) => !m)}
                  speaking={voice.recording}
                  speakingSeconds={voice.seconds}
                  onStartSpeaking={voice.start}
                  notice={micNotice(voice.notice, () => {
                    voice.dismissNotice();
                    void api.openMicrophoneSettings().catch(() => {});
                  })}
                />
              </div>
            )}

            {/*
              Nothing matched the words. Guesses say they are guesses — italic, and labelled
              — and a dismissal is remembered, because being told twice is worse than not
              being told at all.
            */}
            {isFind && query && wordHits === 0 ? (
              <>
                <div
                  style={{
                    marginTop: "26px",
                    fontSize: "var(--size-d1)",
                    color: "var(--ink-far)",
                    fontVariationSettings: "'wdth' 94",
                    lineHeight: 1.4,
                  }}
                >
                  {meaningOn
                    ? "nothing with those words. close in meaning, maybe:"
                    : "nothing with those words."}
                </div>
                {guesses.length > 0 ? (
                  <div
                    style={{
                      marginTop: "28px",
                      display: "flex",
                      flexDirection: "column",
                      gap: "22px",
                      fontSize: "var(--size-moment)",
                      lineHeight: 1.28,
                      fontVariationSettings: "'wdth' 94",
                      color: "var(--ink-near)",
                      fontStyle: "italic",
                    }}
                  >
                    {guesses.map((g) => (
                      <div key={g.fragment.id} style={{ display: "flex", gap: "22px" }}>
                        <span
                          style={{
                            ...metaStyle(),
                            width: "68px",
                            flex: "none",
                            paddingTop: "6px",
                            fontStyle: "normal",
                          }}
                        >
                          {new Date(g.fragment.capturedAt).getDate()}{" "}
                          {MONTHS[new Date(g.fragment.capturedAt).getMonth()]}
                        </span>
                        <div
                          onClick={() => handleOpen(g.fragment)}
                          style={{ flex: 1, cursor: "pointer" }}
                        >
                          {g.fragment.body}
                        </div>
                        <Verb
                          quiet
                          onClick={() =>
                            setRejectedGuesses((s) => new Set(s).add(g.fragment.id))
                          }
                          style={{
                            ...metaStyle(),
                            fontStyle: "normal",
                            paddingTop: "6px",
                            cursor: "pointer",
                          }}
                        >
                          not this
                        </Verb>
                      </div>
                    ))}
                  </div>
                ) : null}
              </>
            ) : null}

            {loaded ? (
              <Record
                fragments={fragments}
                held={held}
                now={now}
                materials={materials}
                anchor={anchor}
                query={query || null}
                loaded={loaded}
                keyboardActive={keyboardInRecord}
                onLeaveKeyboard={() => setKeyboardInRecord(false)}
                onOpen={handleOpen}
                onContinue={handleContinue}
                onHold={handleHold}
                onRelease={handleRelease}
                onStandIn={(year, month) => stand({ year, month })}
                onCorrect={startCorrecting}
                onRemove={handleRemove}
                onCopy={(f) => void copyText(f.body)}
                onCopyLink={(f) => void copyText(`chinotto://fragment/${f.id}`)}
                justSaved={justSavedView}
                onAcceptSuggestion={acceptSuggestion}
                onRejectSuggestion={() => setJustSaved((s) => (s ? { ...s, suggestion: null } : s))}
                editingId={editing?.id ?? null}
                editText={editing?.text ?? ""}
                onEditChange={(v) => setEditing((e) => (e ? { ...e, text: v } : e))}
                onEditSave={saveCorrection}
                onEditCancel={() => setEditing(null)}
                lineMeta={lineMeta}
                returnSlot={
                  /*
                    A return steps aside while something is being written — it must never
                    compete with the thing the person came here to do. It collapses rather
                    than unmounting, because letting go of a return does not remove
                    anything and the record should not snap upward as if it had.
                  */
                  <div
                    style={{
                      display: "grid",
                      gridTemplateRows: drafting ? "0fr" : "1fr",
                      opacity: drafting ? 0 : 1,
                      transition:
                        "grid-template-rows var(--let-go) var(--ease), opacity var(--let-go) var(--ease)",
                    }}
                  >
                    <div style={{ overflow: "hidden", minHeight: 0 }}>
                      {returned && !anchor && !query ? (
                        <ReturnBlock
                          value={returned}
                          domain={(() => {
                            const enc = materials.encounters.get(returned.fragment.id);
                            if (!enc) return null;
                            return enc.title ? `${enc.domain ?? "a link"} · ${enc.title}` : enc.domain;
                          })()}
                          leaving={retLeaving}
                          onOpen={(f) => {
                            void api.recordReturnOutcome(returned.id, "opened");
                            handleOpen(f);
                          }}
                          onContinue={(f) => {
                            void api.recordReturnOutcome(returned.id, "continued");
                            handleContinue(f);
                          }}
                          onHold={(f) => {
                            void api.recordReturnOutcome(returned.id, "opened");
                            void handleHold(f);
                          }}
                          onLetGo={(v) => {
                            retLeavingRef.current = true;
                            setRetLeaving(true);
                            window.setTimeout(() => {
                              void api.recordReturnOutcome(v.id, "let_go");
                              setReturned(null);
                              setRetLeaving(false);
                              retLeavingRef.current = false;
                            }, 450);
                          }}
                        />
                      ) : null}
                    </div>
                  </div>
                }
              />
            ) : null}
          </>
        ) : (
          <FragmentFocus
            id={focusId}
            corpus={fragments}
            materials={materials}
            startContinuing={focusContinuing}
            onLeave={() => {
              setFocusId(null);
              setFocusContinuing(false);
              void reload();
            }}
            onChanged={() => void reload()}
            onOpen={handleOpen}
            onHold={handleHold}
            onRelease={handleRelease}
            onRemove={handleRemove}
            heldIds={new Set(held.map((h) => h.fragment.id))}
          />
        )}
      </div>

      {/*
        Outside the column on purpose.

        The column carries `chinotto-rise`, and an element with an animation on `transform`
        is a containing block for anything `position: fixed` inside it — so in here the quiet
        line stopped being fixed to the window and pinned itself to the bottom of the record
        instead, landing on top of the last rows. Nothing about its own styling said so.
      */}
      {launch === "done" ? (
      <QuietLine
        undo={
          undo
            ? {
                text:
                  firstLineOf(undo.fragment.body).slice(0, 48) +
                  (firstLineOf(undo.fragment.body).length > 48 ? "…" : ""),
                secondsLeft: Math.max(
                  0,
                  UNDO_WINDOW_SECONDS - Math.floor((Date.now() - undo.at) / 1000),
                ),
                onBringBack: () => void bringBack(),
              }
            : null
        }
        notice={notice}
        offline={!online}
        syncOn={isFirebaseSyncConfigured()}
        update={
          updater.phase === "available" && updater.version
            ? {
                text: `chinotto ${updater.version} is out · download`,
                onClick: () => void updater.download(),
              }
            : updater.phase === "downloading" && updater.version
              ? { text: `downloading ${updater.version}…`, onClick: () => {} }
              : updater.phase === "ready"
                ? { text: "update ready · restart to finish", onClick: () => void updater.installAndRestart() }
                : null
        }
        surfaceOpen={surface !== null}
        onSettings={() => setSurface("settings")}
      />
      ) : null}

      {error ? (
        <div style={{ position: "fixed", right: "48px", bottom: "36px", ...metaStyle() }}>
          {error}
        </div>
      ) : null}

      <Launch phase={launch} visible={loaded} />
    </div>
  );
}
