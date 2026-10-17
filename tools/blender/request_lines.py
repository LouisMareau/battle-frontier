"""Lines and profiles for model requests (requests.md, "Lines and profiles"): the lines a run
traces on each picture, and the front and side it draws from them, kept in the request's
lines.json; drawn over the pictures (lines_<picture>.png: for the run, and for the user to see
how well they sit, with how far each outline and the picture's own silhouette overlap, where
they part tinted), as a front and a side in cells (profiles.png), and after each render over
the model (fit_front.png, fit_side.png, match_<picture>.png, which compare.png gets as a row
more), with how far the outlines and the model's silhouette overlap.

    python tools/blender/model_requests.py lines ID

lines.json:

    {"pictures": {"front.png": {"view": "front", "camera": {"turn": 0, "tilt": 0},
                                "lines": [{"name": "outline", "points": [[x, y], ...], "closed": true, "smooth": true},
                                          {"name": "eye", "ellipse": [x, y, rx, ry], "angle": 0},
                                          {"name": "horn tip", "mark": [x, y]}]}},
     "front": {"lines": [...]},
     "side": {"lines": [...]}}

A picture's lines are in its own pixels (x right, y down); the front and the side are drawn
the same way, at about the pictures' scale, standing on their lowest point (or "ground": y),
centred on their middle (or "centre": x): the front as seen from the front, the side facing
left (or "faces": "right" when it's drawn the other way). Plain Python: drawing needs Pillow,
`profiles` (which request_kit.py reads in Blender) doesn't.
"""
import json
import math
from pathlib import Path

import model_requests as mr

FILE = "lines.json"
VIEWS = ("front", "side")
# a pen for each line's name, the same in every drawing: the first names first
PENS = ("#00e5ff", "#ff3df0", "#ffe600", "#7dff3a", "#ff8a00", "#4d8bff", "#ff4d6a", "#b77bff", "#00ffa8", "#f5f5f5", "#c8ff00", "#ff9ecb")
HALO = (20, 20, 24, 210)
# what the editor shows, in order (lines_* and match_* once per picture)
SHOWN = ("lines_{}", "profiles", "fit_front", "fit_side", "match_{}")
# a picture's silhouette (outline_score) is measured at most this many pixels across
SCORED = 600
# how far off the background's colour a pixel may be and still be background (in the channel
# most off): at least the first (a JPEG's noise round the lines), more when the box's edges vary
# more (a soft gradient), at most the second
NEAR = (28, 40)
# the background's shadow on the ground: a darker tone of it, between these shares of its
# lightness, each channel at most this far off that tone (a tint)
SHADOW = (0.35, 0.99, 24)
# where an outline and its picture part, tinted on lines_<picture>.png: the silhouette past the
# outline (the line to move out), the outline past the silhouette (to move in), how strongly
PAST, SHORT, TINT = (255, 40, 40), (40, 120, 255), 0.45


class LinesError(ValueError):
    """What's wrong with a lines.json, all of it at once."""


def path(root, rid):
    return mr.folder(root, rid) / FILE


# ---------------------------------------------------------------- reading it, checked

def _num(v):
    return isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v)


def _point(v):
    return isinstance(v, (list, tuple)) and len(v) == 2 and all(_num(c) for c in v)


def _check_line(item, where):
    if not isinstance(item, dict):
        return [f"{where}: a line is {{\"name\": ..., \"points\": [[x, y], ...]}}, an \"ellipse\" or a \"mark\""]
    name = item.get("name")
    out = []
    if not isinstance(name, str) or not name.strip():
        out.append(f"{where}: it needs a \"name\"")
    else:
        where = f"{where} ({name})"
    kinds = [k for k in ("points", "ellipse", "mark") if k in item]
    if len(kinds) != 1:
        return out + [f"{where}: give it one of \"points\", \"ellipse\" or \"mark\""]
    v = item[kinds[0]]
    if kinds[0] == "points":
        if not isinstance(v, list) or not all(_point(p) for p in v):
            out.append(f"{where}: \"points\" is a list of [x, y]")
        elif len(v) < (3 if item.get("closed") else 2):
            out.append(f"{where}: {len(v)} point{'' if len(v) == 1 else 's'} is too few for a{' closed' if item.get('closed') else ''} line")
    elif kinds[0] == "ellipse":
        if not (isinstance(v, list) and len(v) == 4 and all(_num(c) for c in v) and v[2] > 0 and v[3] > 0):
            out.append(f"{where}: an \"ellipse\" is [x, y, its half width, its half height]")
    elif not _point(v):
        out.append(f"{where}: a \"mark\" is [x, y]")
    for flag in ("closed", "smooth"):
        if flag in item and not isinstance(item[flag], bool):
            out.append(f"{where}: \"{flag}\" is true or false")
    if "angle" in item and not _num(item["angle"]):
        out.append(f"{where}: \"angle\" is in degrees")
    return out


def _check_lines(part, where):
    lines = part.get("lines", [])
    if not isinstance(lines, list):
        return [f"{where}.lines: a list of lines"]
    return [p for i, item in enumerate(lines) for p in _check_line(item, f"{where}.lines[{i}]")]


def check(data, images):
    """Everything wrong with lines.json's data, said plainly (nothing: it's right)."""
    if not isinstance(data, dict):
        return ["it's {\"pictures\": {...}, \"front\": {...}, \"side\": {...}}"]
    out = [f"\"{k}\": lines.json has \"pictures\", \"front\" and \"side\"" for k in data if k not in ("pictures", *VIEWS)]
    pictures = data.get("pictures", {})
    if not isinstance(pictures, dict):
        out.append("pictures: {\"PICTURE\": {\"view\": ..., \"camera\": ..., \"lines\": [...]}, ...}")
        pictures = {}
    for name, pic in pictures.items():
        where = f"pictures.{name}"
        if name not in images:
            out.append(f"{where}: not one of the request's pictures ({', '.join(images) or 'none'})")
        if not isinstance(pic, dict):
            out.append(f"{where}: {{\"view\": ..., \"camera\": ..., \"lines\": [...]}}")
            continue
        cam = pic.get("camera")
        if cam is not None and not (isinstance(cam, dict) and all(_num(cam.get(k, 0)) for k in ("turn", "tilt"))):
            out.append(f"{where}.camera: {{\"turn\": degrees, \"tilt\": degrees}}")
        out += _check_lines(pic, where)
    for view in VIEWS:
        if view not in data:
            continue
        part = data[view]
        if not isinstance(part, dict):
            out.append(f"{view}: {{\"lines\": [...]}}")
            continue
        out += [f"{view}.{k}: a number (pixels)" for k in ("centre", "ground") if k in part and not _num(part[k])]
        if part.get("faces", "left") not in ("left", "right"):
            out.append(f"{view}.faces: \"left\" or \"right\" (which way its front is)")
        out += _check_lines(part, view)
    return out


def read(root, rid):
    """lines.json, checked (LinesError says all that's wrong); {} when there's none yet."""
    p = path(root, rid)
    if not p.is_file():
        return {}
    try:
        data = json.loads(p.read_text(encoding="utf-8"))
    except ValueError as e:
        raise LinesError(f"{p.name} isn't JSON: {e}")
    problems = check(data, mr.load(root, rid)["images"])
    if problems:
        raise LinesError(f"{p.name}:\n" + "\n".join(f"  {x}" for x in problems))
    return data


# ---------------------------------------------------------------- the lines' shapes

def _smooth(points, closed, steps=8):
    """A curve through the points (Catmull-Rom)."""
    n = len(points)
    if n < 3:
        return list(points)
    out = []
    for i in range(n if closed else n - 1):
        p0 = points[(i - 1) % n] if closed else points[max(i - 1, 0)]
        p1, p2 = points[i], points[(i + 1) % n]
        p3 = points[(i + 2) % n] if closed else points[min(i + 2, n - 1)]
        for s in range(steps):
            t = s / steps
            out.append(tuple(0.5 * (2 * b + (c - a) * t + (2 * a - 5 * b + 4 * c - d) * t * t + (3 * b - a - 3 * c + d) * t ** 3)
                             for a, b, c, d in zip(p0, p1, p2, p3)))
    if not closed:
        out.append(tuple(points[-1]))
    return out


def shape(item):
    """A line as it's drawn: its points (a curve through them when smooth), an ellipse's
    outline, a mark's one point."""
    if "mark" in item:
        return [tuple(item["mark"])]
    if "ellipse" in item:
        x, y, rx, ry = item["ellipse"]
        a = math.radians(item.get("angle", 0.0))
        out = []
        for k in range(48):
            t = math.tau * k / 48
            ex, ey = rx * math.cos(t), ry * math.sin(t)
            out.append((x + ex * math.cos(a) - ey * math.sin(a), y + ex * math.sin(a) + ey * math.cos(a)))
        return out
    pts = [tuple(p) for p in item["points"]]
    return _smooth(pts, bool(item.get("closed"))) if item.get("smooth") else pts


def is_closed(item):
    return "ellipse" in item or ("points" in item and bool(item.get("closed")))


def outline(part):
    """The part's line named "outline", if it's a closed one."""
    return next((item for item in part.get("lines", []) if item.get("name") == "outline" and is_closed(item)), None)


def _box(points):
    xs, ys = [p[0] for p in points], [p[1] for p in points]
    return min(xs), min(ys), max(xs), max(ys)


def around(items, size):
    """The part of a picture its lines are in, with room round them (the whole picture when
    they cover most of it), in whole pixels."""
    w, h = size
    x0, y0, x1, y1 = _box([p for item in items for p in shape(item)])
    m = max(30.0, 0.3 * max(x1 - x0, y1 - y0))
    box = (max(0, int(x0 - m)), max(0, int(y0 - m)), min(w, int(math.ceil(x1 + m))), min(h, int(math.ceil(y1 + m))))
    if box[2] - box[0] < 2 or box[3] - box[1] < 2 or (box[2] - box[0]) * (box[3] - box[1]) > 0.7 * w * h:
        return 0, 0, w, h
    return box


# ---------------------------------------------------------------- the front and the side, in cells

def in_cells(part, height):
    """One view's lines in cells, as the model stands: [(name, [(u, z), ...])], how wide (or
    long) it is, and how many cells a pixel is. u is across the view (x for the front, y for
    the side), z up from its ground; the view stands `height` tall, centred across; a side
    drawn facing right ("faces": "right") is turned to face left."""
    pts = [p for item in part["lines"] for p in shape(item)]
    x0, top, x1, low = _box(pts)
    ground = float(part.get("ground", low))
    if ground - top <= 1e-9:
        raise LinesError("a view with no height: its lines are all on one level")
    centre = float(part.get("centre", (x0 + x1) / 2))
    s = height / (ground - top)
    flip = -1.0 if part.get("faces") == "right" else 1.0
    lines = [(item["name"], [(round((x - centre) * s * flip, 5), round((ground - y) * s, 5)) for x, y in shape(item)]) for item in part["lines"]]
    return lines, (x1 - x0) * s, s


# what's at a creature's front, by its line's name: a side whose front is on the other side
# than it says faces the wrong way
FRONT_WORDS = ("head", "face", "eye", "nose", "snout", "beak", "mouth", "muzzle", "jaw", "mandible")


def facing_wrong(part):
    """A side whose head-like lines are on its back half: it faces the other way than it says."""
    pts = [p for item in part["lines"] for p in shape(item)]
    x0, _, x1, _ = _box(pts)
    front = [p for item in part["lines"] if any(w in item["name"].lower() for w in FRONT_WORDS) for p in shape(item)]
    if not front or x1 - x0 < 1e-9:
        return False
    mid = sum(p[0] for p in front) / len(front)
    on_right = mid > (x0 + x1) / 2 + 0.08 * (x1 - x0)
    on_left = mid < (x0 + x1) / 2 - 0.08 * (x1 - x0)
    return on_right if part.get("faces", "left") == "left" else on_left


def profiles(root, rid, data=None, height=None):
    """The front and the side in cells, as the model stands: {"front": {name: [(x, z), ...]},
    "side": {name: [(y, z), ...]}}. Each view stands the request's height from its ground (z
    0), centred across; the front as seen from the front (x to the right: the model's left),
    the side as seen from the model's left (its front toward -y, on the left). An ellipse comes
    as its outline, a mark as one point; a name used again gets " 2", " 3"... Empty when
    lines.json has neither yet."""
    data = read(root, rid) if data is None else data
    height = float(mr.load(root, rid)["height"]) if height is None else float(height)
    out = {}
    for view in VIEWS:
        part = data.get(view)
        if not part or not part.get("lines"):
            continue
        named = {}
        for name, pts in in_cells(part, height)[0]:
            key, n = name, 2
            while key in named:
                key, n = f"{name} {n}", n + 1
            named[key] = pts
        out[view] = named
    return out


# ---------------------------------------------------------------- drawing (Pillow)

def pens(data, images):
    """A colour for each line's name, the same in every drawing."""
    names = [item["name"] for pic in images for item in data.get("pictures", {}).get(pic, {}).get("lines", [])]
    names += [item["name"] for view in VIEWS for item in data.get(view, {}).get("lines", [])]
    out = {}
    for name in names:
        out.setdefault(name, PENS[len(out) % len(PENS)])
    return out


def _font(size):
    from PIL import ImageFont
    try:
        return ImageFont.load_default(size=size)
    except TypeError:
        return ImageFont.load_default()


def _nice(span, divisions=10):
    """A round step dividing `span` into about `divisions`."""
    raw = max(span, 1e-9) / divisions
    mag = 10 ** math.floor(math.log10(raw))
    return next(m * mag for m in (1, 2, 2.5, 5, 10) if m * mag >= raw)


def _rgb(hexa):
    return tuple(int(hexa[i:i + 2], 16) for i in (1, 3, 5))


def _ink(layer, items, to_px, colours, width, labels=True, font=None):
    """Lines on a see-through layer (Pillow): their halos first, then the lines, then their names."""
    from PIL import ImageDraw
    d = ImageDraw.Draw(layer)
    drawn = []
    for item in items:
        pts = [to_px(p) for p in shape(item)]
        if is_closed(item):
            pts = pts + [pts[0]]
        drawn.append((item, pts, _rgb(colours.get(item["name"], PENS[0]))))
    for halo in (True, False):
        for item, pts, colour in drawn:
            w = width + (width if halo else 0)
            fill = HALO if halo else colour + (255,)
            if "mark" in item:
                (x, y), r = pts[0], width * 2.6
                d.line([(x - r, y), (x + r, y)], fill=fill, width=w)
                d.line([(x, y - r), (x, y + r)], fill=fill, width=w)
            else:
                d.line(pts, fill=fill, width=w, joint="curve")
    if labels and font is not None:
        for item, pts, colour in drawn:
            x, y = min(pts, key=lambda p: p[1])
            d.text((x + width * 2, y - width * 2), item["name"], fill=colour + (255,), font=font, anchor="ls", stroke_width=max(2, width // 2), stroke_fill=HALO)


def draw_picture(img, box, k, items, colours, grid=True, labels=True, under=None):
    """A box of a picture `k` times as big with lines on it (in the picture's pixels), and a
    grid in the picture's own pixels, its numbers along the top and the left. `under`: (a
    see-through layer, the box of the picture it covers, stretched to it), laid on the picture
    under the lines."""
    from PIL import Image, ImageDraw
    x0, y0, x1, y1 = box
    w, h = max(1, round((x1 - x0) * k)), max(1, round((y1 - y0) * k))
    margin = 36 if grid else 0
    canvas = Image.new("RGBA", (w + margin, h + margin), (40, 40, 44, 255))
    canvas.paste(img.convert("RGBA").crop(box).resize((w, h), Image.LANCZOS), (margin, margin))
    if under is not None:
        layer, (ux0, uy0, ux1, uy1) = under
        over = Image.new("RGBA", (w, h), (0, 0, 0, 0))
        over.paste(layer.resize((max(1, round((ux1 - ux0) * k)), max(1, round((uy1 - uy0) * k))), Image.BILINEAR),
                   (round((ux0 - x0) * k), round((uy0 - y0) * k)))
        canvas.alpha_composite(over, (margin, margin))
    ss = 2
    layer = Image.new("RGBA", (canvas.width * ss, canvas.height * ss), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    step = _nice(max(x1 - x0, y1 - y0)) if grid else 0
    if grid:
        g = x0 - x0 % step + (step if x0 % step else 0)
        while g <= x1:
            X = (margin + (g - x0) * k) * ss
            d.line([(X, margin * ss), (X, canvas.height * ss)], fill=(30, 30, 30, 70), width=ss)
            g += step
        g = y0 - y0 % step + (step if y0 % step else 0)
        while g <= y1:
            Y = (margin + (g - y0) * k) * ss
            d.line([(margin * ss, Y), (canvas.width * ss, Y)], fill=(30, 30, 30, 70), width=ss)
            g += step
    _ink(layer, items, lambda p: ((margin + (p[0] - x0) * k) * ss, (margin + (p[1] - y0) * k) * ss), colours, 3 * ss, labels, _font(14 * ss))
    out = Image.alpha_composite(canvas, layer.resize(canvas.size, Image.LANCZOS))
    if grid:
        d, font = ImageDraw.Draw(out), _font(12)
        num = (lambda v: f"{v:g}")
        g = x0 - x0 % step + (step if x0 % step else 0)
        while g <= x1:
            d.text((margin + (g - x0) * k, margin / 2), num(g), fill=(225, 225, 225, 255), font=font, anchor="mm")
            g += step
        g = y0 - y0 % step + (step if y0 % step else 0)
        while g <= y1:
            d.text((margin / 2, margin + (g - y0) * k), num(g), fill=(225, 225, 225, 255), font=font, anchor="mm")
            g += step
    return out.convert("RGB")


def _redrawn(lines, originals):
    """Lines moved to another space (in_cells' [(name, points)]) as lines to draw again."""
    return [{"name": name, "mark": list(pts[0])} if "mark" in orig else {"name": name, "points": [list(p) for p in pts], "closed": is_closed(orig)}
            for (name, pts), orig in zip(lines, originals)]


def draw_profiles(data, height, colours):
    """The front and the side side by side, in cells: a grid, the ground, the request's height,
    the middle; None when lines.json has neither yet."""
    from PIL import Image, ImageDraw
    views = [(v, in_cells(data[v], height)) for v in VIEWS if data.get(v, {}).get("lines")]
    if not views:
        return None
    ppc, pad, title = 520.0 / height, 0.14 * height, 44
    panels = []
    for view, (lines, across, _) in views:
        us = [p[0] for _, pts in lines for p in pts]
        u0, u1 = min(us + [0.0]) - pad, max(us + [0.0]) + pad
        z0, z1 = -pad, height + pad
        w, h = round((u1 - u0) * ppc), round((z1 - z0) * ppc)
        ss = 2
        panel = Image.new("RGBA", (w, h + title), (246, 245, 241, 255))
        layer = Image.new("RGBA", (w * ss, (h + title) * ss), (0, 0, 0, 0))
        d = ImageDraw.Draw(layer)
        to_px = (lambda p, u0=u0, z1=z1: ((p[0] - u0) * ppc * ss, (title + (z1 - p[1]) * ppc) * ss))
        step = _nice(height, 6)
        g = math.ceil(u0 / step) * step
        while g <= u1:
            d.line([to_px((g, z0)), to_px((g, z1))], fill=(0, 0, 0, 28), width=ss)
            g += step
        g = math.ceil(z0 / step) * step
        while g <= z1:
            d.line([to_px((u0, g)), to_px((u1, g))], fill=(0, 0, 0, 28), width=ss)
            g += step
        d.line([to_px((u0, 0)), to_px((u1, 0))], fill=(90, 80, 60, 255), width=2 * ss)
        dash = step / 4
        u = u0
        while u < u1:
            d.line([to_px((u, height)), to_px((min(u + dash * 0.6, u1), height))], fill=(200, 60, 60, 255), width=2 * ss)
            u += dash
        z = z0
        while z < z1:
            d.line([to_px((0, z)), to_px((0, min(z + dash * 0.6, z1)))], fill=(60, 60, 200, 200), width=ss)
            z += dash
        _ink(layer, _redrawn(lines, data[view]["lines"]), to_px, colours, 3 * ss, True, _font(14 * ss))
        panel = Image.alpha_composite(panel, layer.resize(panel.size, Image.LANCZOS))
        d = ImageDraw.Draw(panel)
        font, small = _font(16), _font(12)
        name = "Front: as seen from the front" if view == "front" else "Side: seen from its left, facing left"
        d.text((10, 8), name, fill=(30, 30, 30, 255), font=font)
        d.text((10, 28), f"{across:.2f} {'wide' if view == 'front' else 'long'} x {height:.2f} tall (cells); the dashed line: its height", fill=(90, 90, 90, 255), font=small)
        g = math.ceil(z0 / step) * step
        while g <= z1:
            x, y = to_px((u0, g))
            d.text((x / ss + 4, y / ss - 2), f"{g:.2f}".rstrip("0").rstrip(".") if g else "0", fill=(110, 110, 110, 255), font=small, anchor="ls")
            g += step
        panels.append(panel)
    gap = 16
    sheet = Image.new("RGB", (sum(p.width for p in panels) + gap * (len(panels) + 1), max(p.height for p in panels) + 2 * gap), (222, 220, 214))
    x = gap
    for p in panels:
        sheet.paste(p.convert("RGB"), (x, gap))
        x += p.width + gap
    return sheet


# ---------------------------------------------------------------- the lines command

def shown(root, rid):
    """The drawings the editor shows for the request, those there are, in order."""
    d = mr.folder(root, rid)
    stems = [Path(p).stem for p in mr.load(root, rid)["images"]]
    names = [n.format(s) for n in SHOWN for s in (stems if "{}" in n else [None])]
    return [f"{n}.png" for n in names if (d / f"{n}.png").is_file()]


def lines(root, rid):
    """lines.json drawn: each traced picture's lines over it (lines_<picture>.png, with how far
    its outline and the picture's own silhouette overlap), the front and the side in cells
    (profiles.png); the request lists them (the editor shows them). Returns what to say: what
    there is, each outline's overlap, what's missing."""
    mr._pillow("lines")
    from PIL import Image
    record = mr.load(root, rid)
    if not path(root, rid).is_file():
        raise LinesError(f"no {FILE} in private/requests/{rid}/ yet: write it first (requests.md, \"Lines and profiles\")")
    data = read(root, rid)
    d = mr.folder(root, rid)
    colours = pens(data, record["images"])
    said, missing = [], []
    for pic in record["images"]:
        part = data.get("pictures", {}).get(pic)
        if not part or not part.get("lines"):
            missing.append(f"{pic} has no lines yet")
            continue
        img = Image.open(d / pic)
        box = around(part["lines"], img.size)
        k = max(1.0, min(6.0, 1100.0 / max(box[2] - box[0], box[3] - box[1])))
        out = d / f"lines_{Path(pic).stem}.png"
        line = outline(part)
        found, drawn, at, why = _masks(img, line) if line else (None, None, None, "no outline to score (a closed line named \"outline\")")
        o = _overlaps(found, drawn) if found is not None else None
        # where they part, tinted under the lines; the caption's squares say which is which
        drawing = draw_picture(img, box, k, part["lines"], colours, under=(_tint(found, drawn), at) if o else None)
        _caption(drawing, _short(f"the outline on {pic}" if line else pic, o, "the silhouette", f"can't be scored: {why}" if line else why, (PAST, SHORT)),
                 below=True).save(out)
        cam = part.get("camera")
        seen = f"camera turn {cam.get('turn', 0):g}, tilt {cam.get('tilt', 0):g}" if cam else "no camera"
        if o:
            score = "; " + _overlap_text(o, "the picture's silhouette", "the silhouette")
        else:
            score = f"; the outline can't be scored: {why} (judge it by eye)" if line else ""
        said.append(f"{out.name}: {len(part['lines'])} lines on {pic} ({part.get('view', 'a view not said')}, {seen}): {', '.join(i['name'] for i in part['lines'])}{score}")
        if line is None:
            missing.append(f"{pic} has no outline (a closed line named \"outline\"): the model can't be measured against it")
        if cam is None:
            missing.append(f"{pic} has no camera: the model won't be rendered from where it was drawn")
    sheet = draw_profiles(data, float(record["height"]), colours)
    if sheet is not None:
        sheet.save(d / "profiles.png")
        for view in VIEWS:
            if data.get(view, {}).get("lines"):
                lines_, across, s = in_cells(data[view], float(record["height"]))
                said.append(f"profiles.png, {view}: {len(lines_)} lines, {across:.2f} {'wide' if view == 'front' else 'long'} x {float(record['height']):.2f} tall (cells; a pixel is {s:.5f})")
    for view in VIEWS:
        if not data.get(view, {}).get("lines"):
            missing.append(f"no {view} yet (\"{view}\": its lines, drawn from the pictures' lines)")
            continue
        if outline(data[view]) is None:
            missing.append(f"the {view} has no outline: the model can't be measured against it")
        if view == "side" and facing_wrong(data[view]):
            other = "right" if data[view].get("faces", "left") == "left" else "left"
            missing.append(f"the side's head is on its {other}: say \"faces\": \"{other}\" (or draw it facing the other way)")
    mr.update(root, rid, lines=shown(root, rid))
    if record["status"] == "in_progress":
        mr.step(root, rid, "drawing the front and the side" if sheet is not None else "tracing the lines", 0.12 if sheet is not None else 0.08)
    return said + [f"missing: {m}" for m in missing] + ["Look at each drawing: do the lines sit on the design, each outline overlapping its picture's silhouette "
                                                        "about 90% or more? Fix lines.json and run this again."]


# ---------------------------------------------------------------- the lines on the renders

def _mask_count(mask):
    return mask.histogram()[255]


def _centroid(mask):
    from PIL import Image
    w, h = mask.size
    cols = mask.resize((w, 1), Image.BOX).tobytes()
    rows = mask.resize((1, h), Image.BOX).tobytes()
    sx, sy = sum(cols), sum(rows)
    if not sx or not sy:
        return None
    return sum(i * v for i, v in enumerate(cols)) / sx / w, sum(i * v for i, v in enumerate(rows)) / sy / h


def _side_word(c, box):
    """Which side of the box a point (fractions of the picture) is on, in words."""
    if c is None:
        return ""
    bx0, by0, bx1, by1 = box
    u = (c[0] - bx0) / max(bx1 - bx0, 1e-9)
    v = (c[1] - by0) / max(by1 - by0, 1e-9)
    row = "top" if v < 0.4 else "bottom" if v > 0.6 else ""
    col = "left" if u < 0.4 else "right" if u > 0.6 else ""
    return " ".join(w for w in (row, col) if w) or "all round"


def _overlaps(found, drawn):
    """How a silhouette and an outline, each a mask of the same size (255 inside), overlap: (both
    over either, the silhouette's share past the outline and where, the outline's share past the
    silhouette and where)."""
    from PIL import ImageChops
    either = _mask_count(ImageChops.lighter(found, drawn))
    if not either:
        return 0.0, 0.0, "", 0.0, ""
    both = _mask_count(ImageChops.multiply(found, drawn))
    past, short = ImageChops.subtract(found, drawn), ImageChops.subtract(drawn, found)
    w, h = found.size
    box = ImageChops.lighter(found, drawn).getbbox() or (0, 0, w, h)
    box = (box[0] / w, box[1] / h, box[2] / w, box[3] / h)
    return (both / either, _mask_count(past) / either, _side_word(_centroid(past), box), _mask_count(short) / either, _side_word(_centroid(short), box))


def overlap(render, ring):
    """How a closed outline (in a render's pixels) and the model's silhouette there (all that
    isn't the render's background) overlap: (both over either, the model's share past the
    outline and where, the outline's share past the model and where)."""
    from PIL import Image, ImageChops, ImageDraw
    rgb = render.convert("RGB")
    bg = rgb.getpixel((1, 1))
    model = ImageChops.difference(rgb, Image.new("RGB", rgb.size, bg)).convert("L").point(lambda v: 255 if v > 10 else 0)
    drawn = Image.new("L", rgb.size, 0)
    ImageDraw.Draw(drawn).polygon([tuple(p) for p in ring], fill=255)
    return _overlaps(model, drawn)


def _overlap_text(o, thing="the model", again=None):
    """An overlap in words: "the outline and the model overlap 86%; the model goes 7% past it
    (top); it goes 3% past the model (left)" (`again`: what the thing is called after that)."""
    share, past, where_past, short, where_short = o
    again = again or thing
    out = f"the outline and {thing} overlap {share:.0%}"
    if past >= 0.01:
        out += f"; {again} goes {past:.0%} past it ({where_past})"
    if short >= 0.01:
        out += f"; it goes {short:.0%} past {again} ({where_short})"
    return out


def _short(label, o, thing="the model", why="no outline to measure", keys=None):
    """An overlap said short, for under a drawing: "front · 86% overlap · the model past it 7%
    (top)"; with no overlap, why. `keys`: the colours the drawing shows each going past in, for
    a square before each (_caption)."""
    if o is None:
        return [label, why]
    share, past, where_past, short, where_short = o
    keys = keys or (None, None)
    out = [label, f"{share:.0%} overlap"]
    if past >= 0.01:
        out.append((f"{thing} past it {past:.0%} ({where_past})", keys[0]))
    if short >= 0.01:
        out.append((f"it past {thing} {short:.0%} ({where_short})", keys[1]))
    return out


def _caption(img, parts, below=False):
    """Words along a drawing's foot, on as many lines as they need: over it, or `below` it (the
    drawing made taller for them). A part given as (words, colour) has a square of that colour
    before it: the key to what the drawing shows in it. Returns the drawing."""
    from PIL import Image, ImageDraw
    d = ImageDraw.Draw(img)
    size = max(9, min(15, img.width // 42))
    font, step, dot = _font(size), size + 5, " · "
    parts = [p if isinstance(p, tuple) else (p, None) for p in parts]
    # each row's words on one baseline, a key's square as tall as a capital
    rise, cap = (-d.textbbox((0, 0), sample, font=font, anchor="ls")[1] for sample in ("Ahl(%", "H"))

    def wide(part):
        return d.textlength(part[0], font=font) + (cap + 5 if part[1] else 0)

    rows, row, used = [], [], 0.0
    for part in parts:
        more = wide(part) + (d.textlength(dot, font=font) if row else 0)
        if row and used + more > img.width - 12:
            rows.append(row)
            row, used = [part], wide(part)
        else:
            row.append(part)
            used += more
    rows.append(row)
    band = 6 + step * len(rows)
    if below:
        taller = Image.new(img.mode, (img.width, img.height + band))
        taller.paste(img, (0, 0))
        img, d = taller, ImageDraw.Draw(taller)
    top = img.height - band
    d.rectangle((0, top, img.width, img.height), fill=(30, 30, 34))
    for i, row in enumerate(rows):
        x, base = 6.0, top + 3 + step * i + rise
        for j, (text, key) in enumerate(row):
            if j:
                d.text((x, base), dot, fill=(235, 235, 235), font=font, anchor="ls")
                x += d.textlength(dot, font=font)
            if key:
                d.rectangle((x, base - cap, x + cap, base), fill=tuple(key))
                x += cap + 5
            d.text((x, base), text, fill=(235, 235, 235), font=font, anchor="ls")
            x += d.textlength(text, font=font)
    return img


def _add_row(sheet_path, files, gap=12):
    """compare.png (the kit's: the pictures, then the renders) with one more row under them."""
    from PIL import Image
    sheet = Image.open(sheet_path).convert("RGB")
    W, H = sheet.size
    height = max(40, (H - gap) // 2 - gap)
    tiles = []
    for f in files:
        t = Image.open(f).convert("RGB")
        tiles.append(t.resize((max(1, round(t.width * height / t.height)), height), Image.LANCZOS))
    width = max(W, sum(t.width for t in tiles) + gap * (len(tiles) + 1))
    out = Image.new("RGB", (width, H + height + gap), sheet.getpixel((0, 0)))
    out.paste(sheet, (0, 0))
    x = gap
    for t in tiles:
        out.paste(t, (x, H))
        x += t.width + gap
    out.save(sheet_path)


def on_renders(root, rid, since=0.0):
    """After a render (request_kit.py's previews wrote views.json: how each ortho render sees
    the world): the front's and the side's lines on the model (fit_front.png, fit_side.png),
    each picture's lines on the model seen from its camera (match_<picture>.png: lined up on
    the outline's box), how far each outline and the model overlap, and those drawings as a row
    more in compare.png. Returns what to say (nothing when there's nothing to draw)."""
    d = mr.folder(root, rid)
    views_path = d / "views.json"
    if not path(root, rid).is_file() or not views_path.is_file() or views_path.stat().st_mtime < since - 1:
        return []
    mr._pillow("lines on the renders")
    from PIL import Image
    data, views, record = read(root, rid), json.loads(views_path.read_text(encoding="utf-8")), mr.load(root, rid)
    colours = pens(data, record["images"])
    made, said = [], []

    def lay(render_name, items, to_px, ring, out_name, what, label):
        img = Image.open(d / render_name).convert("RGBA")
        ss = 2
        layer = Image.new("RGBA", (img.width * ss, img.height * ss), (0, 0, 0, 0))
        _ink(layer, items, lambda p: tuple(c * ss for c in to_px(p)), colours, 2 * ss, True, _font(13 * ss))
        o = overlap(img, [to_px(p) for p in ring]) if ring else None
        out = Image.alpha_composite(img, layer.resize(img.size, Image.LANCZOS)).convert("RGB")
        text = f"{what}: {_overlap_text(o)}" if o else f"{what} (no outline to measure)"
        _caption(out, _short(label, o))
        out.save(d / out_name)
        made.append(d / out_name)
        said.append(f"{out_name}: {text}")

    for view in VIEWS:
        part, name = data.get(view), f"preview_{view}.png"
        if not part or not part.get("lines") or name not in views or not (d / name).is_file():
            continue
        v = views[name]
        cells = in_cells(part, float(record["height"]))[0]
        right, up, centre, scale, (w, _) = v["right"], v["up"], v["centre"], v["scale"], v["size"]
        axis = 0 if view == "front" else 1

        def to_px(p, right=right, up=up, centre=centre, scale=scale, w=w, axis=axis):
            world = [0.0, 0.0, p[1]]
            world[axis] = p[0]
            rel = [a - b for a, b in zip(world, centre)]
            return (w / 2 + sum(a * b for a, b in zip(rel, right)) / scale * w, w / 2 - sum(a * b for a, b in zip(rel, up)) / scale * w)

        ring = next((pts for (n, pts), item in zip(cells, part["lines"]) if n == "outline" and is_closed(item)), None)
        lay(name, _redrawn(cells, part["lines"]), to_px, ring, f"fit_{view}.png", f"the {view}'s lines on the model", f"the {view}")

    for pic in record["images"]:
        part, name = data.get("pictures", {}).get(pic), f"seen_{Path(pic).stem}.png"
        if not part or not part.get("lines") or name not in views or not (d / name).is_file():
            continue
        v = views[name]
        line = outline(part)
        ref = shape(line) if line else [p for item in part["lines"] for p in shape(item)]
        ox0, oy0, ox1, oy1 = _box(ref)
        mx0, my0, mx1, my1 = v["box"]
        k = math.sqrt(max(mx1 - mx0, 1e-6) / max(ox1 - ox0, 1e-6) * max(my1 - my0, 1e-6) / max(oy1 - oy0, 1e-6))
        ocx, ocy, mcx, mcy = (ox0 + ox1) / 2, (oy0 + oy1) / 2, (mx0 + mx1) / 2, (my0 + my1) / 2

        def to_px(p, k=k, ocx=ocx, ocy=ocy, mcx=mcx, mcy=mcy):
            return (mcx + (p[0] - ocx) * k, mcy + (p[1] - ocy) * k)

        lay(name, part["lines"], to_px, shape(line) if line else None, f"match_{Path(pic).stem}.png", f"{pic}'s lines on the model, seen from its camera", f"{pic}, from its camera")

    if made:
        sheet = d / "compare.png"
        if sheet.is_file() and sheet.stat().st_mtime >= since - 1:
            _add_row(sheet, made)
            said.append("compare.png: those as its last row")
        mr.update(root, rid, lines=shown(root, rid))
    return said


# ---------------------------------------------------------------- each outline on its picture

def _rim(size):
    """A box's edge pixels, as a mask."""
    from PIL import Image, ImageDraw
    rim = Image.new("L", size, 0)
    ImageDraw.Draw(rim).rectangle((0, 0, size[0] - 1, size[1] - 1), outline=255)
    return rim


def _share(mask, within):
    """How much of a mask's pixels (`within`) another one covers."""
    from PIL import ImageChops
    n = _mask_count(within)
    return _mask_count(ImageChops.multiply(mask, within)) / n if n else 0.0


def _near(rgb, colour, rim):
    """The pixels near a colour (the channel most off it at most NEAR's first, or more when the
    box's edges near it vary more: how far off it nearly all of them are, and some)."""
    from PIL import Image, ImageChops
    r, g, b = ImageChops.difference(rgb, Image.new("RGB", rgb.size, tuple(colour))).split()
    off = ImageChops.lighter(ImageChops.lighter(r, g), b)
    edges = off.histogram(mask=rim)[:49]
    spread, run, most = 0, 0, 0.95 * sum(edges)
    for v, n in enumerate(edges):
        run += n
        if run >= most:
            spread = v
            break
    tolerance = min(NEAR[1], max(NEAR[0], spread + 12))
    return off.point(lambda v: 255 if v <= tolerance else 0)


def _tones(rgb, colour):
    """The pixels that are a darker tone of a colour (its shadow on the ground): the colour
    times a lightness between SHADOW's, each channel at most SHADOW's tint off it. None on a
    near black colour (a shadow there doesn't show)."""
    from PIL import ImageChops
    lo, hi, tint = SHADOW
    sq = float(sum(c * c for c in colour))
    if sq < 30 ** 2:
        return None
    # how much of the colour a pixel is (its share along it, 255 for all of it)...
    share = rgb.convert("L", tuple(255 * c / sq for c in colour) + (0,))
    tones = share.point(lambda v: 255 if lo * 255 <= v <= hi * 255 else 0)
    # ...and what's left of it once that share is taken off (another hue, or a tint), 128 for none
    u = [c / math.sqrt(sq) for c in colour]
    left = rgb.convert("RGB", tuple(v for i in range(3) for v in [(1.0 if i == j else 0.0) - u[i] * u[j] for j in range(3)] + [128]))
    for band in left.split():
        tones = ImageChops.multiply(tones, band.point(lambda v: 255 if abs(v - 128) <= tint else 0))
    return tones


def _foot(mask):
    """The row nearly all of a mask's pixels are above (all but a hundredth of them)."""
    from PIL import Image
    rows = mask.resize((1, mask.height), Image.BOX).tobytes()
    most, run = 0.99 * sum(rows), 0
    for y, v in enumerate(rows):
        run += v
        if run >= most:
            return y
    return mask.height


def _from_edges(free):
    """What of a mask's 255s its edges reach through them (each pixel to its four neighbours), a
    row's run at a time, as a mask."""
    from PIL import Image
    w, h = free.size
    can = free.tobytes()
    reached = bytearray(w * h)
    seeds = [i for i in (*range(w), *range((h - 1) * w, h * w), *range(0, h * w, w), *range(w - 1, h * w, w)) if can[i]]
    while seeds:
        i = seeds.pop()
        if reached[i]:
            continue
        row = i - i % w
        a, b = can.rfind(b"\0", row, i), can.find(b"\0", i, row + w)
        a, b = row if a < 0 else a + 1, row + w if b < 0 else b
        reached[a:b] = b"\xff" * (b - a)
        for other in (row - w, row + w):
            if not 0 <= other < w * h:
                continue
            x, end = a - row + other, b - row + other
            while x < end:
                j = can.find(b"\xff", x, end)
                if j < 0:
                    break
                if not reached[j]:
                    seeds.append(j)
                k = can.find(b"\0", j, other + w)
                x = other + w if k < 0 else k
    return Image.frombytes("L", (w, h), bytes(reached))


BUSY = "the box's edges aren't mostly one colour (a busy background)"


def silhouette(img, box):
    """A picture's silhouette in a box of it: every pixel the background doesn't reach from the
    box's edges. The background is the colour along most of the edges (as model_requests.py
    colours finds it) and, right under the creature, a darker tone of it: its shadow on the
    ground. A picture whose edges there are see-through: what isn't. Returns (a mask, 255 where
    the silhouette is, at most SCORED pixels across; its scale to the picture's pixels; ""), or
    (None, 1, why it can't be told)."""
    from PIL import Image, ImageChops, ImageDraw
    part = img.convert("RGBA").crop(box)
    w, h = part.size
    rim = _rim(part.size)
    clear = part.getchannel("A").point(lambda v: 255 if v < 128 else 0)
    if _share(clear, rim) >= 0.5:
        free = clear
    else:
        colour = mr.background(mr.palette(part)[0])
        if colour is None:
            return None, 1.0, BUSY
        rgb = part.convert("RGB")
        near = _near(rgb, colour, rim)
        if _share(near, rim) < 0.5:
            return None, 1.0, BUSY
        free = ImageChops.lighter(near, clear)
        rest = ImageChops.invert(free)
        tones, middle = _tones(rgb, colour), _centroid(rest)
        if tones is not None and middle is not None:
            # right under the creature: below its middle (of all that isn't the background), a
            # quarter of the way down to its foot (where its shadow is)
            top = middle[1] * h + 0.25 * (_foot(rest) - middle[1] * h)
            under = Image.new("L", part.size, 0)
            ImageDraw.Draw(under).rectangle((0, round(top), w, h), fill=255)
            free = ImageChops.lighter(free, ImageChops.multiply(tones, under))
    s = min(1.0, SCORED / max(w, h))
    if s < 1.0:
        # smaller, a pixel is background only where all it stands for is: no line between the
        # creature and the background opens
        free = free.resize((max(1, round(w * s)), max(1, round(h * s))), Image.BOX).point(lambda v: 255 if v == 255 else 0)
    return ImageChops.invert(_from_edges(free)), s, ""


def _masks(img, line):
    """A picture's silhouette and its outline (a closed line in its pixels) filled, as masks
    the same size, over the outline's box with room round it (`around`): (silhouette, outline,
    that box, ""), or (None, None, that box, why the silhouette can't be told)."""
    from PIL import Image, ImageDraw
    box = around([line], img.size)
    found, s, why = silhouette(img, box)
    if found is None:
        return None, None, box, why
    drawn = Image.new("L", found.size, 0)
    ImageDraw.Draw(drawn).polygon([((x - box[0]) * s, (y - box[1]) * s) for x, y in shape(line)], fill=255)
    return found, drawn, box, ""


def outline_score(img, line):
    """How a picture's outline (a closed line in its pixels) and the picture's own silhouette
    overlap, in the outline's box with room round it (`around`), as `overlap` says it for a
    render: (the overlap, ""), or (None, why it can't be scored)."""
    found, drawn, _, why = _masks(img, line)
    return (_overlaps(found, drawn), "") if found is not None else (None, why)


def _tint(found, drawn):
    """Where a silhouette and an outline (masks the same size) part, as a see-through layer that
    size: the silhouette past the outline in PAST's colour, the outline past the silhouette in
    SHORT's; slivers thinner than three of its pixels left out (where they agree but for the
    drawing's own edge)."""
    from PIL import Image, ImageChops, ImageFilter
    layer = Image.new("RGBA", found.size, (0, 0, 0, 0))
    for mask, colour in ((ImageChops.subtract(found, drawn), PAST), (ImageChops.subtract(drawn, found), SHORT)):
        mask = mask.filter(ImageFilter.MinFilter(3)).filter(ImageFilter.MaxFilter(3))
        tint = Image.new("RGBA", found.size, colour + (0,))
        tint.putalpha(mask.point(lambda v: round(v * TINT)))
        layer = Image.alpha_composite(layer, tint)
    return layer
