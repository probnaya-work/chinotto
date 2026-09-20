import { CHINOTTO_MAC_APP_STORE_URL } from "@/lib/chinottoLinks";

/**
 * Shown at the site root on Firebase Hosting (and optional `VITE_OAUTH_BRIDGE_ORIGIN` host)
 * so visitors do not mistake the OAuth-only deploy for a web app. `/chinotto-oauth` stays full SPA.
 */
export function HostingDesktopOnly() {
  return (
    <div
      style={{
        minHeight: "100vh",
        margin: 0,
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        padding: "1.5rem",
        boxSizing: "border-box",
        background: "#141416",
        color: "#d4d3ce",
        fontFamily: "system-ui, sans-serif",
        textAlign: "center",
        lineHeight: 1.55,
        fontSize: "15px",
      }}
    >
      <p style={{ margin: "0 0 1rem", maxWidth: "24rem" }}>
        Chinotto is a Mac app; your entries stay in the local database on your Mac. This site is
        only used for a short sign-in step from the app, not a full web version.
      </p>
      <p style={{ margin: "0 0 0.75rem", maxWidth: "24rem" }}>
        <a href={CHINOTTO_MAC_APP_STORE_URL} style={{ color: "#7dd3fc", textDecoration: "underline" }}>
          App Store
        </a>
        {" · "}
        <a href="https://getchinotto.app" style={{ color: "#7dd3fc", textDecoration: "underline" }}>
          getchinotto.app
        </a>
      </p>
    </div>
  );
}
