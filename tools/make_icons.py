"""The app's icons, drawn from scratch: a sunset over a ridge, in a gold ring, on the
game's night blue. Writes crates/game/assets/icon-192.png, icon-512.png and
icon-maskable-512.png (its emblem inside the safe zone, the background to the edges).

    python tools/make_icons.py
"""
import math
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

OUT = Path(__file__).resolve().parent.parent / "crates" / "game" / "assets"
S = 4  # drawn this many times larger, then scaled down: smooth edges


def lerp(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def gradient(size, top, bottom):
    im = Image.new("RGB", (size, size))
    d = ImageDraw.Draw(im)
    for y in range(size):
        d.line([(0, y), (size, y)], fill=lerp(top, bottom, y / (size - 1)))
    return im


def emblem(size):
    """The ring, the sun and the ridge, on a transparent square `size` wide."""
    im = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    c = size / 2
    r = size * 0.40
    # the sky in the ring: sunset colours, a glow behind the sun
    sky = gradient(size, (255, 196, 92), (232, 74, 76)).convert("RGBA")
    disc = Image.new("L", (size, size), 0)
    ImageDraw.Draw(disc).ellipse([c - r, c - r, c + r, c + r], fill=255)
    im.paste(sky, (0, 0), disc)
    d = ImageDraw.Draw(im)
    # the sun, low, half behind the ridge
    sun_r = r * 0.42
    sy = c + r * 0.08
    glow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    ImageDraw.Draw(glow).ellipse([c - sun_r * 1.5, sy - sun_r * 1.5, c + sun_r * 1.5, sy + sun_r * 1.5], fill=(255, 240, 180, 120))
    glow = glow.filter(ImageFilter.GaussianBlur(size * 0.03))
    im.alpha_composite(Image.composite(glow, Image.new("RGBA", (size, size), (0, 0, 0, 0)), disc))
    d.ellipse([c - sun_r, sy - sun_r, c + sun_r, sy + sun_r], fill=(255, 246, 214, 255))
    # the ridge: peaks across the lower half, a nearer, darker one in front
    def ridge(points, colour):
        layer = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        poly = [(c + x * r, c + y * r) for x, y in points] + [(c + r, c + r * 1.2), (c - r, c + r * 1.2)]
        ImageDraw.Draw(layer).polygon(poly, fill=colour)
        im.alpha_composite(Image.composite(layer, Image.new("RGBA", (size, size), (0, 0, 0, 0)), disc))
    ridge([(-1.0, 0.30), (-0.62, 0.02), (-0.38, 0.22), (-0.05, -0.20), (0.22, 0.12), (0.48, -0.04), (0.78, 0.24), (1.0, 0.16)], (64, 34, 84, 255))
    ridge([(-1.0, 0.52), (-0.55, 0.30), (-0.20, 0.46), (0.18, 0.28), (0.55, 0.50), (1.0, 0.38)], (26, 18, 46, 255))
    # a star over the peak: four points
    sx, sy2, k = c - r * 0.05, c - r * 0.55, r * 0.16
    star = []
    for i in range(8):
        a = math.pi / 4 * i - math.pi / 2
        rad = k if i % 2 == 0 else k * 0.28
        star.append((sx + math.cos(a) * rad, sy2 + math.sin(a) * rad))
    d.polygon(star, fill=(255, 252, 236, 255))
    # the ring
    w = size * 0.045
    d.ellipse([c - r - w / 2, c - r - w / 2, c + r + w / 2, c + r + w / 2], outline=(240, 196, 92, 255), width=round(w))
    d.ellipse([c - r - w * 0.9, c - r - w * 0.9, c + r + w * 0.9, c + r + w * 0.9], outline=(150, 108, 40, 255), width=max(1, round(w * 0.18)))
    return im


def icon(px, maskable):
    size = px * S
    bg = gradient(size, (28, 38, 66), (12, 17, 28)).convert("RGBA")
    if not maskable:
        # rounded corners
        mask = Image.new("L", (size, size), 0)
        ImageDraw.Draw(mask).rounded_rectangle([0, 0, size - 1, size - 1], radius=size * 0.18, fill=255)
        bg.putalpha(mask)
    # maskable: the emblem within the middle 70% (the safe zone is a circle of 80%)
    scale = 0.70 if maskable else 0.92
    e = emblem(round(size * scale))
    off = (size - e.width) // 2
    bg.alpha_composite(e, (off, off))
    out = bg.resize((px, px), Image.LANCZOS)
    return out if not maskable else out.convert("RGB")


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    for px in (192, 512):
        icon(px, False).save(OUT / f"icon-{px}.png", optimize=True)
        print(OUT / f"icon-{px}.png")
    icon(512, True).save(OUT / "icon-maskable-512.png", optimize=True)
    print(OUT / "icon-maskable-512.png")
