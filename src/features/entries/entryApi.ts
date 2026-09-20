import { invoke } from "@tauri-apps/api/core";
import type { Entry } from "../../types/entry";

export async function createEntry(
  text: string,
  options?: { spaceId?: string }
): Promise<string> {
  return invoke<string>("create_entry", {
    input: {
      text,
      ...(options?.spaceId ? { spaceId: options.spaceId } : {}),
    },
  });
}

export async function restoreEntry(
  id: string,
  text: string,
  created_at: string,
  space_id?: string | null
): Promise<string> {
  return invoke<string>("restore_entry", {
    input: {
      id,
      text,
      createdAt: created_at,
      ...(space_id ? { spaceId: space_id } : {}),
    },
  });
}

export async function updateEntry(entryId: string, text: string): Promise<void> {
  return invoke("update_entry", { entryId, text });
}

export type ContinuationMarker = {
  continuation_from: number;
  continuation_at: string;
};

export async function markEntryContinuation(
  entryId: string,
  fromOffset: number,
  text: string
): Promise<ContinuationMarker | null> {
  return invoke<ContinuationMarker | null>("mark_entry_continuation", {
    entryId,
    fromOffset,
    text,
  });
}

export async function setEntrySpace(
  entryId: string,
  spaceId: string | null
): Promise<void> {
  return invoke("set_entry_space", { entryId, spaceId });
}

export function generateEmbedding(entryId: string): void {
  invoke("generate_embedding", { entryId }).catch((err) => {
    console.warn("[chinotto] generate_embedding failed", entryId, err);
  });
}

export type EntryTheme = {
  themeId: string;
  confidence: number;
  source: string;
  locked: boolean;
};

import { isThemesEnabled } from "@/lib/themeSettings";
import { scheduleEntryThemePush } from "@/lib/entryThemePush";

export function classifyEntryTheme(entryId: string): void {
  if (!isThemesEnabled()) return;
  invoke("classify_entry_theme", { entryId })
    .then(() => {
      scheduleEntryThemePush(entryId);
    })
    .catch((err) => {
      console.warn("[chinotto] classify_entry_theme failed", entryId, err);
    });
}

export async function getEntryTheme(
  entryId: string
): Promise<EntryTheme | null> {
  return invoke<EntryTheme | null>("get_entry_theme", { entryId });
}

export async function setEntryTheme(
  entryId: string,
  themeId: string | null,
  locked: boolean
): Promise<void> {
  await invoke("set_entry_theme", {
    input: { entryId, themeId, locked },
  });
  scheduleEntryThemePush(entryId);
}

export async function findSimilarEntries(
  entryId: string,
  /** 0 = all entries above the similarity threshold */
  limit = 0
): Promise<Entry[]> {
  return invoke<Entry[]>("find_similar_entries", { entryId, limit });
}

export type Resurfaced = {
  entry: Entry;
  reason: string;
  trail_neighbor_count?: number;
};

export async function getResurfacedEntry(
  excludeIds: string[] = []
): Promise<Resurfaced | null> {
  return invoke<Resurfaced | null>("get_resurfaced_entry", {
    excludeIds,
  });
}

export async function listEntries(spaceFilter?: string): Promise<Entry[]> {
  return invoke<Entry[]>("list_entries", {
    spaceFilter: spaceFilter ?? null,
  });
}

export async function getEntry(entryId: string): Promise<Entry | null> {
  return invoke<Entry | null>("get_entry", { entryId });
}

export async function searchEntries(
  query: string,
  spaceFilter?: string,
  themeFilter?: string
): Promise<Entry[]> {
  return invoke<Entry[]>("search_entries", {
    query,
    spaceFilter: spaceFilter ?? null,
    themeFilter: themeFilter ?? null,
  });
}

export type ThemeCount = {
  themeId: string;
  count: number;
};

export async function listThemeCounts(): Promise<ThemeCount[]> {
  return invoke<ThemeCount[]>("list_theme_counts");
}

export async function listThemeCountsRecent(
  days = 7
): Promise<ThemeCount[]> {
  return invoke<ThemeCount[]>("list_theme_counts_recent", { days });
}

export type UserTheme = {
  id: string;
  label: string;
  sort_order: number;
};

export async function listUserThemes(): Promise<UserTheme[]> {
  return invoke<UserTheme[]>("list_user_themes");
}

export async function createUserTheme(label: string): Promise<UserTheme> {
  return invoke<UserTheme>("create_user_theme", {
    input: { label },
  });
}

export async function updateUserTheme(
  id: string,
  label: string
): Promise<UserTheme> {
  return invoke<UserTheme>("update_user_theme", {
    input: { id, label },
  });
}

export async function deleteUserTheme(id: string): Promise<void> {
  return invoke("delete_user_theme", { id });
}

export type SpaceRow = {
  id: string;
  label: string;
  sort_order: number;
};

export async function listSpaces(): Promise<SpaceRow[]> {
  return invoke<SpaceRow[]>("list_spaces");
}

export async function jumpDatesInMonth(
  year: number,
  month: number,
  spaceFilter?: string
): Promise<string[]> {
  return invoke<string[]>("jump_dates_in_month", {
    year,
    month,
    spaceFilter: spaceFilter ?? null,
  });
}

export async function jumpAnchorForLocalDate(
  localDate: string,
  spaceFilter?: string
): Promise<string | null> {
  return invoke<string | null>("jump_anchor_for_local_date", {
    localDate,
    spaceFilter: spaceFilter ?? null,
  });
}

export async function getThoughtTrail(entryId: string): Promise<Entry[]> {
  return invoke<Entry[]>("get_thought_trail", { entryId });
}

export async function listThoughtTrailEntryIds(): Promise<string[]> {
  return invoke<string[]>("list_thought_trail_entry_ids");
}

export type CaptureContinuationHint = {
  entry_id: string;
  preview: string;
  days_earlier: number;
  shared_terms?: string[];
};

export async function getCaptureContinuationHint(
  text: string,
  excludeId?: string
): Promise<CaptureContinuationHint | null> {
  return invoke<CaptureContinuationHint | null>("get_capture_continuation_hint", {
    text,
    excludeId: excludeId ?? null,
  });
}

export async function pinEntry(entryId: string): Promise<void> {
  return invoke("pin_entry", { entryId });
}

export async function unpinEntry(entryId: string): Promise<void> {
  return invoke("unpin_entry", { entryId });
}

export async function getPinnedEntryIds(): Promise<string[]> {
  return invoke<string[]>("get_pinned_entry_ids");
}

export function recordEntryOpen(entryId: string): void {
  invoke("record_entry_open", { entryId }).catch(() => {});
}

export async function deleteEntry(entryId: string): Promise<void> {
  return invoke("delete_entry", { entryId });
}

/** Wipes all entries (and pins/embeddings). Used by dev/debug UI only. */
export async function deleteAllEntries(): Promise<void> {
  return invoke("delete_all_entries", {});
}

/** Firestore pull ingest (mobile sync.md). Returns count of newly inserted rows. */
export async function ingestFirestoreEntries(
  entries: { id: string; text: string; createdAt: string }[]
): Promise<number> {
  return invoke<number>("ingest_firestore_entries", { entries });
}

/** Sync v2: queue `{ op: "tombstone", entryId }` for Firestore flush (coalesced in SQLite). */
export async function enqueueSyncTombstone(entryId: string): Promise<void> {
  return invoke("enqueue_sync_tombstone", { entryId });
}

export async function listSyncTombstoneOutbox(): Promise<string[]> {
  return invoke<string[]>("list_sync_tombstone_outbox");
}

/** Tombstones that have waited out the undo window. */
export async function listDueSyncTombstoneOutbox(minAgeSecs = 8): Promise<string[]> {
  return invoke<string[]>("list_due_sync_tombstone_outbox", { minAgeSecs });
}

export async function removeSyncTombstoneOutbox(entryId: string): Promise<void> {
  return invoke("remove_sync_tombstone_outbox", { entryId });
}

/** After remote Firebase user / cloud path is invalid (account removed elsewhere). */
export async function clearSyncTombstoneOutboxAll(): Promise<void> {
  return invoke("clear_sync_tombstone_outbox_all");
}

export async function clearFirestoreIngestSuppression(entryId: string): Promise<void> {
  return invoke("clear_firestore_ingest_suppression", { entryId });
}

/** Physically delete local rows for remote tombstones (no new suppression). */
export async function deleteLocalEntriesForSync(entryIds: string[]): Promise<number> {
  if (entryIds.length === 0) {
    return 0;
  }
  return invoke<number>("delete_local_entries_for_sync", { entryIds });
}
