/**
 * Which appearance the record is drawn in, and how much bigger the text is.
 *
 * Light is a full second appearance, not a dimmed dark one — the same distances read the
 * other way round — so this only ever sets `data-theme` and lets the token layer decide
 * what that means. Nothing here knows a colour.
 *
 * "Lift contrast in bright light" is a separate, adaptive behaviour rather than a fourth
 * appearance: it raises the quiet end of the ladder without changing which appearance you
 * are in, because a sunlit screen is a problem with the room, not a preference.
 */

const THEME_KEY = "chinotto.appearance";
const SUN_KEY = "chinotto.liftContrast";
const ZOOM_KEY = "chinotto.textScale";

export type Appearance = "system" | "light" | "dark";

/** The prototype's range and step. 100% is "as designed"; everything else says so. */
export const ZOOM_MIN = 80;
export const ZOOM_MAX = 140;
export const ZOOM_STEP = 10;

function store(): Storage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    // Private windows and blocked site data both throw here rather than returning null.
    return null;
  }
}

export function readAppearance(): Appearance {
  const raw = store()?.getItem(THEME_KEY);
  return raw === "light" || raw === "dark" ? raw : "system";
}

export function readLiftContrast(): boolean {
  return store()?.getItem(SUN_KEY) === "1";
}

export function readTextScale(): number {
  const raw = Number(store()?.getItem(ZOOM_KEY));
  if (!Number.isFinite(raw) || raw <= 0) return 100;
  return clampTextScale(raw);
}

export function clampTextScale(percent: number): number {
  const stepped = Math.round(percent / ZOOM_STEP) * ZOOM_STEP;
  return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, stepped));
}

/**
 * Writes the choice to the document.
 *
 * `system` removes the attribute rather than resolving it here, so the token layer's
 * `prefers-color-scheme` query stays the single place that decides — and the window follows
 * the mac live, without this having to listen for the change.
 */
export function applyAppearance(appearance: Appearance, liftContrast: boolean): void {
  const root = document.documentElement;
  if (appearance === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", appearance);
  if (liftContrast) root.setAttribute("data-lift-contrast", "");
  else root.removeAttribute("data-lift-contrast");
}

export function applyTextScale(percent: number): void {
  // On the root font-size, so every `rem`-free px value still scales through `zoom`'s
  // simpler behaviour without a transform that would blur text.
  document.documentElement.style.setProperty("--text-scale", String(percent / 100));
}

export function writeAppearance(appearance: Appearance): void {
  store()?.setItem(THEME_KEY, appearance);
}

export function writeLiftContrast(on: boolean): void {
  store()?.setItem(SUN_KEY, on ? "1" : "0");
}

export function writeTextScale(percent: number): number {
  const clamped = clampTextScale(percent);
  store()?.setItem(ZOOM_KEY, String(clamped));
  return clamped;
}

/** "as designed" at 100%, and the number otherwise. */
export function textScaleLabel(percent: number): string {
  return percent === 100 ? "as designed" : `${percent}%`;
}
