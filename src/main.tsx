import "@fontsource-variable/archivo/wdth.css";
import "@fontsource/open-sauce-one/400.css";
import "@fontsource/open-sauce-one/500.css";
import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { TrayCapturePanel } from "./features/entries/TrayCapturePanel";
import { IconVariantShowcase } from "./components/IconVariantShowcase";
import { RecordApp } from "./features/record/RecordApp";
import { HostingDesktopOnly } from "./components/HostingDesktopOnly";
import { OAuthBridge } from "./components/OAuthBridge";
import { setUmami } from "./lib/analytics";
import "./index.css";

const umamiUrl = import.meta.env.VITE_UMAMI_URL ?? null;
const umamiWebsiteId = import.meta.env.VITE_UMAMI_WEBSITE_ID ?? null;
if (import.meta.env.DEV) {
  console.log("[analytics] env at init", {
    VITE_UMAMI_URL: umamiUrl ?? "(undefined)",
    VITE_UMAMI_WEBSITE_ID: umamiWebsiteId ? `${String(umamiWebsiteId).slice(0, 4)}…` : "(undefined)",
  });
}
setUmami(umamiUrl, umamiWebsiteId);

function Root() {
  const [hash, setHash] = useState(() => window.location.hash);

  useEffect(() => {
    const onHash = () => setHash(window.location.hash);
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  // Identity-asset tooling, dev only. Not part of the product.
  if (import.meta.env.DEV && hash === "#icon-variants") {
    return <IconVariantShowcase />;
  }

  // The Record is the product. There is no other home, and no route to the old one.
  return <RecordApp />;
}

/* Path-based route survives Firebase redirect better than only ?chinotto_oauth=1 (that query is often dropped). */
function isOAuthBridgeEntry(): boolean {
  if (typeof window === "undefined") {
    return false;
  }
  const path = (window.location.pathname.replace(/\/$/, "") || "/").toLowerCase();
  if (path === "/chinotto-oauth") {
    return true;
  }
  return new URLSearchParams(window.location.search).get("chinotto_oauth") === "1";
}

function isTrayCaptureSurface(): boolean {
  if (typeof window === "undefined") return false;
  return window.location.hash === "#tray-capture";
}

function isTauriShell(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Hosts where we deploy the SPA for desktop OAuth only; root should not load the full app in a browser tab. */
function isOauthBridgePublicHost(hostname: string): boolean {
  if (hostname.endsWith(".web.app") || hostname.endsWith(".firebaseapp.com")) {
    return true;
  }
  const bridge = import.meta.env.VITE_OAUTH_BRIDGE_ORIGIN?.trim();
  if (!bridge) {
    return false;
  }
  try {
    const u = new URL(bridge.replace(/\/+$/, ""));
    return u.hostname === hostname;
  } catch {
    return false;
  }
}

function shouldRenderHostingDesktopOnlyGate(): boolean {
  if (typeof window === "undefined" || isTauriShell()) {
    return false;
  }
  /* Do not rely on import.meta.env.PROD alone: some CI / predeploy paths have left users with a
   * production bundle where the flag did not gate; hostname + absence of Tauri is sufficient. */
  return isOauthBridgePublicHost(window.location.hostname);
}

const trayCapture = isTrayCaptureSurface() && isTauriShell();
const oauthChild = isOAuthBridgeEntry();
const hostingDesktopOnlyGate = shouldRenderHostingDesktopOnlyGate() && !oauthChild;

/* OAuth must not run under StrictMode: dev double-mount fires Firebase redirect twice and breaks auth. */
createRoot(document.getElementById("root")!).render(
  trayCapture ? (
    <StrictMode>
      <div className="tray-capture-root">
        <TrayCapturePanel />
      </div>
    </StrictMode>
  ) : oauthChild ? (
    <OAuthBridge />
  ) : hostingDesktopOnlyGate ? (
    <HostingDesktopOnly />
  ) : (
    <StrictMode>
      <Root />
    </StrictMode>
  )
);
