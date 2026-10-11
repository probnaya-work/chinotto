import { useCallback, useEffect, useRef, useState } from "react";
import type { User } from "firebase/auth";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getOauthBridgeWebviewUrl, isFirebaseSyncConfigured } from "@/lib/firebaseConfig";
import {
  signInWithAppleCredential,
  signOutFirebaseSync,
  subscribeSyncAuth,
  type BridgedOAuthCredentialJson,
} from "@/lib/desktopFirestoreSync";
import { track } from "@/lib/analytics";
import {
  logOAuthDiagnostic,
  logOAuthUnknownError,
  userMessageFromCredentialApplyError,
  userMessageOAuthTimeoutMainWindow,
} from "@/lib/oauthDiagnostics";
import { IS_MAC_APP_STORE } from "@/lib/distribution";

function isTauriShell(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Tauri / plugin failures are not always `Error` with `.message`. */
function extractAppleSyncStartErrorText(e: unknown): string {
  if (e == null) {
    return "";
  }
  if (typeof e === "string") {
    return e.trim();
  }
  if (e instanceof Error && e.message.trim()) {
    return e.message.trim();
  }
  if (typeof e === "object" && "message" in e) {
    const m = (e as { message: unknown }).message;
    if (typeof m === "string" && m.trim()) {
      return m.trim();
    }
  }
  try {
    const s = JSON.stringify(e);
    if (s && s !== "{}" && s !== "null") {
      return s.length > 400 ? `${s.slice(0, 400)}…` : s;
    }
  } catch {
    /* ignore */
  }
  const s = String(e).trim();
  return s.length > 400 ? `${s.slice(0, 400)}…` : s;
}

function messageFromAppleSyncStartFailure(
  e: unknown,
  phase?: "listener" | "browser"
): string {
  const raw = extractAppleSyncStartErrorText(e);
  const prefix =
    phase === "listener" ? "Local helper: " : phase === "browser" ? "Open browser: " : "";

  if (import.meta.env.DEV) {
    return raw ? `${prefix}${raw}` : `${prefix}(no error text — see console)`;
  }

  if (/Another Sign in with Apple flow is already in progress/i.test(raw)) {
    return "A sign-in attempt is still active. Quit Chinotto fully, then try again.";
  }
  if (/Sign in with Apple timed out/i.test(raw)) {
    return "Sign in with Apple timed out. Try again.";
  }
  if (/Not allowed to open url/i.test(raw)) {
    return "The app could not open the sign-in page. Rebuild from the latest sources or check security settings.";
  }
  if (/Permission denied|operation not permitted|Address already in use/i.test(raw)) {
    return "Sign-in could not start (system blocked the local helper). Quit Chinotto fully and try again.";
  }
  if (raw && raw.length <= 160 && !raw.startsWith("{")) {
    return raw;
  }
  return "Could not start sign-in. Quit Chinotto fully and try again.";
}

const OAUTH_TIMEOUT_MS = 4 * 60 * 1000;
const CREDENTIAL_TIMEOUT_MS = 30 * 1000;

type OauthSuccessPayload = { nonce: string; credential: BridgedOAuthCredentialJson };
type OauthErrorPayload = { nonce: string; message: string };
type NativeAppleSignInResult = { idToken: string; rawNonce: string };

export function isNativeAppleSignInCancellation(e: unknown): boolean {
  const raw = extractAppleSyncStartErrorText(e);
  return /APPLE_SIGN_IN_CANCELLED|AuthorizationError[^\n]*1001|\bcancel(?:l)?ed\b/i.test(raw);
}

async function applyAppleCredentialWithDeadline(
  credential: BridgedOAuthCredentialJson,
): Promise<User> {
  let timeout: ReturnType<typeof setTimeout> | null = null;
  const deadline = new Promise<never>((_, reject) => {
    timeout = setTimeout(() => {
      reject({
        code: "auth/network-request-failed",
        message: "Firebase credential exchange timed out.",
      });
    }, CREDENTIAL_TIMEOUT_MS);
  });
  try {
    return await Promise.race([signInWithAppleCredential(credential), deadline]);
  } finally {
    if (timeout != null) clearTimeout(timeout);
  }
}

type UseAppleSyncOAuthOptions = {
  /** When false, auth subscription is inactive (saves work when modal is closed). */
  active: boolean;
};

/**
 * Apple / Firebase device sync: system browser on Firebase Hosting `/chinotto-oauth` (Apple accepts
 * https redirect_uri). Credential returns via loopback bridge POST (form navigation, not fetch — PNA).
 * Dev uses Vite localhost in the browser tab instead of Hosting.
 */
export function useAppleSyncOAuth({ active }: UseAppleSyncOAuthOptions) {
  const [user, setUser] = useState<User | null>(null);
  const [busy, setBusy] = useState(false);
  /** True while disconnect (sign-out) runs; keeps the modal in a short “Disconnecting…” state. */
  const [signingOut, setSigningOut] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inflightCleanupRef = useRef<(() => void) | null>(null);
  const attemptRef = useRef(0);

  useEffect(() => {
    if (!active || !isFirebaseSyncConfigured()) {
      return undefined;
    }
    return subscribeSyncAuth(setUser);
  }, [active]);

  useEffect(() => {
    return () => {
      inflightCleanupRef.current?.();
      inflightCleanupRef.current = null;
    };
  }, []);

  const onContinueApple = useCallback(async () => {
    const attempt = ++attemptRef.current;
    setError(null);
    if (!isTauriShell()) {
      setError("Use the Chinotto desktop app to continue.");
      return;
    }

    inflightCleanupRef.current?.();
    inflightCleanupRef.current = null;

    if (IS_MAC_APP_STORE) {
      setBusy(true);
      try {
        const native = await invoke<NativeAppleSignInResult>("native_apple_sign_in");
        if (attempt !== attemptRef.current) return;
        const signedInUser = await applyAppleCredentialWithDeadline({
          providerId: "apple.com",
          signInMethod: "apple.com",
          idToken: native.idToken,
          nonce: native.rawNonce,
        });
        if (attempt !== attemptRef.current) return;
        setUser(signedInUser);
        track({ event: "sync_oauth_completed" });
      } catch (e) {
        if (attempt !== attemptRef.current) return;
        track({ event: "sync_oauth_failed", reason: "credential" });
        setError(isNativeAppleSignInCancellation(e) ? null : userMessageFromCredentialApplyError(e));
      } finally {
        if (attempt === attemptRef.current) setBusy(false);
      }
      return;
    }

    const nonce = crypto.randomUUID();
    try {
      localStorage.setItem("chinotto_oauth_nonce", nonce);
    } catch {
      /* ignore */
    }
    setBusy(true);

    const unlisteners: UnlistenFn[] = [];
    let timeoutId: ReturnType<typeof setTimeout> | null = null;

    function cleanup() {
      if (timeoutId != null) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }
      for (const u of unlisteners) {
        try {
          u();
        } catch {
          /* ignore */
        }
      }
      unlisteners.length = 0;
      inflightCleanupRef.current = null;
    }

    inflightCleanupRef.current = cleanup;

    const unSuccess = await listen<OauthSuccessPayload>("chinotto-sync-oauth-success", async (event) => {
      if (event.payload.nonce !== nonce || attempt !== attemptRef.current) {
        return;
      }
      cleanup();
      try {
        const signedInUser = await applyAppleCredentialWithDeadline(event.payload.credential);
        if (attempt !== attemptRef.current) return;
        setUser(signedInUser);
        try {
          localStorage.removeItem("chinotto_oauth_nonce");
        } catch {
          /* ignore */
        }
        track({ event: "sync_oauth_completed" });
      } catch (e) {
        if (attempt !== attemptRef.current) return;
        track({ event: "sync_oauth_failed", reason: "credential" });
        setError(userMessageFromCredentialApplyError(e));
      } finally {
        if (attempt === attemptRef.current) setBusy(false);
      }
    });
    unlisteners.push(unSuccess);

    const unErr = await listen<OauthErrorPayload>("chinotto-sync-oauth-error", (event) => {
      if (event.payload.nonce !== nonce || attempt !== attemptRef.current) {
        return;
      }
      cleanup();
      track({ event: "sync_oauth_failed", reason: "oauth_bridge" });
      setError(event.payload.message);
      setBusy(false);
    });
    unlisteners.push(unErr);

    timeoutId = setTimeout(() => {
      cleanup();
      try {
        localStorage.removeItem("chinotto_oauth_nonce");
      } catch {
        /* ignore */
      }
      setBusy(false);
      logOAuthDiagnostic("timeout", "main_window_listen_timeout", {
        message: `no success/error event within ${OAUTH_TIMEOUT_MS}ms`,
      });
      track({ event: "sync_oauth_failed", reason: "timeout" });
      setError(userMessageOAuthTimeoutMainWindow(true));
    }, OAUTH_TIMEOUT_MS);

    try {
      const bridgeSecret = crypto.randomUUID();
      const bridgePort = await invoke<number>("start_oauth_dev_bridge_listener", {
        args: { secret: bridgeSecret },
      });
      const oauthUrl = import.meta.env.DEV
        ? new URL(window.location.href)
        : new URL(getOauthBridgeWebviewUrl(nonce));
      if (import.meta.env.DEV) {
        oauthUrl.pathname = "/chinotto-oauth";
        oauthUrl.search = "";
        oauthUrl.hash = "";
      }
      oauthUrl.searchParams.set("nonce", nonce);
      oauthUrl.searchParams.set("oauthDevBridge", "1");
      oauthUrl.searchParams.set("oauthDevBridgePort", String(bridgePort));
      oauthUrl.searchParams.set("oauthDevBridgeSecret", bridgeSecret);
      logOAuthDiagnostic(
        "config",
        import.meta.env.DEV ? "dev_browser_open" : "packaged_hosted_browser_open",
        { message: oauthUrl.origin + oauthUrl.pathname }
      );
      await openUrl(oauthUrl.toString());
    } catch (e) {
      cleanup();
      track({ event: "sync_oauth_failed", reason: "start" });
      logOAuthUnknownError("onContinueApple", e);
      console.warn("[Chinotto sync oauth] start failed", e);
      setError(messageFromAppleSyncStartFailure(e));
      setBusy(false);
    }
  }, []);

  const onCancelApple = useCallback(() => {
    ++attemptRef.current;
    inflightCleanupRef.current?.();
    inflightCleanupRef.current = null;
    setError(null);
    setBusy(false);
    try {
      localStorage.removeItem("chinotto_oauth_nonce");
    } catch {
      /* ignore */
    }

    if (IS_MAC_APP_STORE && isTauriShell()) {
      void invoke("cancel_native_apple_sign_in").catch((e) => {
        logOAuthUnknownError("cancel_native_apple_sign_in", e);
      });
    }
  }, []);

  const onSignOut = useCallback(async () => {
    setError(null);
    setSigningOut(true);
    setBusy(true);
    try {
      await signOutFirebaseSync();
      /* onAuthStateChanged can lag; update UI immediately so the modal leaves the connected state. */
      setUser(null);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
    } finally {
      setBusy(false);
      setSigningOut(false);
    }
  }, []);

  const stable = user != null && !user.isAnonymous;

  // A timed-out credential request may still complete underneath the SDK. The auth observer is
  // the source of truth, so a real signed-in user must also retire any stale retry message.
  useEffect(() => {
    if (stable) setError(null);
  }, [stable]);

  return {
    user,
    stable,
    busy,
    signingOut,
    error,
    setError,
    onContinueApple,
    onCancelApple,
    onSignOut,
  };
}
