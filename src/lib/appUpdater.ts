import { useState, useEffect, useRef, useCallback } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { IS_MAC_APP_STORE } from "@/lib/distribution";

export type AppUpdaterPhase =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "ready"
  | "error";

function logUpdater(context: string, err: unknown) {
  console.warn(`[updater] ${context}`, err);
}

export function useAppUpdater() {
  const [phase, setPhase] = useState<AppUpdaterPhase>("idle");
  const [version, setVersion] = useState<string | null>(null);
  const updateRef = useRef<Update | null>(null);

  useEffect(() => {
    if (!import.meta.env.PROD || IS_MAC_APP_STORE) {
      return;
    }

    let cancelled = false;
    setPhase("checking");

    (async () => {
      try {
        const update = await check();
        if (cancelled) {
          return;
        }
        if (!update) {
          setPhase("idle");
          return;
        }
        updateRef.current = update;
        setVersion(update.version);
        setPhase("available");
      } catch (e) {
        logUpdater("check failed", e);
        if (!cancelled) {
          setPhase("idle");
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, []);

  const download = useCallback(async () => {
    if (IS_MAC_APP_STORE) {
      return;
    }
    const update = updateRef.current;
    if (!update) {
      return;
    }
    setPhase("downloading");
    try {
      await update.download();
      setPhase("ready");
    } catch (e) {
      logUpdater("download failed", e);
      setPhase("error");
    }
  }, []);

  const installAndRestart = useCallback(async () => {
    if (IS_MAC_APP_STORE) {
      return;
    }
    const update = updateRef.current;
    if (!update) {
      return;
    }
    try {
      await update.install();
      await relaunch();
    } catch (e) {
      logUpdater("install or relaunch failed", e);
      setPhase("error");
    }
  }, []);

  const retryAfterError = useCallback(() => {
    if (updateRef.current) {
      setPhase("available");
    } else {
      setPhase("idle");
    }
  }, []);

  return {
    phase,
    version,
    download,
    installAndRestart,
    retryAfterError,
  };
}
