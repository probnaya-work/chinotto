#!/usr/bin/env python3
"""
Every raster of the Chinotto mark, drawn from one description.

The mark is a ring with three dots receding down a column, and it has three rungs which are
NOT one drawing scaled:

    >= 40px   ring r28 stroke 2.5 (3 in the app icon) + dots r8@23, r4.5@38, r2.5@47.5
    24-39px   ring r28 stroke 3.5                     + dots r9@23, r5@40
    <= 20px   ring r27 stroke 6                       + dot  r11@27

Picking the wrong rung is the only way to get this wrong, so every output below names the
rung it wants and this file is the only place that knows what each one is.

Everything is drawn at 4x and resampled, which is both sharper than rasterising an SVG
through a thumbnailer and — more importantly — exact: no renderer sits between the geometry
and the pixels to disagree about stroke placement.

    python3 scripts/generate-identity.py

Requires Pillow, and macOS `iconutil` for the .icns.
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[1]
ICONS = ROOT / "src-tauri" / "icons"
PUBLIC = ROOT / "public"

INK = (230, 230, 227, 255)       # #e6e6e3
FIELD = (20, 20, 22, 255)        # #141416
PAPER = (242, 241, 236, 255)     # #f2f1ec
PAPER_INK = (27, 27, 29, 255)    # #1b1b1d

SS = 4  # supersampling factor


def draw_mark(size: int, rung: str, ink, app_icon: bool = False) -> Image.Image:
    """The mark alone, at `size` px, on transparency."""
    hi = size * SS
    im = Image.new("RGBA", (hi, hi), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    s = hi / 64.0  # mark units -> supersampled px

    def circle(cx, cy, r, fill=None, outline=None, width=0):
        box = ((cx - r) * s, (cy - r) * s, (cx + r) * s, (cy + r) * s)
        d.ellipse(box, fill=fill, outline=outline, width=width)

    if rung == "large":
        stroke = 3.0 if app_icon else 2.5
        circle(32, 32, 28, outline=ink, width=max(1, round(stroke * s)))
        circle(32, 23, 8, fill=ink)
        circle(32, 38, 4.5, fill=ink)
        circle(32, 47.5, 2.5, fill=ink)
    elif rung == "medium":
        circle(32, 32, 28, outline=ink, width=max(1, round(3.5 * s)))
        circle(32, 23, 9, fill=ink)
        circle(32, 40, 5, fill=ink)
    elif rung == "small":
        circle(32, 32, 27, outline=ink, width=max(1, round(6 * s)))
        circle(32, 27, 11, fill=ink)
    else:
        raise SystemExit(f"unknown rung: {rung}")

    return im.resize((size, size), Image.LANCZOS)


def rung_for(mark_px: float) -> str:
    """The ladder, in one place. Every caller asks rather than assuming."""
    if mark_px >= 40:
        return "large"
    if mark_px >= 24:
        return "medium"
    return "small"


def app_icon(size: int = 1024, light: bool = False) -> Image.Image:
    """
    The macOS app icon: the squircle drawn at 824 inside a 1024 canvas, corner radius 185,
    with the mark at 0.62 of the tile. The margin stays transparent — macOS expects the
    icon to bring its own shape, not to fill the square.

    The rung is chosen from the size the mark will actually occupy, not from the master.
    Downscaling the three-dot drawing to 16px is exactly the failure the size ladder exists
    to prevent: the ring turns to a hairline and the far dots disappear into grey. At the
    sizes Finder and Spotlight use, the icon is drawn with one dot or two.
    """
    hi = size * SS
    scale = hi / 1024.0
    im = Image.new("RGBA", (hi, hi), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)

    tile = round(824 * scale)
    off = (hi - tile) // 2
    d.rounded_rectangle(
        (off, off, off + tile - 1, off + tile - 1),
        radius=round(185 * scale),
        fill=PAPER if light else FIELD,
    )

    mark_px = round(tile * 0.62)
    rung = rung_for(size * 0.62 * (824 / 1024))
    mark = draw_mark(mark_px, rung, PAPER_INK if light else INK, app_icon=True)
    im.alpha_composite(mark, ((hi - mark_px) // 2, (hi - mark_px) // 2))
    return im.resize((size, size), Image.LANCZOS)


def square_logo(size: int, light: bool = False) -> Image.Image:
    """Windows masks its own corners, so these ship square and full-bleed."""
    hi = size * SS
    im = Image.new("RGBA", (hi, hi), PAPER if light else FIELD)
    mark_px = round(hi * 0.62)
    mark = draw_mark(mark_px, rung_for(size * 0.62), PAPER_INK if light else INK, app_icon=True)
    im.alpha_composite(mark, ((hi - mark_px) // 2, (hi - mark_px) // 2))
    return im.resize((size, size), Image.LANCZOS)


def tray_template(box_pt: int, scale: int) -> Image.Image:
    """
    The menu-bar glyph: the <=20px rung, pure black on transparency, 17pt inside a 22pt box.

    A real template image — macOS tints it with the bar, inverts it under a light bar and
    turns it white while the popover is open. It is drawn at final size: the old 1.32x
    optical upscale is gone, because a glyph that is scaled is a glyph at the wrong rung.
    """
    box = box_pt * scale
    glyph = round(17 * scale)
    im = Image.new("RGBA", (box, box), (0, 0, 0, 0))
    mark = draw_mark(glyph, "small", (0, 0, 0, 255))
    im.alpha_composite(mark, ((box - glyph) // 2, (box - glyph) // 2))
    return im


# The modifier the identity file allows the glyph, and the only one: a dot when something
# is waiting. In the drawing it sits beside the glyph, not on it — the row is
# `[glyph][gap 14][dot]` with the dot pulled back 9, so five of the fourteen survive — and
# it is 1/3 of the glyph's width.
TRAY_DOT_RATIO = 5 / 15
TRAY_DOT_GAP_RATIO = 5 / 15


def tray_template_waiting(box_pt: int, scale: int) -> Image.Image:
    """
    The same glyph with the waiting modifier, in a box widened to hold it.

    The identity file draws the dot in periwinkle. A template image is a mask — macOS keeps
    its alpha and throws its colour away — and the file is equally clear that this asset is
    a real template with no periwinkle in it, so the hue cannot survive and the dot is drawn
    in the same black as the glyph. It then tints with the bar exactly as the glyph does,
    which is what the modifier is for: the dot's presence carries the meaning.

    The box grows to the right rather than the dot moving inward, because a 22pt box holds
    a 17pt glyph with 2.5pt to spare and a badge laid over the ring at that size is not a
    dot on a mark, it is a damaged mark.
    """
    glyph = round(17 * scale)
    dot = round(17 * TRAY_DOT_RATIO * scale)
    gap = round(17 * TRAY_DOT_GAP_RATIO * scale)
    pad = round(((box_pt - 17) / 2) * scale)

    box_w = pad + glyph + gap + dot + pad
    box_h = box_pt * scale
    im = Image.new("RGBA", (box_w, box_h), (0, 0, 0, 0))

    mark = draw_mark(glyph, "small", (0, 0, 0, 255))
    top = (box_h - glyph) // 2
    im.alpha_composite(mark, (pad, top))

    # Top-aligned with the glyph, as the drawing aligns it to the row's start.
    hi = Image.new("RGBA", (dot * SS, dot * SS), (0, 0, 0, 0))
    ImageDraw.Draw(hi).ellipse((0, 0, dot * SS - 1, dot * SS - 1), fill=(0, 0, 0, 255))
    im.alpha_composite(hi.resize((dot, dot), Image.LANCZOS), (pad + glyph + gap, top))
    return im


def favicon(size: int) -> Image.Image:
    """The <=20px rung on the ink field; the browser tab is a 16px surface."""
    hi = size * SS
    im = Image.new("RGBA", (hi, hi), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    d.rounded_rectangle((0, 0, hi - 1, hi - 1), radius=round(hi * 0.22), fill=FIELD)
    glyph = round(hi * 0.66)
    im.alpha_composite(draw_mark(glyph, "small", INK), ((hi - glyph) // 2, (hi - glyph) // 2))
    return im.resize((size, size), Image.LANCZOS)


def save(im: Image.Image, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    im.save(path, "PNG", optimize=True)
    print(f"  {path.relative_to(ROOT)}")


def main() -> None:
    print("app icon (dark is the shipped default)")
    master = app_icon(1024)
    save(master, ICONS / "icon_1024.png")
    for name, px in [
        ("32x32.png", 32),
        ("64x64.png", 64),
        ("128x128.png", 128),
        ("128x128@2x.png", 256),
        ("icon.png", 512),
    ]:
        # Drawn at that size, not resampled from the master: see `app_icon`.
        save(app_icon(px), ICONS / name)

    print("app icon (light alternate)")
    save(app_icon(1024, light=True), ICONS / "icon_1024_light.png")

    print("iconset and icns")
    appset = ICONS / "macos" / "AppIcon.appiconset"
    for name, px in [
        ("icon_16x16.png", 16), ("icon_16x16@2x.png", 32),
        ("icon_32x32.png", 32), ("icon_32x32@2x.png", 64),
        ("icon_128x128.png", 128), ("icon_128x128@2x.png", 256),
        ("icon_256x256.png", 256), ("icon_256x256@2x.png", 512),
        ("icon_512x512.png", 512), ("icon_512x512@2x.png", 1024),
    ]:
        save(app_icon(px), appset / name)

    with tempfile.TemporaryDirectory() as tmp:
        iconset = Path(tmp) / "Chinotto.iconset"
        iconset.mkdir()
        for f in appset.glob("*.png"):
            (iconset / f.name).write_bytes(f.read_bytes())
        try:
            subprocess.run(
                ["iconutil", "-c", "icns", str(iconset), "-o", str(ICONS / "icon.icns")],
                check=True,
            )
            print(f"  {(ICONS / 'icon.icns').relative_to(ROOT)}")
        except (FileNotFoundError, subprocess.CalledProcessError) as e:
            print(f"  icon.icns SKIPPED ({e}) — run on macOS", file=sys.stderr)

    print("windows tiles (square, unrounded — windows masks its own)")
    for name, px in [
        ("Square30x30Logo.png", 30), ("Square44x44Logo.png", 44),
        ("Square71x71Logo.png", 71), ("Square89x89Logo.png", 89),
        ("Square107x107Logo.png", 107), ("Square142x142Logo.png", 142),
        ("Square150x150Logo.png", 150), ("Square284x284Logo.png", 284),
        ("Square310x310Logo.png", 310), ("StoreLogo.png", 50),
    ]:
        save(square_logo(px), ICONS / name)
    # One drawing with Pillow-downscaled sizes: the .ico is a Windows artefact this
    # product does not ship, and per-size artwork inside an .ico would have to be built by
    # hand. Recorded rather than pretended otherwise.
    app_icon(256).save(
        ICONS / "icon.ico", format="ICO",
        sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    print(f"  {(ICONS / 'icon.ico').relative_to(ROOT)}")

    print("menu-bar template (17pt glyph in a 22pt box, black on transparent)")
    save(tray_template(22, 1), ICONS / "tray_menu_template.png")
    save(tray_template(22, 2), ICONS / "tray_menu_template@2x.png")
    save(tray_template_waiting(22, 1), ICONS / "tray_menu_waiting_template.png")
    save(tray_template_waiting(22, 2), ICONS / "tray_menu_waiting_template@2x.png")

    print("favicons")
    save(favicon(32), PUBLIC / "favicon-32.png")
    favicon(64).save(
        PUBLIC / "favicon.ico", format="ICO", sizes=[(16, 16), (32, 32), (48, 48)]
    )
    print(f"  {(PUBLIC / 'favicon.ico').relative_to(ROOT)}")

    print("done")


if __name__ == "__main__":
    main()
