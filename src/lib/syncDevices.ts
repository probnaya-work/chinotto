/**
 * Devices on this record.
 *
 * Sync knew a user and not a device, so the sync surface had nothing truthful to list and
 * `remove` had nothing to revoke. This adds a device per install, additively: a collection
 * beside the entries the legacy contract already carries, which mobile neither reads nor
 * needs. Nothing about the `{id, text, created_at}` contract changes, so a rollback to the
 * previous binary leaves the record and the phone exactly as they were.
 *
 * A removed device is revoked rather than deleted. The row has to survive so the device
 * itself can find out it was removed — deleting it would look identical to never having
 * registered, and the device would cheerfully re-register on its next heartbeat.
 */

import { getAuth } from "firebase/auth";
import {
  collection,
  deleteField,
  doc,
  getDoc,
  onSnapshot,
  serverTimestamp,
  setDoc,
  Timestamp,
} from "firebase/firestore";
import { isFirebaseSyncConfigured } from "./firebaseConfig";
import { getOrInitApp, getOrInitFirestore } from "./desktopFirestoreSync";
import { thisDevice } from "./recordApi";

export interface SyncDevice {
  id: string;
  name: string;
  platform: string;
  /** Null until the device has checked in at least once. */
  lastSeenAt: Date | null;
  revokedAt: Date | null;
  /** Whether this row is the machine you are looking at. */
  isThisDevice: boolean;
}

/** How often a device says it is still here. */
export const HEARTBEAT_MS = 60_000;

function uid(): string | null {
  if (!isFirebaseSyncConfigured()) return null;
  try {
    const user = getAuth(getOrInitApp()).currentUser;
    return user && !user.isAnonymous ? user.uid : null;
  } catch {
    return null;
  }
}

function toDate(value: unknown): Date | null {
  if (value instanceof Timestamp) return value.toDate();
  if (value instanceof Date) return value;
  return null;
}

/**
 * Registers this mac and marks it seen.
 *
 * `merge` rather than a plain write: the name is this machine's to set, but `revokedAt`
 * belongs to whichever device removed it, and a full write would erase a revocation that
 * arrived a moment earlier.
 */
export async function registerThisDevice(): Promise<void> {
  const user = uid();
  if (!user) return;
  const [id, name] = await thisDevice();
  const db = getOrInitFirestore();
  await setDoc(
    doc(db, "users", user, "devices", id),
    { name, platform: "mac", lastSeenAt: serverTimestamp() },
    { merge: true },
  );
}

/** Whether another device has removed this one from the record. */
export async function isThisDeviceRevoked(): Promise<boolean> {
  const user = uid();
  if (!user) return false;
  try {
    const [id] = await thisDevice();
    const snap = await getDoc(doc(getOrInitFirestore(), "users", user, "devices", id));
    return Boolean(snap.data()?.revokedAt);
  } catch {
    // Not being able to ask is not the same as having been removed.
    return false;
  }
}

/**
 * Watches the device list. Returns an unsubscribe, and calls back with `null` when sync is
 * not signed in — the surface then says so rather than showing an empty list, which would
 * read as "no devices" instead of "nothing to say yet".
 */
export function subscribeDevices(onChange: (devices: SyncDevice[] | null) => void): () => void {
  const user = uid();
  if (!user) {
    onChange(null);
    return () => {};
  }
  let stopped = false;
  let unsub: (() => void) | null = null;

  void thisDevice()
    .then(([selfId]) => {
      if (stopped) return;
      unsub = onSnapshot(
        collection(getOrInitFirestore(), "users", user, "devices"),
        (snap) => {
          const devices = snap.docs
            .map((d) => {
              const data = d.data();
              return {
                id: d.id,
                name: typeof data.name === "string" ? data.name : "a device",
                platform: typeof data.platform === "string" ? data.platform : "unknown",
                lastSeenAt: toDate(data.lastSeenAt),
                revokedAt: toDate(data.revokedAt),
                isThisDevice: d.id === selfId,
              } satisfies SyncDevice;
            })
            // A revoked device is not on the record any more; it is kept only so it can
            // find that out for itself.
            .filter((d) => !d.revokedAt)
            .sort((a, b) => Number(b.isThisDevice) - Number(a.isThisDevice));
          onChange(devices);
        },
        () => onChange(null),
      );
    })
    .catch(() => onChange(null));

  return () => {
    stopped = true;
    unsub?.();
  };
}

/**
 * Removes another device from the record.
 *
 * What is already on that device stays there; it just stops receiving. That is a statement
 * about what this can do, not a softening — the legacy contract has no way to reach into
 * another device's local store, and claiming otherwise would be a lie in the confirm text.
 */
export async function removeDevice(id: string): Promise<void> {
  const user = uid();
  if (!user) throw new Error("this mac is not signed in");
  await setDoc(
    doc(getOrInitFirestore(), "users", user, "devices", id),
    { revokedAt: serverTimestamp() },
    { merge: true },
  );
}

/** Undoes a revocation, for a device that is being set up again rather than removed. */
export async function restoreDevice(id: string): Promise<void> {
  const user = uid();
  if (!user) return;
  await setDoc(
    doc(getOrInitFirestore(), "users", user, "devices", id),
    { revokedAt: deleteField() },
    { merge: true },
  );
}
