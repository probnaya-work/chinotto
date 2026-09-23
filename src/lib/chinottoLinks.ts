/** Mac App Store listing URL. This is a separate app record from iOS ("Chinotto for Mac", id6763830030) — iOS is "Chinotto", id6761345307. */
export const CHINOTTO_MAC_APP_STORE_URL =
  "https://apps.apple.com/us/app/chinotto-for-mac/id6763830030";

/** Hosted thread read URL. Copied on share create; HTML file works without hosting. */
export const CHINOTTO_SHARE_BASE_URL = "https://getchinotto.app/t";

/** Share API origin for publish/revoke (override in dev via VITE_CHINOTTO_SHARE_API_BASE). */
export const CHINOTTO_SHARE_API_BASE =
  import.meta.env.VITE_CHINOTTO_SHARE_API_BASE ?? "https://getchinotto.app";

export function shareThreadUrl(token: string): string {
  return `${CHINOTTO_SHARE_BASE_URL}/${token}`;
}
