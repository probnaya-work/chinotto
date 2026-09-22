export type DistributionChannel = "direct" | "mas";

/**
 * Build-time distribution boundary. Direct distribution remains the default so ordinary
 * development and GitHub releases cannot accidentally inherit App Store behavior.
 */
export const DISTRIBUTION_CHANNEL: DistributionChannel =
  import.meta.env.VITE_DISTRIBUTION_CHANNEL === "mas" ? "mas" : "direct";

export const IS_MAC_APP_STORE = DISTRIBUTION_CHANNEL === "mas";
