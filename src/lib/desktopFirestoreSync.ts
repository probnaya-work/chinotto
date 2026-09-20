import { getApp, getApps, initializeApp, type FirebaseApp } from "firebase/app";
import {
  getAuth,
  linkWithCredential,
  onAuthStateChanged,
  OAuthProvider,
  signInWithCredential,
  signOut,
  type User,
} from "firebase/auth";
import {
  collection,
  deleteDoc,
  deleteField,
  doc,
  enableNetwork,
  getDocFromServer,
  getDocs,
  initializeFirestore,
  limit,
  memoryLocalCache,
  onSnapshot,
  orderBy,
  query,
  serverTimestamp,
  setDoc,
  startAfter,
  Timestamp,
  where,
  type CollectionReference,
  type DocumentData,
  type DocumentSnapshot,
  type Firestore,
  type Query,
  type QueryDocumentSnapshot,
} from "firebase/firestore";
import {
  clearFirestoreIngestSuppression,
  clearSyncTombstoneOutboxAll,
  deleteLocalEntriesForSync,
  enqueueSyncTombstone,
  getEntryTheme,
  ingestFirestoreEntries,
  listEntries,
  listSyncTombstoneOutbox,
  listDueSyncTombstoneOutbox,
  removeSyncTombstoneOutbox,
  type EntryTheme,
} from "@/features/entries/entryApi";
import type { Entry } from "@/types/entry";
import { getFirebaseWebOptions, isFirebaseSyncConfigured } from "./firebaseConfig";
import { isFirestoreDocumentTombstoned } from "./firestoreTombstone";
import {
  parseEntryThemeField,
  partitionUserThemeDocs,
  toFirestoreEntryThemeWire,
  USER_THEME_QUERY_LIMIT,
} from "./firestoreSyncTheme";
import {
  applyRemoteEntryTheme,
  applyRemoteUserThemeTombstones,
  clearSyncUserThemeOutboxAll,
  ingestRemoteUserThemes,
} from "./themeSyncApi";
import { backfillLocalThemesToRemote } from "./themeSyncBackfill";
import { flushSyncUserThemeOutbox } from "./userThemeFlush";
import { isThemesEnabled } from "./themeSettings";

/** Shape of `OAuthCredential.toJSON()` from the OAuth bridge webview (Apple redirect). */
export type BridgedOAuthCredentialJson = {
  providerId: string;
  signInMethod: string;
  idToken?: string;
  accessToken?: string;
  secret?: string;
  nonce?: string;
  pendingToken?: string | null;
};

const INGEST_PAGE_SIZE = 500;
/** Matches mobile backfill cap (~20k docs). */
const INGEST_BACKFILL_MAX_PAGES = 40;
/** One-shot upload of pre-sync local rows to Firestore (per Firebase uid). */
const LOCAL_FIRESTORE_UPLOAD_UID_KEY = "chinotto_local_entries_firestore_upload_v1_uid";
const LOCAL_UPLOAD_BATCH_SIZE = 25;
let localUploadInFlight: Promise<void> | null = null;
/** Tombstones are queried separately so deletes on older entries (outside the recent ingest window) still apply locally. */
const TOMBSTONE_QUERY_LIMIT = 1000;
/**
 * Desktop sync modal **gate** (`sync_desktop_sessions/{ds}`) only: poll interval.
 * Fewer watch streams on that path mitigates Firestore INTERNAL ASSERTION b815/ca9 in embedded WebKit.
 */
const SYNC_MODAL_GATE_POLL_MS = 2500;
/** Re-read `users/{uid}` while waiting for mobile to set `active: true` (stuck snapshot workaround). */
const SYNC_ACCESS_WAITING_POLL_MS = 800;
/** Extra server reads after subscribe (ms) — mobile may write `active` right after the first read. */
const SYNC_ACCESS_WAITING_STAGGER_MS = [250, 550, 1100] as const;
/** After `active` is true, still re-read occasionally so **turning sync off on mobile** reaches desktop if `onSnapshot` stalls. */
const SYNC_ACCESS_WHILE_ACTIVE_POLL_MS = 30000;
/** After focus/visibility: short burst of server reads while still waiting for `active: true` (timers are throttled in background WebViews). */
const SYNC_ACCESS_FOCUS_BURST_MS = 1000;
const SYNC_ACCESS_FOCUS_BURST_MAX = 15;
/** After we have shown sync active, delay reporting inactive so focus/reattach glitches do not flip the header or modal. */
const SYNC_ACCESS_EMIT_FALSE_DEBOUNCE_MS = 450;

const FIRESTORE_RULES_SYNC_MODAL_HINT =
  "Firestore Security Rules must allow: (1) anyone may read sync_desktop_sessions/{sessionId}; " +
  "(2) signed-in users may read/write users/{userId} when request.auth.uid == userId. " +
  "Paste rules from chinotto-mobile docs/internal/sync/sync.md (Security Rules) and Publish in Firebase Console.";

function isFirestorePermissionDenied(e: unknown): boolean {
  if (e && typeof e === "object" && "code" in e) {
    const c = String((e as { code: string }).code);
    return c === "permission-denied";
  }
  return false;
}

/** Firestore rejected the request as this client (e.g. after Firebase Auth user was deleted elsewhere). */
export function isFirestoreSessionAccessLostError(e: unknown): boolean {
  if (e && typeof e === "object" && "code" in e) {
    const c = String((e as { code: string }).code);
    return c === "permission-denied" || c === "unauthenticated";
  }
  return false;
}

/** Mobile-written field on `users/{uid}`; keep in sync with chinotto-mobile `firestoreSyncAccessMirror`. */
export function isChinottoSyncAccessActiveInUserDoc(data: DocumentData | undefined): boolean {
  return data?.chinottoSyncAccess?.active === true;
}

/**
 * Enable sync modal / header: **only** `users/{uid}.chinottoSyncAccess.active === true` (mobile mirror).
 */

/**
 * Doc ids from the latest tombstone-query snapshot. Firestore already filtered `deletedAt != null`;
 * we trust the query result even when `instanceof Timestamp` / tombstone heuristics fail (duplicate SDK
 * bundles, plain `{ seconds }` shapes). Also used to block ingest from re-adding rows on stale cache.
 */
let lastTombstoneQueryDocIds: ReadonlySet<string> = new Set();

/** Throttle for getDocs tombstone reconcile (onSnapshot can be unreliable in some webviews). */
let lastTombstoneGetDocsAt = 0;

let appSingleton: FirebaseApp | null = null;
let dbSingleton: Firestore | null = null;

export function getOrInitApp(): FirebaseApp {
  if (appSingleton) {
    return appSingleton;
  }
  if (getApps().length > 0) {
    appSingleton = getApp();
    return appSingleton;
  }
  appSingleton = initializeApp(getFirebaseWebOptions());
  return appSingleton;
}

export function getOrInitFirestore(): Firestore {
  if (dbSingleton) {
    return dbSingleton;
  }
  dbSingleton = initializeFirestore(getOrInitApp(), {
    localCache: memoryLocalCache(),
  });
  return dbSingleton;
}

export type SyncIngestRow = {
  id: string;
  text: string;
  createdAt: string;
  theme?: EntryTheme | null;
};

/** Exported for tests. Converts Firestore `createdAt` wire shapes to RFC3339 for Rust `ingest_firestore_entries`. */
export function normalizeFirestoreCreatedAtForIngest(value: unknown): string | null {
  if (value == null) {
    return null;
  }
  if (typeof value === "string" && value.trim()) {
    return value.trim();
  }
  if (typeof value === "object" && value !== null) {
    const o = value as Record<string, unknown>;
    if (typeof o.toDate === "function") {
      const d = (o as { toDate: () => Date }).toDate();
      if (d instanceof Date && !Number.isNaN(d.getTime())) {
        return d.toISOString();
      }
      return null;
    }
    if (typeof o.seconds === "number" && Number.isFinite(o.seconds)) {
      const nano =
        typeof o.nanoseconds === "number" && Number.isFinite(o.nanoseconds)
          ? o.nanoseconds
          : 0;
      return new Date(o.seconds * 1000 + nano / 1e6).toISOString();
    }
  }
  return null;
}

function partitionFirestoreSnapshotDocs(
  docs: QueryDocumentSnapshot<DocumentData>[]
): { tombstonedIds: string[]; activeRows: SyncIngestRow[] } {
  const tombstonedIds: string[] = [];
  const activeRows: SyncIngestRow[] = [];
  for (const d of docs) {
    const data = d.data();
    if (isFirestoreDocumentTombstoned(data)) {
      tombstonedIds.push(d.id);
      continue;
    }
    const text = typeof data.text === "string" ? data.text.trim() : "";
    if (!text) {
      continue;
    }
    const createdAt = normalizeFirestoreCreatedAtForIngest(data.createdAt);
    if (!createdAt) {
      continue;
    }
    const row: SyncIngestRow = { id: d.id, text, createdAt };
    const theme = parseEntryThemeField(data);
    if (theme !== undefined) {
      row.theme = theme;
    }
    activeRows.push(row);
  }
  return { tombstonedIds, activeRows };
}

async function applyIngestEntryThemes(rows: SyncIngestRow[]): Promise<boolean> {
  if (!isThemesEnabled()) {
    return false;
  }
  let changed = false;
  for (const row of rows) {
    if (row.theme === undefined) {
      continue;
    }
    try {
      if (await applyRemoteEntryTheme(row.id, row.theme)) {
        changed = true;
      }
    } catch (e) {
      if (import.meta.env.DEV) {
        console.error("[chinotto sync] entry theme apply failed", row.id, e);
      }
    }
  }
  return changed;
}

function userThemesCollection(db: Firestore, uid: string): CollectionReference<DocumentData> {
  return collection(db, "users", uid, "user_themes");
}

function userThemeIngestQuery(coll: CollectionReference<DocumentData>) {
  return query(coll, orderBy("updatedAt", "desc"), limit(USER_THEME_QUERY_LIMIT));
}

function userThemeTombstoneQuery(coll: CollectionReference<DocumentData>) {
  return query(
    coll,
    where("deletedAt", "!=", null),
    orderBy("deletedAt", "desc"),
    limit(USER_THEME_QUERY_LIMIT)
  );
}

async function applyRemoteUserThemeSnapshot(
  docs: QueryDocumentSnapshot<DocumentData>[],
  onIngested: () => void
): Promise<void> {
  if (!isThemesEnabled()) {
    return;
  }
  const { tombstonedIds, activeRows } = partitionUserThemeDocs(
    docs.map((d) => ({ id: d.id, data: () => d.data() }))
  );
  let changed = false;
  if (tombstonedIds.length > 0) {
    try {
      const removed = await applyRemoteUserThemeTombstones(tombstonedIds);
      if (removed > 0) {
        changed = true;
      }
    } catch (e) {
      if (import.meta.env.DEV) {
        console.error("[chinotto sync] user theme tombstone apply failed", e);
      }
    }
  }
  if (activeRows.length > 0) {
    try {
      const applied = await ingestRemoteUserThemes(activeRows);
      if (applied > 0) {
        changed = true;
      }
    } catch (e) {
      if (import.meta.env.DEV) {
        console.error("[chinotto sync] user theme ingest failed", e);
      }
    }
  }
  if (changed) {
    onIngested();
  }
}

async function applyRemoteTombstonesById(ids: string[]): Promise<number> {
  if (ids.length === 0) {
    return 0;
  }
  try {
    return await deleteLocalEntriesForSync(ids);
  } catch (e) {
    console.error("[chinotto sync] tombstone apply failed", e);
    return 0;
  }
}

function tombstoneQuery(coll: CollectionReference<DocumentData>) {
  return query(
    coll,
    where("deletedAt", "!=", null),
    orderBy("deletedAt", "desc"),
    limit(TOMBSTONE_QUERY_LIMIT)
  );
}

/**
 * One-shot server read of tombstoned doc ids (same query as the tombstone listener).
 * Backs up onSnapshot when the listener errors, never attaches, or misses updates in Tauri.
 */
/**
 * Paginated `getDocs` for active-shaped docs outside the live `limit(500)` window (mobile parity).
 * Idempotent with `INSERT OR IGNORE`; respects tombstone query ids and per-doc tombstone field.
 */
async function runFirestoreIngestBackfill(
  coll: CollectionReference<DocumentData>,
  onIngested: () => void,
  shouldAbort: () => boolean,
  onSessionAccessLost?: () => void
): Promise<void> {
  let lastDoc: QueryDocumentSnapshot<DocumentData> | null = null;
  let insertedTotal = 0;

  for (let page = 0; page < INGEST_BACKFILL_MAX_PAGES; page++) {
    if (shouldAbort()) {
      return;
    }
    const q: Query<DocumentData> = lastDoc
      ? query(
          coll,
          orderBy("createdAt", "desc"),
          startAfter(lastDoc),
          limit(INGEST_PAGE_SIZE)
        )
      : query(coll, orderBy("createdAt", "desc"), limit(INGEST_PAGE_SIZE));

    let snap;
    try {
      snap = await getDocs(q);
    } catch (e) {
      console.error("[chinotto sync] ingest backfill getDocs failed", e);
      if (isFirestoreSessionAccessLostError(e)) {
        onSessionAccessLost?.();
      }
      return;
    }
    if (shouldAbort()) {
      return;
    }
    if (snap.empty) {
      break;
    }

    const docs = snap.docs as QueryDocumentSnapshot<DocumentData>[];
    lastDoc = docs[docs.length - 1]!;
    const { tombstonedIds, activeRows } = partitionFirestoreSnapshotDocs(docs);
    await applyRemoteTombstonesById(tombstonedIds);
    if (shouldAbort()) {
      return;
    }
    const activeRowsSafe = activeRows.filter((r) => !lastTombstoneQueryDocIds.has(r.id));
    if (activeRowsSafe.length > 0) {
      try {
        let batchChanged = false;
        const inserted = await ingestFirestoreEntries(activeRowsSafe);
        if (inserted > 0) {
          batchChanged = true;
        }
        if (await applyIngestEntryThemes(activeRowsSafe)) {
          batchChanged = true;
        }
        if (batchChanged) {
          insertedTotal += Math.max(inserted, 1);
        }
      } catch (e) {
        if (import.meta.env.DEV) {
          console.error("[chinotto sync] ingest backfill batch failed", e);
        }
        return;
      }
    }

    if (snap.docs.length < INGEST_PAGE_SIZE) {
      break;
    }
  }

  if (insertedTotal > 0) {
    onIngested();
  }
}

async function pullTombstonesFromServer(
  coll: CollectionReference<DocumentData>,
  onIngested: () => void,
  options: { force?: boolean; minIntervalMs?: number; onSessionAccessLost?: () => void } = {}
): Promise<void> {
  const force = options.force ?? false;
  const minMs = options.minIntervalMs ?? 2000;
  const now = Date.now();
  if (!force && now - lastTombstoneGetDocsAt < minMs) {
    return;
  }
  lastTombstoneGetDocsAt = now;
  try {
    const snap = await getDocs(tombstoneQuery(coll));
    const ids = snap.docs.map((d) => d.id);
    lastTombstoneQueryDocIds = new Set(ids);
    const removed = await applyRemoteTombstonesById(ids);
    if (removed > 0) {
      onIngested();
    }
  } catch (e) {
    console.error("[chinotto sync] tombstone getDocs failed", e);
    if (isFirestoreSessionAccessLostError(e)) {
      options.onSessionAccessLost?.();
    }
  }
}

export type FirestoreEntryPush = {
  id: string;
  text: string;
  created_at: string;
};

/**
 * Upsert `users/{uid}/entries/{id}` so mobile (and other clients) receive new or restored thoughts.
 * Uses merge; `deletedAt` is cleared so Cmd+Z after a synced delete can revive the doc remotely.
 */
export async function pushEntryUpsertToFirestore(
  entry: FirestoreEntryPush,
  theme?: EntryTheme | null
): Promise<boolean> {
  if (!isFirebaseSyncConfigured()) {
    return false;
  }
  const auth = getAuth(getOrInitApp());
  const user = auth.currentUser;
  if (!user || user.isAnonymous) {
    return false;
  }
  const trimmed = entry.text.trim();
  if (!trimmed) {
    return false;
  }
  const ms = Date.parse(entry.created_at);
  if (Number.isNaN(ms)) {
    return false;
  }
  const db = getOrInitFirestore();
  const ref = doc(db, "users", user.uid, "entries", entry.id);
  const payload: Record<string, unknown> = {
    text: trimmed,
    createdAt: Timestamp.fromMillis(ms),
    updatedAt: serverTimestamp(),
    deletedAt: deleteField(),
  };
  if (isThemesEnabled() && theme !== undefined) {
    payload.theme = theme == null ? null : toFirestoreEntryThemeWire(theme);
  }
  try {
    await setDoc(ref, payload, { merge: true });
    return true;
  } catch (e) {
    if (isFirestoreSessionAccessLostError(e)) {
      void invalidateFirebaseSyncAfterRemoteSessionLost("pushEntry");
      return false;
    }
    if (import.meta.env.DEV) {
      console.warn("[chinotto sync] push entry failed", entry.id, e);
    }
    return false;
  }
}

function localFirestoreUploadDoneForUid(uid: string): boolean {
  try {
    return localStorage.getItem(LOCAL_FIRESTORE_UPLOAD_UID_KEY) === uid;
  } catch {
    return false;
  }
}

function markLocalFirestoreUploadDone(uid: string): void {
  try {
    localStorage.setItem(LOCAL_FIRESTORE_UPLOAD_UID_KEY, uid);
  } catch {
    /* ignore */
  }
}

/**
 * After first sign-in per uid: push all local entries to Firestore so mobile can ingest them.
 * Skips tombstone outbox ids; runs after `flushSyncTombstoneOutbox` so deletes win.
 */
async function runLocalEntriesFirestoreBackfillUpload(
  uid: string,
  shouldAbort: () => boolean
): Promise<void> {
  if (localUploadInFlight) {
    return localUploadInFlight;
  }
  localUploadInFlight = runLocalEntriesFirestoreBackfillUploadInner(uid, shouldAbort).finally(
    () => {
      localUploadInFlight = null;
    }
  );
  return localUploadInFlight;
}

async function runLocalEntriesFirestoreBackfillUploadInner(
  uid: string,
  shouldAbort: () => boolean
): Promise<void> {
  if (localFirestoreUploadDoneForUid(uid)) {
    return;
  }
  let entries: Entry[];
  try {
    entries = await listEntries();
  } catch (e) {
    console.error("[chinotto sync] local upload list_entries failed", e);
    return;
  }
  let tombstonePending = new Set<string>();
  try {
    tombstonePending = new Set(await listSyncTombstoneOutbox());
  } catch (e) {
    console.error("[chinotto sync] local upload list tombstone outbox failed", e);
    return;
  }
  const candidates = entries.filter((e) => !tombstonePending.has(e.id));
  if (candidates.length === 0) {
    markLocalFirestoreUploadDone(uid);
    return;
  }

  console.log(`[chinotto sync] local upload: pushing ${candidates.length} entries to Firestore`);

  let failed = 0;
  for (let i = 0; i < candidates.length; i += LOCAL_UPLOAD_BATCH_SIZE) {
    if (shouldAbort()) {
      return;
    }
    const batch = candidates.slice(i, i + LOCAL_UPLOAD_BATCH_SIZE);
    for (const entry of batch) {
      if (shouldAbort()) {
        return;
      }
      const ok = await pushEntryUpsertToFirestore(
        {
          id: entry.id,
          text: entry.text,
          created_at: entry.created_at,
        },
        isThemesEnabled() ? await getEntryTheme(entry.id) : undefined
      );
      if (!ok) {
        failed += 1;
        if (shouldAbort()) {
          return;
        }
      }
    }
  }

  if (failed > 0) {
    console.warn(
      `[chinotto sync] local upload: ${failed}/${candidates.length} entry pushes failed; will retry after next sign-in`
    );
    return;
  }

  markLocalFirestoreUploadDone(uid);
  console.log(`[chinotto sync] local upload: done (${candidates.length} entries)`);
}

/**
 * Push pre-sync local SQLite entries to Firestore once per uid. Runs on auth (not gated by sync modal).
 */
export function startLocalEntriesFirestoreUploadOnAuth(): () => void {
  if (!isFirebaseSyncConfigured()) {
    return () => {};
  }

  let auth: ReturnType<typeof getAuth>;
  try {
    auth = getAuth(getOrInitApp());
  } catch (e) {
    console.error("[chinotto sync] Firebase init failed; local upload disabled.", e);
    return () => {};
  }

  let aborted = false;

  const unsub = onAuthStateChanged(auth, (user) => {
    if (aborted || !user || user.isAnonymous) {
      return;
    }
    const uidAtStart = user.uid;
    void (async () => {
      try {
        await auth.authStateReady();
      } catch {
        /* ignore */
      }
      const stillSignedIn = () =>
        !aborted &&
        auth.currentUser != null &&
        auth.currentUser.uid === uidAtStart &&
        !auth.currentUser.isAnonymous;
      if (!stillSignedIn()) {
        return;
      }
      await flushSyncTombstoneOutbox();
      if (!stillSignedIn()) {
        return;
      }
      await runLocalEntriesFirestoreBackfillUpload(uidAtStart, () => !stillSignedIn());
    })();
  });

  return () => {
    aborted = true;
    unsub();
  };
}

let tombstoneFlushRetry: ReturnType<typeof setTimeout> | undefined;

const TOMBSTONE_UNDO_SECS = 8;

/**
 * Flush pending `{ op: "tombstone", entryId }` rows to Firestore with `deletedAt: serverTimestamp()`.
 * Idempotent: `setDoc` + merge on an already-tombstoned doc is allowed.
 *
 * Tombstones younger than eight seconds are skipped so `bring back` can still reach other
 * devices. Remaining young rows schedule another flush when they become due.
 */
export async function flushSyncTombstoneOutbox(): Promise<void> {
  if (!isFirebaseSyncConfigured()) {
    return;
  }
  const auth = getAuth(getOrInitApp());
  const user = auth.currentUser;
  if (!user || user.isAnonymous) {
    return;
  }
  const db = getOrInitFirestore();
  const ids = await listDueSyncTombstoneOutbox(TOMBSTONE_UNDO_SECS);
  for (const entryId of ids) {
    const ref = doc(db, "users", user.uid, "entries", entryId);
    try {
      await setDoc(ref, { deletedAt: serverTimestamp() }, { merge: true });
      await removeSyncTombstoneOutbox(entryId);
      await clearFirestoreIngestSuppression(entryId);
    } catch (e) {
      if (isFirestoreSessionAccessLostError(e)) {
        void invalidateFirebaseSyncAfterRemoteSessionLost("tombstoneFlush");
        break;
      }
      if (import.meta.env.DEV) {
        console.warn("[chinotto sync] tombstone flush failed, will retry", entryId, e);
      }
    }
  }
  const stillWaiting = (await listSyncTombstoneOutbox()).filter((id) => !ids.includes(id));
  if (stillWaiting.length > 0 && tombstoneFlushRetry == null) {
    tombstoneFlushRetry = setTimeout(() => {
      tombstoneFlushRetry = undefined;
      void flushSyncTombstoneOutbox();
    }, 1000);
  }
}

/**
 * After local SQLite delete (user action): enqueue tombstone and try immediate Firestore flush.
 */
export async function notifyEntryDeletedForSync(entryId: string): Promise<void> {
  if (!isFirebaseSyncConfigured()) {
    return;
  }
  await enqueueSyncTombstone(entryId);
  await flushSyncTombstoneOutbox();
}

/**
 * Subscribe to auth + Firestore `users/{uid}/entries` (see docs/internal/sync.md; wire contract: mobile `docs/internal/sync/sync.md`).
 * Applies remote tombstones (physical local delete) and ingests active docs.
 */
export function startDesktopFirestoreIngest(onIngested: () => void): () => void {
  if (!isFirebaseSyncConfigured()) {
    return () => {};
  }

  let auth: ReturnType<typeof getAuth>;
  try {
    auth = getAuth(getOrInitApp());
  } catch (e) {
    console.error("[chinotto sync] Firebase init failed; sync disabled for this session.", e);
    return () => {};
  }

  let unsubIngest: (() => void) | undefined;
  let unsubTombstones: (() => void) | undefined;
  let unsubUserThemes: (() => void) | undefined;
  let unsubUserThemeTombstones: (() => void) | undefined;
  let tombstonePollTimer: ReturnType<typeof setInterval> | undefined;
  let ingestBackfillAbort = false;

  const detachFirestoreListeners = () => {
    ingestBackfillAbort = true;
    if (tombstonePollTimer != null) {
      clearInterval(tombstonePollTimer);
      tombstonePollTimer = undefined;
    }
    lastTombstoneQueryDocIds = new Set();
    lastTombstoneGetDocsAt = 0;
    unsubIngest?.();
    unsubIngest = undefined;
    unsubTombstones?.();
    unsubTombstones = undefined;
    unsubUserThemes?.();
    unsubUserThemes = undefined;
    unsubUserThemeTombstones?.();
    unsubUserThemeTombstones = undefined;
  };

  const unsubAuth = onAuthStateChanged(auth, (user) => {
    detachFirestoreListeners();
    detachIngestOnExternalSessionLoss = null;
    if (!user || user.isAnonymous) {
      return;
    }
    ingestBackfillAbort = false;
    const uidAtStart = user.uid;
    let coll: ReturnType<typeof collection>;
    let userThemeColl: ReturnType<typeof collection>;
    try {
      const db = getOrInitFirestore();
      coll = collection(db, "users", user.uid, "entries");
      userThemeColl = userThemesCollection(db, user.uid);
    } catch (e) {
      console.error("[chinotto sync] Firestore init failed after sign-in; ingest skipped.", e);
      return;
    }

    const handleIngestSessionAccessLost = () => {
      detachFirestoreListeners();
      detachIngestOnExternalSessionLoss = null;
      void invalidateFirebaseSyncAfterRemoteSessionLost("ingest");
    };
    detachIngestOnExternalSessionLoss = detachFirestoreListeners;

    void (async () => {
      await flushSyncTombstoneOutbox();
      await flushSyncUserThemeOutbox();
      const stillSignedIn = () =>
        !ingestBackfillAbort &&
        auth.currentUser != null &&
        auth.currentUser.uid === uidAtStart &&
        !auth.currentUser.isAnonymous;
      if (!stillSignedIn()) {
        return;
      }
      await pullTombstonesFromServer(coll, onIngested, {
        force: true,
        onSessionAccessLost: handleIngestSessionAccessLost,
      });
      if (!stillSignedIn()) {
        return;
      }
      await runFirestoreIngestBackfill(
        coll,
        onIngested,
        () => !stillSignedIn(),
        handleIngestSessionAccessLost
      );
      if (!stillSignedIn()) {
        return;
      }

      tombstonePollTimer = setInterval(() => {
        void pullTombstonesFromServer(coll, onIngested, {
          force: true,
          onSessionAccessLost: handleIngestSessionAccessLost,
        });
      }, 12_000);
      const qIngest = query(coll, orderBy("createdAt", "desc"), limit(INGEST_PAGE_SIZE));
      unsubIngest = onSnapshot(
        qIngest,
        async (snap) => {
          // Always reconcile tombstones from the server before ingesting "active" rows from this
          // snapshot. Otherwise `pullTombstonesFromServer` can no-op (2s throttle) while `snap` is
          // still stale → we skip `lastTombstoneQueryDocIds` and re-insert a row Firestore already
          // tombstoned (mobile wrote `deletedAt`).
          await pullTombstonesFromServer(coll, onIngested, {
            force: true,
            onSessionAccessLost: handleIngestSessionAccessLost,
          });
          const { tombstonedIds, activeRows } = partitionFirestoreSnapshotDocs(
            snap.docs as QueryDocumentSnapshot<DocumentData>[]
          );
          let changed = false;
          const removedMain = await applyRemoteTombstonesById(tombstonedIds);
          if (removedMain > 0) {
            changed = true;
          }
          const activeRowsSafe = activeRows.filter((r) => !lastTombstoneQueryDocIds.has(r.id));
          if (activeRowsSafe.length > 0) {
            try {
              const inserted = await ingestFirestoreEntries(activeRowsSafe);
              if (inserted > 0) {
                changed = true;
              }
              if (await applyIngestEntryThemes(activeRowsSafe)) {
                changed = true;
              }
            } catch (e) {
              if (import.meta.env.DEV) {
                console.error("[chinotto sync] ingest failed", e);
              }
            }
          }
          if (changed) {
            onIngested();
          }
          await flushSyncTombstoneOutbox();
          await flushSyncUserThemeOutbox();
        },
        (err) => {
          console.error("[chinotto sync] ingest snapshot error", err);
          if (isFirestoreSessionAccessLostError(err)) {
            handleIngestSessionAccessLost();
          }
        }
      );

      const qTombstones = tombstoneQuery(coll);
      unsubTombstones = onSnapshot(
        qTombstones,
        async (snap) => {
          // Query already enforces `deletedAt != null`; do not rely on JS tombstone heuristics here.
          const ids = snap.docs.map((d) => d.id);
          lastTombstoneQueryDocIds = new Set(ids);
          const removed = await applyRemoteTombstonesById(ids);
          if (removed > 0) {
            onIngested();
          }
          await flushSyncTombstoneOutbox();
          await flushSyncUserThemeOutbox();
        },
        (err) => {
          console.error("[chinotto sync] tombstone snapshot error", err);
          if (isFirestoreSessionAccessLostError(err)) {
            handleIngestSessionAccessLost();
          }
        }
      );

      void getDocs(userThemeIngestQuery(userThemeColl)).then(async (snap) => {
        if (!stillSignedIn()) {
          return;
        }
        await applyRemoteUserThemeSnapshot(
          snap.docs as QueryDocumentSnapshot<DocumentData>[],
          onIngested
        );
      });

      unsubUserThemes = onSnapshot(
        userThemeIngestQuery(userThemeColl),
        async (snap) => {
          await applyRemoteUserThemeSnapshot(
            snap.docs as QueryDocumentSnapshot<DocumentData>[],
            onIngested
          );
        },
        (err) => {
          console.error("[chinotto sync] user theme ingest error", err);
          if (isFirestoreSessionAccessLostError(err)) {
            handleIngestSessionAccessLost();
          }
        }
      );

      unsubUserThemeTombstones = onSnapshot(
        userThemeTombstoneQuery(userThemeColl),
        async (snap) => {
          if (!isThemesEnabled()) {
            return;
          }
          const ids = snap.docs.map((d) => d.id);
          try {
            const removed = await applyRemoteUserThemeTombstones(ids);
            if (removed > 0) {
              onIngested();
            }
          } catch (e) {
            if (import.meta.env.DEV) {
              console.error("[chinotto sync] user theme tombstone snapshot failed", e);
            }
          }
        },
        (err) => {
          console.error("[chinotto sync] user theme tombstone snapshot error", err);
          if (isFirestoreSessionAccessLostError(err)) {
            handleIngestSessionAccessLost();
          }
        }
      );
    })();
  });

  return () => {
    detachFirestoreListeners();
    unsubAuth();
  };
}

function authErrorCode(e: unknown): string {
  if (e && typeof e === "object" && "code" in e && typeof (e as { code: unknown }).code === "string") {
    const c = (e as { code: string }).code;
    /** Some SDKs / wrappers emit `failed-precondition` without the `auth/` prefix. */
    return c === "failed-precondition" ? "auth/failed-precondition" : c;
  }
  return "";
}

function isAuthFailedPrecondition(e: unknown): boolean {
  return authErrorCode(e) === "auth/failed-precondition";
}

/**
 * Applies the Apple OAuth credential from the bridge webview in the **main** window.
 *
 * `signInWithCredential` can throw `auth/failed-precondition` if a non-anonymous user is already
 * signed in (Firebase expects sign-out or `linkWithCredential` for anonymous). We sign out first
 * when replacing an existing session so “Continue with Apple” always applies the fresh credential.
 */
export async function signInWithAppleCredential(credentialJson: BridgedOAuthCredentialJson): Promise<void> {
  if (!isFirebaseSyncConfigured()) {
    throw new Error("Sync is not configured");
  }
  const idToken = credentialJson.idToken?.trim();
  if (!idToken) {
    throw new Error("Apple sign-in did not provide an ID token");
  }
  const auth = getAuth(getOrInitApp());
  await auth.authStateReady();
  const credential = OAuthProvider.credentialFromJSON({
    ...credentialJson,
    idToken,
    accessToken: credentialJson.accessToken?.trim() ? credentialJson.accessToken : undefined,
  });
  if (!credential) {
    throw new Error("Apple sign-in credential could not be built");
  }

  const signInAfterClearingSession = async () => {
    try {
      await signInWithCredential(auth, credential);
    } catch (e) {
      if (!isAuthFailedPrecondition(e)) {
        throw e;
      }
      await signOut(auth);
      await auth.authStateReady();
      await signInWithCredential(auth, credential);
    }
  };

  const cur = auth.currentUser;
  if (cur?.isAnonymous) {
    try {
      await linkWithCredential(cur, credential);
    } catch (e) {
      if (isAuthFailedPrecondition(e)) {
        await signOut(auth);
        await auth.authStateReady();
        await signInAfterClearingSession();
      } else {
        throw e;
      }
    }
  } else if (cur) {
    await signOut(auth);
    await auth.authStateReady();
    await signInAfterClearingSession();
  } else {
    await signInAfterClearingSession();
  }

  await flushSyncTombstoneOutbox();
  await flushSyncUserThemeOutbox();
  void backfillLocalThemesToRemote();
}

export async function signOutFirebaseSync(): Promise<void> {
  if (!isFirebaseSyncConfigured()) {
    return;
  }
  const auth = getAuth(getOrInitApp());
  await signOut(auth);
}

let firebaseSyncInvalidation: Promise<void> | null = null;

/**
 * While Firestore ingest listeners are attached, `subscribeChinottoUserSyncAccess` can detach them when
 * the cloud session is invalid (e.g. account deleted on mobile).
 */
let detachIngestOnExternalSessionLoss: (() => void) | null = null;

/**
 * Clears the local tombstone queue and signs out of Firebase so the app stays local-only without
 * tight retries on a dead `users/{uid}` path (account removed on another client, revoked token, etc.).
 */
export function invalidateFirebaseSyncAfterRemoteSessionLost(source: string): Promise<void> {
  if (firebaseSyncInvalidation) {
    return firebaseSyncInvalidation;
  }
  firebaseSyncInvalidation = (async () => {
    if (import.meta.env.DEV) {
      console.warn(`[chinotto sync] ending cloud session (${source})`);
    }
    try {
      await clearSyncTombstoneOutboxAll();
    } catch (err) {
      console.warn("[chinotto sync] clear tombstone outbox failed", err);
    }
    try {
      await clearSyncUserThemeOutboxAll();
    } catch (err) {
      console.warn("[chinotto sync] clear user theme outbox failed", err);
    }
    try {
      await signOutFirebaseSync();
    } catch (err) {
      console.warn("[chinotto sync] signOut after invalid session failed", err);
    }
  })().finally(() => {
    firebaseSyncInvalidation = null;
  });
  return firebaseSyncInvalidation;
}

export function subscribeSyncAuth(onChange: (user: User | null) => void): () => void {
  if (!isFirebaseSyncConfigured()) {
    onChange(null);
    return () => {};
  }
  try {
    const auth = getAuth(getOrInitApp());
    return onAuthStateChanged(auth, onChange);
  } catch (e) {
    console.error("[chinotto sync] subscribeSyncAuth: Firebase init failed.", e);
    onChange(null);
    return () => {};
  }
}

/**
 * Desktop sync modal: poll for mobile unlock on this session (`?ds=` on the QR URL).
 * Uses **getDocFromServer** polling (not cache) instead of onSnapshot to avoid extra watch streams
 * (see SYNC_MODAL_GATE_POLL_MS) and stale `chinottoSyncAccess` after mobile writes.
 * Rules must allow unauthenticated **read** on `sync_desktop_sessions/{sessionId}`.
 */
export function subscribeDesktopSyncGateSession(
  sessionId: string,
  onUnlocked: (unlocked: boolean) => void,
  options?: { onPermissionDenied?: () => void; onReadSucceeded?: () => void }
): () => void {
  if (!isFirebaseSyncConfigured()) {
    onUnlocked(false);
    return () => {};
  }
  if (!sessionId?.trim()) {
    onUnlocked(false);
    return () => {};
  }
  try {
    const db = getOrInitFirestore();
    const ref = doc(db, "sync_desktop_sessions", sessionId);
    let stopped = false;
    let loggedPermissionDenied = false;
    const poll = async () => {
      if (stopped) {
        return;
      }
      try {
        const snap = await getDocFromServer(ref);
        if (stopped) {
          return;
        }
        options?.onReadSucceeded?.();
        onUnlocked(snap.data()?.unlocked === true);
      } catch (e) {
        if (isFirestorePermissionDenied(e)) {
          if (!loggedPermissionDenied) {
            loggedPermissionDenied = true;
            console.warn(
              `[chinotto sync] desktop gate: permission denied — ${FIRESTORE_RULES_SYNC_MODAL_HINT}`,
              e
            );
            options?.onPermissionDenied?.();
          }
        } else {
          console.error("[chinotto sync] desktop gate poll error", e);
        }
        if (!stopped) {
          onUnlocked(false);
        }
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), SYNC_MODAL_GATE_POLL_MS);
    return () => {
      stopped = true;
      clearInterval(timer);
    };
  } catch (e) {
    console.error("[chinotto sync] desktop gate listener setup failed", e);
    onUnlocked(false);
    return () => {};
  }
}

/**
 * After Sign in with Apple: mobile mirrors paid sync access on `users/{uid}`.
 * Uses **`onSnapshot`** for push updates, plus **`getDocFromServer`** on focus/visibility and on a
 * timer (fast while waiting for access, slower while `active` so **revokes** from mobile still land
 * if the listener stalls) — embedded WebKit can stop delivering snapshot updates until restart.
 * On **focus / visibility**, nudges the client with `enableNetwork`, **re-attaches** the snapshot
 * listener, and (while still waiting for `active`) runs a short **burst** of server reads — background
 * tabs throttle `setInterval`, so polling alone may not run until the app is restarted.
 * Transitions to **inactive** are debounced briefly once the UI has shown active, so reattach/network
 * hiccups after restore from tray do not flash “sync off”.
 */
export function subscribeChinottoUserSyncAccess(
  uid: string,
  onActive: (active: boolean) => void,
  options?: { onPermissionDenied?: () => void; onReadSucceeded?: () => void }
): () => void {
  if (!isFirebaseSyncConfigured()) {
    onActive(false);
    return () => {};
  }
  if (!uid?.trim()) {
    onActive(false);
    return () => {};
  }
  try {
    const db = getOrInitFirestore();
    const ref = doc(db, "users", uid);
    let stopped = false;
    let snapshotUnsub: (() => void) | null = null;
    let loggedPermissionDenied = false;
    let lastActive = false;
    let lastEmittedToUi: boolean | null = null;
    let emitFalsePending: ReturnType<typeof setTimeout> | null = null;

    const clearEmitFalsePending = () => {
      if (emitFalsePending != null) {
        clearTimeout(emitFalsePending);
        emitFalsePending = null;
      }
    };

    const emitActiveToUi = (active: boolean) => {
      if (stopped) {
        return;
      }
      if (active) {
        clearEmitFalsePending();
        if (lastEmittedToUi !== true) {
          lastEmittedToUi = true;
          onActive(true);
        }
        return;
      }
      if (lastEmittedToUi !== true) {
        if (lastEmittedToUi !== false) {
          lastEmittedToUi = false;
          onActive(false);
        }
        return;
      }
      clearEmitFalsePending();
      emitFalsePending = setTimeout(() => {
        emitFalsePending = null;
        if (stopped) {
          return;
        }
        lastEmittedToUi = false;
        onActive(false);
      }, SYNC_ACCESS_EMIT_FALSE_DEBOUNCE_MS);
    };

    let serverPoll: ReturnType<typeof setInterval> | null = null;
    const clearServerPoll = () => {
      if (serverPoll != null) {
        clearInterval(serverPoll);
        serverPoll = null;
      }
    };

    const scheduleServerPoll = () => {
      clearServerPoll();
      if (stopped) {
        return;
      }
      const ms = lastActive ? SYNC_ACCESS_WHILE_ACTIVE_POLL_MS : SYNC_ACCESS_WAITING_POLL_MS;
      serverPoll = setInterval(() => {
        void refetchFromServer();
      }, ms);
    };

    const applySnap = (snap: DocumentSnapshot) => {
      if (stopped) {
        return;
      }
      const prev = lastActive;
      lastActive = isChinottoSyncAccessActiveInUserDoc(snap.data());
      options?.onReadSucceeded?.();
      emitActiveToUi(lastActive);
      if (prev !== lastActive) {
        scheduleServerPoll();
      }
    };

    const applyError = (e: unknown) => {
      if (isFirestoreSessionAccessLostError(e)) {
        if (!loggedPermissionDenied) {
          loggedPermissionDenied = true;
          if (isFirestorePermissionDenied(e)) {
            console.warn(
              `[chinotto sync] user sync access: permission denied — ${FIRESTORE_RULES_SYNC_MODAL_HINT}`,
              e
            );
          } else {
            console.warn("[chinotto sync] user sync access: session no longer valid for Firestore.", e);
          }
        }
        if (!stopped) {
          stopped = true;
          clearEmitFalsePending();
          clearServerPoll();
          if (snapshotUnsub != null) {
            snapshotUnsub();
            snapshotUnsub = null;
          }
          detachIngestOnExternalSessionLoss?.();
          detachIngestOnExternalSessionLoss = null;
          lastActive = false;
          emitActiveToUi(false);
          void invalidateFirebaseSyncAfterRemoteSessionLost("userSyncProfile");
        }
        return;
      }
      if (isFirestorePermissionDenied(e)) {
        if (!loggedPermissionDenied) {
          loggedPermissionDenied = true;
          console.warn(
            `[chinotto sync] user sync access: permission denied — ${FIRESTORE_RULES_SYNC_MODAL_HINT}`,
            e
          );
          options?.onPermissionDenied?.();
        }
      } else {
        console.error("[chinotto sync] user sync access listener error", e);
      }
      if (!stopped) {
        const prev = lastActive;
        lastActive = false;
        emitActiveToUi(false);
        if (prev !== lastActive) {
          scheduleServerPoll();
        }
      }
    };

    const refetchFromServer = async () => {
      if (stopped) {
        return;
      }
      try {
        const snap = await getDocFromServer(ref);
        if (stopped) {
          return;
        }
        applySnap(snap);
      } catch (e) {
        if (isFirestoreSessionAccessLostError(e)) {
          applyError(e);
        } else {
          console.warn("[chinotto sync] user sync access: getDocFromServer refetch failed", e);
        }
      }
    };

    const attachSnapshotListener = () => {
      if (snapshotUnsub != null) {
        snapshotUnsub();
        snapshotUnsub = null;
      }
      if (stopped) {
        return;
      }
      snapshotUnsub = onSnapshot(ref, applySnap, applyError);
    };

    attachSnapshotListener();

    scheduleServerPoll();
    void refetchFromServer();

    const staggerIds: number[] = [];
    for (const ms of SYNC_ACCESS_WAITING_STAGGER_MS) {
      staggerIds.push(
        window.setTimeout(() => {
          if (stopped || lastActive) {
            return;
          }
          void refetchFromServer();
        }, ms)
      );
    }

    let focusDebounce: ReturnType<typeof setTimeout> | null = null;
    let focusBurst: ReturnType<typeof setInterval> | null = null;
    const clearFocusBurst = () => {
      if (focusBurst != null) {
        clearInterval(focusBurst);
        focusBurst = null;
      }
    };

    const scheduleFocusRecover = () => {
      if (typeof window === "undefined") {
        return;
      }
      if (focusDebounce != null) {
        clearTimeout(focusDebounce);
      }
      focusDebounce = setTimeout(() => {
        focusDebounce = null;
        void (async () => {
          try {
            await enableNetwork(db);
          } catch {
            /* ignore — best-effort reconnect */
          }
          attachSnapshotListener();
          await refetchFromServer();
          clearFocusBurst();
          let burstCount = 0;
          focusBurst = setInterval(() => {
            if (stopped || lastActive) {
              clearFocusBurst();
              return;
            }
            burstCount += 1;
            if (burstCount > SYNC_ACCESS_FOCUS_BURST_MAX) {
              clearFocusBurst();
              return;
            }
            void refetchFromServer();
          }, SYNC_ACCESS_FOCUS_BURST_MS);
        })();
      }, 250);
    };

    const onVisibility = () => {
      if (typeof document !== "undefined" && document.visibilityState === "visible") {
        scheduleFocusRecover();
      }
    };

    const onPageShow = (e: PageTransitionEvent) => {
      if (e.persisted) {
        scheduleFocusRecover();
      }
    };

    if (typeof window !== "undefined") {
      window.addEventListener("focus", scheduleFocusRecover);
      window.addEventListener("online", scheduleFocusRecover);
      window.addEventListener("pageshow", onPageShow);
    }
    if (typeof document !== "undefined") {
      document.addEventListener("visibilitychange", onVisibility);
    }

    let tauriFocusUnlisten: (() => void) | null = null;
    void (async () => {
      try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        const unlisten = await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
          if (stopped) {
            return;
          }
          if (focused) {
            scheduleFocusRecover();
          }
        });
        if (stopped) {
          unlisten();
          return;
        }
        tauriFocusUnlisten = unlisten;
      } catch {
        /* Web build, tests, or non-Tauri — DOM focus/visibility only */
      }
    })();

    return () => {
      stopped = true;
      clearEmitFalsePending();
      for (const id of staggerIds) {
        clearTimeout(id);
      }
      clearServerPoll();
      clearFocusBurst();
      if (focusDebounce != null) {
        clearTimeout(focusDebounce);
      }
      if (typeof window !== "undefined") {
        window.removeEventListener("focus", scheduleFocusRecover);
        window.removeEventListener("online", scheduleFocusRecover);
        window.removeEventListener("pageshow", onPageShow);
      }
      if (typeof document !== "undefined") {
        document.removeEventListener("visibilitychange", onVisibility);
      }
      if (tauriFocusUnlisten != null) {
        tauriFocusUnlisten();
        tauriFocusUnlisten = null;
      }
      if (snapshotUnsub != null) {
        snapshotUnsub();
        snapshotUnsub = null;
      }
    };
  } catch (e) {
    console.error("[chinotto sync] user sync access listener setup failed", e);
    onActive(false);
    return () => {};
  }
}

/** One server read (e.g. diagnostics); modal uses {@link subscribeChinottoUserSyncAccess} for live updates. */
export async function fetchChinottoUserSyncAccessActive(uid: string): Promise<{
  active: boolean;
  permissionDenied: boolean;
}> {
  if (!isFirebaseSyncConfigured() || !uid?.trim()) {
    return { active: false, permissionDenied: false };
  }
  try {
    const db = getOrInitFirestore();
    const ref = doc(db, "users", uid);
    const snap = await getDocFromServer(ref);
    return {
      active: isChinottoSyncAccessActiveInUserDoc(snap.data()),
      permissionDenied: false,
    };
  } catch (e) {
    if (isFirestorePermissionDenied(e)) {
      console.warn(
        `[chinotto sync] fetch user sync access: permission denied — ${FIRESTORE_RULES_SYNC_MODAL_HINT}`,
        e
      );
      return { active: false, permissionDenied: true };
    }
    console.error("[chinotto sync] fetch user sync access failed", e);
    return { active: false, permissionDenied: false };
  }
}


/** Whether this mac is actually signed in, as opposed to merely configured for sync. */
export function isSignedInForSync(): boolean {
  if (!isFirebaseSyncConfigured()) return false;
  try {
    const user = getAuth(getOrInitApp()).currentUser;
    return user != null && !user.isAnonymous;
  } catch {
    return false;
  }
}

/**
 * Deletes the copy of the record in the cloud, and the account it belonged to.
 *
 * The record on this mac and on the phone is untouched — they just stop meeting. Apple
 * requires a recent sign-in before it will delete an account, and when it asks for one this
 * throws rather than reporting a deletion that did not happen: an account-deletion screen
 * that lies is worse than one that fails.
 */
export async function deleteCloudAccount(): Promise<void> {
  if (!isFirebaseSyncConfigured()) {
    throw new Error("sync is not set up on this mac");
  }
  const auth = getAuth(getOrInitApp());
  const user = auth.currentUser;
  if (!user) {
    throw new Error("this mac is not signed in");
  }
  const db = getOrInitFirestore();

  // The entries first: deleting the auth user revokes the credential that authorises this.
  const entries = await getDocs(collection(db, "users", user.uid, "entries"));
  for (const d of entries.docs) {
    await deleteDoc(d.ref);
  }
  await deleteDoc(doc(db, "users", user.uid)).catch(() => {
    // A user document that was never written is not an error.
  });
  await user.delete();
}
