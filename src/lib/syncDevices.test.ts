import { describe, it, expect, vi } from 'vitest';
const mockDevice = vi.hoisted(() => ({ uid: 'user', revoked: true, failed: false }));
vi.mock('firebase/auth', () => ({ getAuth: () => ({ currentUser: { uid: mockDevice.uid, isAnonymous: false } }) }));
vi.mock('./firebaseConfig', () => ({ isFirebaseSyncConfigured: () => true }));
vi.mock('./desktopFirestoreSync', () => ({ getOrInitApp: () => ({}), getOrInitFirestore: () => ({}) }));
vi.mock('./recordApi', () => ({ thisDevice: async () => ['this-mac', 'Mac'] }));
vi.mock('firebase/firestore', () => ({
  doc: () => ({}),
  getDoc: async () => {
    if (mockDevice.failed) throw new Error('offline');
    return { data: () => ({ revokedAt: mockDevice.revoked ? 'now' : null }) };
  },
}));
import { isThisDeviceKnownRevoked, isThisDeviceRevoked } from './syncDevices';

describe('Record metadata device access', () => {
  it('retains a known removal offline and scopes it to the checked account', async () => {
    expect(await isThisDeviceRevoked()).toBe(true);
    expect(isThisDeviceKnownRevoked()).toBe(true);
    mockDevice.failed = true;
    expect(await isThisDeviceRevoked()).toBe(true);
    mockDevice.uid = 'another-user';
    expect(isThisDeviceKnownRevoked()).toBe(false);
    expect(await isThisDeviceRevoked()).toBe(false);
    mockDevice.failed = false;
    mockDevice.revoked = false;
    expect(await isThisDeviceRevoked()).toBe(false);
  });
});
