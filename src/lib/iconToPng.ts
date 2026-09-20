/**
 * Renders the Chinotto logo (variant style) to PNG bytes for use as a window/dock icon.
 * Used only when running in Tauri; no-op in browser.
 *
 * Canvas size matches the 1024×1024 pt **large** app icon slot Apple uses in asset catalogs
 * (see Human Interface Guidelines → App icons; production templates on developer.apple.com/design/resources).
 * Passing a high-resolution image to `NSApplication.setApplicationIconImage` avoids a soft/upscaled Dock tile.
 */

import type { IconVariant } from "./iconVariants";

/** Large app icon dimension (pt/px @1x) used in Apple’s icon production workflow. */
const ICON_CANVAS_PX = 1024;

/**
 * The squircle is drawn at 824 inside the 1024 canvas with corner radius 185, and the mark
 * occupies 0.62 of that tile. Same numbers as `src-tauri/icons/icon.svg` and
 * `scripts/generate-identity.py` — this is the runtime path for switching the Dock icon, and
 * it has to produce the same drawing as the bundled one or the two disagree at a glance.
 */
const TILE_PX = 824;
const TILE_RADIUS = 185;
const MARK_OF_TILE = 0.62;

/** The >=40px rung, at the app icon's heavier ring. */
function chinottoMarkSvg(foreground: string): string {
  const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/"/g, "&quot;");
  const ink = esc(foreground);
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" fill="none">
  <circle cx="32" cy="32" r="28" stroke="${ink}" stroke-width="3" fill="none"/>
  <circle cx="32" cy="23" r="8" fill="${ink}"/>
  <circle cx="32" cy="38" r="4.5" fill="${ink}"/>
  <circle cx="32" cy="47.5" r="2.5" fill="${ink}"/>
</svg>`;
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = reject;
    img.src = src;
  });
}

function canvasToPngBytes(canvas: HTMLCanvasElement): Promise<Uint8Array> {
  return new Promise((resolve, reject) => {
    canvas.toBlob(
      (blob) => {
        if (!blob) {
          reject(new Error("toBlob failed"));
          return;
        }
        const reader = new FileReader();
        reader.onloadend = () => {
          const buf = reader.result as ArrayBuffer;
          resolve(new Uint8Array(buf));
        };
        reader.onerror = reject;
        reader.readAsArrayBuffer(blob);
      },
      "image/png",
      1
    );
  });
}

/**
 * Renders a variant to a 1024×1024 PNG for the Dock: the squircle at 824 with radius 185,
 * the mark at 0.62 of it, transparent margin. The same drawing the bundle ships, so the
 * runtime switch and the bundled icon cannot disagree.
 */
export async function variantToPngBytes(variant: IconVariant): Promise<Uint8Array> {
  const canvas = document.createElement("canvas");
  canvas.width = ICON_CANVAS_PX;
  canvas.height = ICON_CANVAS_PX;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("Canvas 2d not available");

  // The tile, inset inside the canvas: macOS expects the icon to bring its own shape and
  // leave the margin transparent, not to fill the square.
  const off = (ICON_CANVAS_PX - TILE_PX) / 2;
  ctx.beginPath();
  ctx.roundRect(off, off, TILE_PX, TILE_PX, TILE_RADIUS);
  ctx.fillStyle = variant.background;
  ctx.fill();

  const markPx = TILE_PX * MARK_OF_TILE;
  const markOff = (ICON_CANVAS_PX - markPx) / 2;
  const svg = chinottoMarkSvg(variant.foreground);
  const blob = new Blob([svg], { type: "image/svg+xml" });
  const url = URL.createObjectURL(blob);
  try {
    const img = await loadImage(url);
    ctx.drawImage(img, markOff, markOff, markPx, markPx);
    return canvasToPngBytes(canvas);
  } finally {
    URL.revokeObjectURL(url);
  }
}

