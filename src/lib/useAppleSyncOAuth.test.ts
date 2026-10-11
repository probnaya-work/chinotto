import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { User } from "firebase/auth";

const oauthMocks = vi.hoisted(() => ({
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
  signIn: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (command: string) => {
    if (command === "start_oauth_dev_bridge_listener") return 49152;
    throw new Error(`unexpected command: ${command}`);
  }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, callback: (event: { payload: unknown }) => void) => {
    oauthMocks.listeners.set(event, callback);
    return () => oauthMocks.listeners.delete(event);
  }),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn(async () => {}),
}));

vi.mock("@/lib/firebaseConfig", () => ({
  getOauthBridgeWebviewUrl: () => "https://example.test/chinotto-oauth",
  isFirebaseSyncConfigured: () => true,
}));

vi.mock("@/lib/desktopFirestoreSync", () => ({
  signInWithAppleCredential: oauthMocks.signIn,
  signOutFirebaseSync: vi.fn(async () => {}),
  subscribeSyncAuth: vi.fn(() => () => {}),
}));

vi.mock("@/lib/analytics", () => ({ track: vi.fn() }));

import { isNativeAppleSignInCancellation, useAppleSyncOAuth } from "./useAppleSyncOAuth";

describe("isNativeAppleSignInCancellation", () => {
  it("recognises the explicit native cancellation sentinel", () => {
    expect(isNativeAppleSignInCancellation("APPLE_SIGN_IN_CANCELLED")).toBe(true);
  });

  it("recognises AuthenticationServices cancellation code 1001", () => {
    expect(
      isNativeAppleSignInCancellation(
        "The operation couldn’t be completed. (com.apple.AuthenticationServices.AuthorizationError error 1001.)",
      ),
    ).toBe(true);
  });

  it("does not hide a real authentication failure", () => {
    expect(isNativeAppleSignInCancellation("The network went away.")).toBe(false);
  });
});

describe("useAppleSyncOAuth browser bridge", () => {
  beforeEach(() => {
    oauthMocks.listeners.clear();
    oauthMocks.signIn.mockReset();
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });
    localStorage.clear();
  });

  it("commits the returned Firebase user before leaving the connecting state", async () => {
    const firebaseUser = { uid: "apple-user", isAnonymous: false } as User;
    oauthMocks.signIn.mockResolvedValue(firebaseUser);
    const hook = renderHook(() => useAppleSyncOAuth({ active: true }));

    await act(async () => {
      await hook.result.current.onContinueApple();
    });
    expect(hook.result.current.busy).toBe(true);

    const nonce = localStorage.getItem("chinotto_oauth_nonce");
    expect(nonce).toBeTruthy();
    await act(async () => {
      oauthMocks.listeners.get("chinotto-sync-oauth-success")?.({
        payload: {
          nonce,
          credential: {
            providerId: "apple.com",
            signInMethod: "apple.com",
            idToken: "token",
          },
        },
      });
    });

    await waitFor(() => {
      expect(hook.result.current.busy).toBe(false);
      expect(hook.result.current.stable).toBe(true);
      expect(hook.result.current.user).toBe(firebaseUser);
    });
    expect(localStorage.getItem("chinotto_oauth_nonce")).toBeNull();
  });
});
