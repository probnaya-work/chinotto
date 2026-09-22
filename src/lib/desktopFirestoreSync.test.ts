import { Timestamp } from "firebase/firestore";
import { beforeEach, describe, expect, it, vi } from "vitest";

// `deleteCloudAccount` calls through `getOrInitApp`/`getOrInitFirestore`, which otherwise
// initialize a real Firebase app from env config. These mocks let it run against an
// in-memory fake instead; `firebase/auth` and `firebase/firestore` keep every other real
// export (via `importOriginal`) so the Timestamp-based tests below are unaffected.
const deleteCallLog = vi.hoisted(() => ({
  calls: [] as string[],
  collections: {} as Record<string, { __path: string }[]>,
}));

vi.mock("firebase/app", () => ({
  getApps: vi.fn(() => []),
  getApp: vi.fn(() => ({})),
  initializeApp: vi.fn(() => ({})),
}));

vi.mock("./firebaseConfig", () => ({
  isFirebaseSyncConfigured: () => true,
  getFirebaseWebOptions: () => ({
    apiKey: "test-api-key",
    authDomain: "test.firebaseapp.com",
    projectId: "test-project",
  }),
}));

vi.mock("firebase/auth", async (importOriginal) => {
  const actual = await importOriginal<typeof import("firebase/auth")>();
  return {
    ...actual,
    getAuth: vi.fn(() => ({
      currentUser: { uid: "uid1", delete: vi.fn(async () => {}) },
    })),
  };
});

vi.mock("firebase/firestore", async (importOriginal) => {
  const actual = await importOriginal<typeof import("firebase/firestore")>();
  return {
    ...actual,
    initializeFirestore: vi.fn(() => ({})),
    memoryLocalCache: vi.fn(() => ({})),
    collection: (_db: unknown, ...segments: string[]) => ({ __path: segments.join("/") }),
    doc: (_db: unknown, ...segments: string[]) => ({ __path: segments.join("/") }),
    getDocs: vi.fn(async (coll: { __path: string }) => {
      deleteCallLog.calls.push(`getDocs:${coll.__path}`);
      const docs = deleteCallLog.collections[coll.__path] ?? [];
      return { docs: docs.map((d) => ({ ref: d })) };
    }),
    deleteDoc: vi.fn(async (ref: { __path: string }) => {
      deleteCallLog.calls.push(`deleteDoc:${ref.__path}`);
    }),
  };
});

import {
  deleteCloudAccount,
  isChinottoSyncAccessActiveInUserDoc,
  isFirestoreSessionAccessLostError,
  normalizeFirestoreCreatedAtForIngest,
} from "./desktopFirestoreSync";

describe("normalizeFirestoreCreatedAtForIngest", () => {
  it("passes through non-empty ISO strings", () => {
    expect(normalizeFirestoreCreatedAtForIngest("2025-01-15T12:00:00.000Z")).toBe(
      "2025-01-15T12:00:00.000Z"
    );
  });

  it("trims string values", () => {
    expect(normalizeFirestoreCreatedAtForIngest("  2025-01-15T12:00:00.000Z  ")).toBe(
      "2025-01-15T12:00:00.000Z"
    );
  });

  it("converts Firestore Timestamp", () => {
    const ts = Timestamp.fromMillis(Date.UTC(2025, 2, 1, 12, 0, 0));
    const out = normalizeFirestoreCreatedAtForIngest(ts);
    expect(out).toBe(ts.toDate().toISOString());
  });

  it("converts plain { seconds, nanoseconds }", () => {
    const out = normalizeFirestoreCreatedAtForIngest({
      seconds: 1740820800,
      nanoseconds: 0,
    });
    expect(out).toBe(new Date(1740820800 * 1000).toISOString());
  });

  it("converts plain { seconds } only", () => {
    const out = normalizeFirestoreCreatedAtForIngest({ seconds: 1740820800 });
    expect(out).toBe(new Date(1740820800 * 1000).toISOString());
  });

  it("returns null for empty or missing", () => {
    expect(normalizeFirestoreCreatedAtForIngest(null)).toBeNull();
    expect(normalizeFirestoreCreatedAtForIngest("")).toBeNull();
    expect(normalizeFirestoreCreatedAtForIngest("   ")).toBeNull();
    expect(normalizeFirestoreCreatedAtForIngest({})).toBeNull();
  });
});

describe("isFirestoreSessionAccessLostError", () => {
  it("is true for permission-denied and unauthenticated Firestore-style errors", () => {
    expect(isFirestoreSessionAccessLostError({ code: "permission-denied" })).toBe(true);
    expect(isFirestoreSessionAccessLostError({ code: "unauthenticated" })).toBe(true);
    expect(isFirestoreSessionAccessLostError({ code: "unavailable" })).toBe(false);
    expect(isFirestoreSessionAccessLostError(null)).toBe(false);
  });
});

describe("isChinottoSyncAccessActiveInUserDoc", () => {
  it("is true only when chinottoSyncAccess.active is strictly true", () => {
    expect(isChinottoSyncAccessActiveInUserDoc(undefined)).toBe(false);
    expect(isChinottoSyncAccessActiveInUserDoc({})).toBe(false);
    expect(isChinottoSyncAccessActiveInUserDoc({ chinottoSyncAccess: {} })).toBe(false);
    expect(isChinottoSyncAccessActiveInUserDoc({ chinottoSyncAccess: { active: false } })).toBe(false);
    expect(isChinottoSyncAccessActiveInUserDoc({ chinottoSyncAccess: { active: true } })).toBe(true);
  });
});

describe("deleteCloudAccount", () => {
  beforeEach(() => {
    deleteCallLog.calls.length = 0;
    for (const key of Object.keys(deleteCallLog.collections)) {
      delete deleteCallLog.collections[key];
    }
    deleteCallLog.collections["users/uid1/entries"] = [{ __path: "users/uid1/entries/e1" }];
    deleteCallLog.collections["users/uid1/user_themes"] = [{ __path: "users/uid1/user_themes/t1" }];
  });

  it("deletes entries and user_themes before the parent account doc", async () => {
    await deleteCloudAccount();

    const entryDeleteIndex = deleteCallLog.calls.indexOf("deleteDoc:users/uid1/entries/e1");
    const themeDeleteIndex = deleteCallLog.calls.indexOf("deleteDoc:users/uid1/user_themes/t1");
    const parentDeleteIndex = deleteCallLog.calls.indexOf("deleteDoc:users/uid1");

    expect(entryDeleteIndex).toBeGreaterThan(-1);
    expect(themeDeleteIndex).toBeGreaterThan(-1);
    expect(parentDeleteIndex).toBeGreaterThan(-1);
    expect(entryDeleteIndex).toBeLessThan(parentDeleteIndex);
    expect(themeDeleteIndex).toBeLessThan(parentDeleteIndex);
  });

  it("deletes the user_themes subcollection specifically (regression: previously left orphaned)", async () => {
    await deleteCloudAccount();
    expect(deleteCallLog.calls).toContain("deleteDoc:users/uid1/user_themes/t1");
  });

  it("is idempotent when a subcollection is already empty", async () => {
    deleteCallLog.collections["users/uid1/entries"] = [];
    deleteCallLog.collections["users/uid1/user_themes"] = [];

    await expect(deleteCloudAccount()).resolves.toBeUndefined();
    expect(deleteCallLog.calls).toContain("deleteDoc:users/uid1");
  });
});
