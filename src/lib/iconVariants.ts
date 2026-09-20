/**
 * The app icon's two variants.
 *
 * There were ten — violet, cyan, orange, a gradient, several greys. An icon picker is not
 * an identity, and ten variants meant the mark had no fixed colour at all. The identity
 * spec collapses them to two: dark, which is what ships, and light, which is the alternate
 * and the one the platforms derive their themed icons from.
 *
 * The geometry is not a variant. Both are the same drawing at the same rung; only the field
 * and the ink change.
 */

export type IconVariantId = "dark" | "light";

export interface IconVariant {
  id: IconVariantId;
  name: string;
  /** The mark. */
  foreground: string;
  /** The tile. */
  background: string;
}

export const ICON_VARIANTS: IconVariant[] = [
  { id: "dark", name: "dark", foreground: "#e6e6e3", background: "#141416" },
  { id: "light", name: "light", foreground: "#1b1b1d", background: "#f2f1ec" },
];

export const SELECTABLE_ICON_VARIANT_IDS: IconVariantId[] = ["dark", "light"];

const STORAGE_KEY = "chinotto.iconVariant";

export function getIconVariant(id: string | null | undefined): IconVariant {
  return ICON_VARIANTS.find((v) => v.id === id) ?? ICON_VARIANTS[0];
}

export function getStoredIconVariantId(): IconVariantId {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw === "light" ? "light" : "dark";
  } catch {
    return "dark";
  }
}

export function setStoredIconVariantId(id: string): void {
  try {
    localStorage.setItem(STORAGE_KEY, getIconVariant(id).id);
  } catch {
    // A dock icon that cannot be remembered still changes for this session.
  }
}
