/**
 * Minimal product analytics via Umami. Anonymous, opt-in, batched, non-blocking.
 * Never sends entry text, search queries, or identifiers. See docs/internal/analytics-design.md.
 * Uses Tauri HTTP plugin fetch to avoid CORS (webview origin not allowed by Umami).
 */

import { fetch as tauriFetch } from "@tauri-apps/plugin-http";
import { APP_VERSION } from "@/lib/appVersion";

const STORAGE_KEY = "chinotto-analytics-enabled";
export const ANALYTICS_PROMPT_SHOWN_KEY = "chinotto-analytics-prompt-shown";
const BATCH_INTERVAL_MS = 2_000;
const BATCH_SIZE_MAX = 10;
/** Browser-like UA required: Umami Cloud returns {"beep":"boop"} and does not register events when it flags the request as a bot (e.g. custom app UA). */
const USER_AGENT =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

export type SyncModalSurface = "header" | "settings" | "deeplink";

export type AnalyticsEvent =
  | { event: "entry_created"; text_length: number }
  | { event: "first_entry_created"; text_length: number }
  | { event: "entry_deleted" }
  | { event: "entry_pinned" }
  | { event: "search_used"; result_count?: number }
  | { event: "settings_opened" }
  | { event: "sync_modal_opened"; surface?: SyncModalSurface }
  | { event: "stream_showcase_opened" }
  | { event: "entry_opened" }
  | {
      event: "entry_space_changed";
      lens: "inbox" | "work" | "personal";
    }
  | { event: "resurface_shown"; age_days: number }
  | { event: "resurface_opened"; age_days: number }
  | { event: "thought_trail_opened" }
  | { event: "jump_to_date_calendar_opened" }
  | { event: "jump_to_date_completed"; days_ago: number }
  | { event: "jump_to_date_back_to_now" }
  | { event: "stream_back_to_now" }
  | {
      event: "entry_text_saved";
      source: "detail" | "stream";
      text_length: number;
    }
  | { event: "sync_apple_continue_clicked" }
  | { event: "sync_disconnect_clicked" }
  | { event: "sync_oauth_completed" }
  | { event: "sync_oauth_failed"; reason: "credential" | "oauth_bridge" | "timeout" | "window" | "start" }
  | { event: "sync_stop_sync_clicked" };

type QueuedEvent = AnalyticsEvent & { ts: string };

let umamiBaseUrl: string | null = null;
let websiteId: string | null = null;
let sessionId: string | null = null;
let queue: QueuedEvent[] = [];
let flushTimer: ReturnType<typeof setTimeout> | null = null;

function getStorage(): Storage | null {
  if (typeof localStorage === "undefined") return null;
  return localStorage;
}

function getSessionId(): string {
  if (sessionId) return sessionId;
  sessionId = crypto.randomUUID();
  return sessionId;
}

export function isOptIn(): boolean {
  const storage = getStorage();
  if (!storage) return false;
  return storage.getItem(STORAGE_KEY) === "true";
}

export function setOptIn(enabled: boolean): void {
  const storage = getStorage();
  if (storage) storage.setItem(STORAGE_KEY, enabled ? "true" : "false");
}

export function getAnalyticsPromptShown(): boolean {
  const storage = getStorage();
  if (!storage) return false;
  return storage.getItem(ANALYTICS_PROMPT_SHOWN_KEY) === "true";
}

export function setAnalyticsPromptShown(): void {
  const storage = getStorage();
  if (storage) storage.setItem(ANALYTICS_PROMPT_SHOWN_KEY, "true");
}

function isEnabled(): boolean {
  return !!umamiBaseUrl && !!websiteId && isOptIn();
}

/**
 * Configure Umami. Call once at app init.
 * baseUrl: Umami instance root (e.g. https://cloud.umami.is or https://analytics.yoursite.com).
 * id: Website ID from your Umami dashboard.
 * If either is null/empty, analytics is disabled regardless of opt-in.
 */
export function setUmami(baseUrl: string | null, id: string | null): void {
  umamiBaseUrl = baseUrl?.trim() || null;
  websiteId = id?.trim() || null;
  if (import.meta.env.DEV) {
    console.log("[analytics] setUmami called", {
      baseUrl: umamiBaseUrl ?? "(null)",
      websiteId: websiteId ? `${websiteId.slice(0, 4)}…` : "(null)",
    });
  }
}

/** @deprecated Use setUmami(baseUrl, websiteId). Kept for compatibility. */
export function setEndpoint(url: string | null): void {
  if (!url?.trim()) {
    umamiBaseUrl = null;
    websiteId = null;
    return;
  }
  const u = new URL(url);
  const path = u.pathname.replace(/\/api\/send\/?$/, "");
  umamiBaseUrl = `${u.origin}${path}`;
  const id = u.searchParams.get("website") ?? u.searchParams.get("id");
  websiteId = id ?? null;
}

function eventToData(payload: QueuedEvent): Record<string, string | number> {
  const data: Record<string, string | number> = {
    ts: payload.ts,
    app_version: APP_VERSION,
  };
  if ("text_length" in payload) data.text_length = payload.text_length;
  if ("result_count" in payload && payload.result_count !== undefined)
    data.result_count = payload.result_count;
  if ("age_days" in payload) data.age_days = payload.age_days;
  if ("days_ago" in payload) data.days_ago = payload.days_ago;
  if ("surface" in payload && payload.surface !== undefined) data.surface = payload.surface;
  if ("reason" in payload) data.reason = payload.reason;
  if ("source" in payload) data.source = payload.source;
  return data;
}

function sendToUmami(queued: QueuedEvent): void {
  if (!umamiBaseUrl || !websiteId) {
    if (import.meta.env.DEV) {
      console.log("[analytics] sendToUmami skipped: no baseUrl or websiteId");
    }
    return;
  }
  const { event } = queued;
  const data = eventToData(queued);
  const payload = {
    website: websiteId,
    hostname: "chinotto.app",
    url: "/desktop",
    title: "Chinotto",
    referrer: "",
    language: typeof navigator !== "undefined" ? navigator.language : "en",
    screen: typeof screen !== "undefined" ? `${screen.width}x${screen.height}` : "0x0",
    name: event,
    data,
    id: getSessionId(),
  };
  const body = JSON.stringify({ type: "event", payload });
  const url = `${umamiBaseUrl.replace(/\/$/, "")}/api/send`;
  const headers = {
    "Content-Type": "application/json",
    "User-Agent": USER_AGENT,
  };
  if (import.meta.env.DEV) {
    console.log("[analytics] sendToUmami full request", {
      url,
      method: "POST",
      headers,
      body: body,
    });
  }
  tauriFetch(url, {
    method: "POST",
    headers,
    body,
  })
    .then((res) => {
      res.text().then((text) => {
        if (import.meta.env.DEV) {
          console.log("[analytics] sendToUmami full response", {
            status: res.status,
            statusText: res.statusText,
            body: text,
          });
        }
      });
    })
    .catch((err) => {
      if (import.meta.env.DEV) {
        console.log("[analytics] sendToUmami fetch error", err);
      }
    });
}

function scheduleFlush(): void {
  if (flushTimer !== null) return;
  flushTimer = setTimeout(() => {
    flushTimer = null;
    flush();
  }, BATCH_INTERVAL_MS);
}

function flush(): void {
  if (!isEnabled() || queue.length === 0) return;
  const batch = queue;
  queue = [];
  for (const event of batch) {
    sendToUmami(event);
  }
}

/**
 * Local-calendar full days from a picked `YYYY-MM-DD` to today (0 = today, 1 = yesterday).
 * For `jump_to_date_completed` only; clamped ≥ 0. Does not send the string in payloads.
 */
export function jumpDateDaysAgoMetric(ymd: string): number {
  const p = ymd.split("-").map(Number);
  if (p.length !== 3 || p.some((n) => !Number.isFinite(n))) return 0;
  const [y, m, d] = p;
  const picked = new Date(y, m - 1, d).getTime();
  const now = new Date();
  const todayStart = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate()
  ).getTime();
  const diffDays = Math.round((todayStart - picked) / 86400000);
  return Math.max(0, diffDays);
}

/**
 * Record an event. Fire-and-forget; never blocks. No-op when analytics is disabled or opt-out.
 * Only call with allowed event shapes (no text, no query, no identifiers).
 */
export function track(payload: AnalyticsEvent): void {
  if (import.meta.env.DEV) {
    const enabled = isEnabled();
    console.log("[analytics] track called", {
      payload,
      isEnabled: enabled,
      reason: !enabled
        ? !umamiBaseUrl
          ? "no umamiBaseUrl (setUmami not called or null)"
          : !websiteId
            ? "no websiteId"
            : !isOptIn()
              ? "user opted out"
              : "unknown"
        : "ok",
    });
  }
  if (!isEnabled()) return;
  queue.push({ ...payload, ts: new Date().toISOString() });
  if (queue.length >= BATCH_SIZE_MAX) {
    flush();
  } else {
    scheduleFlush();
  }
}
