#!/usr/bin/env python3
"""
The DMG background: the lockup on the ink field.

The window someone sees once, before the product has said anything — so it says the name and
nothing else. No instructions, no arrow, no drop shadow under a fake folder: Tauri draws the
app and the Applications alias on top, and the background's only job is to be the field they
sit on.

    python3 scripts/generate-dmg-background.py
"""

from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

from importlib.machinery import SourceFileLoader

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "src-tauri" / "dmg-background.png"

identity = SourceFileLoader(
    "identity", str(Path(__file__).resolve().parent / "generate-identity.py")
).load_module()

W, H = 660, 400
FIELD = (20, 20, 22, 255)   # #141416
INK = (230, 230, 227, 255)  # #e6e6e3

# Never below 22px of wordmark, and the gap is 0.38 x its size.
WORDMARK_PX = 30
MARK_PX = 32
GAP = round(WORDMARK_PX * 0.38)

FONT_CANDIDATES = [
    ROOT / "scripts" / "Archivo-Medium.ttf",
    Path("/Library/Fonts/Archivo-Medium.ttf"),
    Path.home() / "Library" / "Fonts" / "Archivo-Medium.ttf",
]


def load_font(px: int):
    """
    Archivo, or nothing.

    The shipped background is rendered in the browser, where the real variable font is
    available; fontsource ships woff2 only, which Pillow cannot read. Falling back to
    Helvetica here would quietly replace the wordmark with a different typeface in the one
    window someone sees before the product has said anything — so this refuses instead.
    """
    for candidate in FONT_CANDIDATES:
        if candidate.exists() and candidate.suffix in {".ttf", ".ttc", ".otf"}:
            try:
                return ImageFont.truetype(str(candidate), px)
            except OSError:
                continue
    raise SystemExit(
        "No Archivo .ttf/.otf found, and the wordmark must not be set in a substitute.\n"
        "The committed src-tauri/dmg-background.png was rendered with the real font; leave\n"
        "it as it is, or drop an Archivo .ttf beside this script and re-run."
    )


def main() -> None:
    im = Image.new("RGBA", (W, H), FIELD)
    d = ImageDraw.Draw(im)

    mark = identity.draw_mark(MARK_PX, identity.rung_for(MARK_PX), INK)
    font = load_font(WORDMARK_PX)
    word = "chinotto"
    try:
        bbox = d.textbbox((0, 0), word, font=font)
        word_w, word_h = bbox[2] - bbox[0], bbox[3] - bbox[1]
    except AttributeError:
        word_w, word_h = d.textsize(word, font=font)

    total = MARK_PX + GAP + word_w
    x = (W - total) // 2
    # Above the two icons Tauri places, not behind them.
    y = 74

    im.alpha_composite(mark, (x, y - MARK_PX // 2))
    d.text((x + MARK_PX + GAP, y - word_h // 2 - 4), word, font=font, fill=INK)

    im.convert("RGB").save(OUT, "PNG", optimize=True)
    print(f"Wrote {OUT.relative_to(ROOT)} ({W}x{H})")


if __name__ == "__main__":
    main()
