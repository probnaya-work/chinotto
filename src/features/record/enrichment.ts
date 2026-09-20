/**
 * Fetching what a page calls itself.
 *
 * Runs after capture, never as part of it. Everything here is allowed to fail, and a
 * failure is recorded as a result rather than retried forever — the fragment was already
 * complete the moment the URL landed.
 *
 * Deliberately modest: a title, a canonical URL and a site name. No article extraction, no
 * readability pass, no screenshot. Those are all things the product may want later, and all
 * things that would make this path slow enough to feel like part of capture.
 */

import { fetch } from "@tauri-apps/plugin-http";
import * as api from "../../lib/recordApi";

/** Give up rather than hold a connection open on a page that will not answer. */
const TIMEOUT_MS = 8000;
/** A page that needs more than this to say its own name is not going to. */
const MAX_BYTES = 512 * 1024;

interface PageMeta {
  title: string | null;
  urlCanonical: string | null;
  siteName: string | null;
}

/**
 * Pulls metadata out of HTML with regexes rather than a parser.
 *
 * A DOM parse of an untrusted page pulls in scripts and styles we have no use for; this
 * reads four tags and ignores everything else. Anything it cannot find stays null, which is
 * the whole point — a missing title is a missing title, not a domain wearing a hat.
 */
export function parseMeta(html: string): PageMeta {
  const meta = (property: string): string | null => {
    const pattern = new RegExp(
      `<meta[^>]+(?:property|name)\\s*=\\s*["']${property}["'][^>]*>`,
      "i",
    );
    const tag = html.match(pattern)?.[0];
    if (!tag) return null;
    const content = tag.match(/content\s*=\s*["']([^"']*)["']/i)?.[1];
    return content ? decodeEntities(content.trim()) || null : null;
  };

  const ogTitle = meta("og:title");
  const rawTitle = html.match(/<title[^>]*>([\s\S]*?)<\/title>/i)?.[1];
  const title = ogTitle ?? (rawTitle ? decodeEntities(rawTitle.trim()) || null : null);

  const canonicalTag = html.match(/<link[^>]+rel\s*=\s*["']canonical["'][^>]*>/i)?.[0];
  const urlCanonical =
    canonicalTag?.match(/href\s*=\s*["']([^"']*)["']/i)?.[1]?.trim() || meta("og:url");

  return {
    title: title && title.length <= 500 ? title : title ? title.slice(0, 500) : null,
    urlCanonical: urlCanonical || null,
    siteName: meta("og:site_name"),
  };
}

function decodeEntities(s: string): string {
  return s
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&nbsp;/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

async function fetchMeta(url: string): Promise<PageMeta> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), TIMEOUT_MS);
  try {
    const response = await fetch(url, {
      method: "GET",
      signal: controller.signal,
      headers: { Accept: "text/html,application/xhtml+xml" },
    });
    if (!response.ok) throw new Error(`${response.status}`);

    const type = response.headers.get("content-type") ?? "";
    if (type && !type.includes("html")) {
      // A PDF or an image is a perfectly good thing to have kept; it just has no <title>.
      throw new Error(`not a page (${type.split(";")[0]})`);
    }

    const text = (await response.text()).slice(0, MAX_BYTES);
    return parseMeta(text);
  } finally {
    clearTimeout(timer);
  }
}

/**
 * Works through the pending queue once. Safe to call whenever the app is idle; it does
 * nothing when the queue is empty or the machine is offline.
 */
export async function runEnrichmentPass(limit = 5): Promise<number> {
  if (typeof navigator !== "undefined" && !navigator.onLine) return 0;

  let pending: [number, string][];
  try {
    pending = await api.encountersAwaitingEnrichment(limit);
  } catch {
    return 0;
  }

  let done = 0;
  for (const [encounterId, url] of pending) {
    try {
      const meta = await fetchMeta(url);
      // A page that answered but named nothing is still an answer, not a failure.
      await api.recordEnrichment(encounterId, meta, null);
    } catch (e) {
      const reason = e instanceof Error ? e.message : String(e);
      await api.recordEnrichment(encounterId, null, reason.slice(0, 200));
    }
    done += 1;
  }
  return done;
}
