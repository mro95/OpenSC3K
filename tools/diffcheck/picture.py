"""The accuracy image for the README: one terrain from the original and the port side by side,
their difference, and the match rates of every check."""

import datetime

from PIL import Image, ImageDraw, ImageFont

BG, INK, MUTED, RULE = (255, 255, 255), (32, 36, 40), (110, 116, 122), (226, 228, 231)
GOOD, BAD, PARTIAL = (46, 160, 67), (207, 34, 46), (191, 135, 0)
SCALE, PAD, GAP = 2, 24, 16


def font(size):
    try:
        return ImageFont.load_default(size=size)
    except TypeError:                       # Pillow < 10.1 has one bitmap size
        return ImageFont.load_default()


def preview_rgb(t, x, y):
    """`Terrain::preview_rgb` from crates/sc3k-sim/src/dirt.rs, so both sides look the same."""
    clamp = lambda v, lo, hi: max(lo, min(hi, v))  # noqa: E731
    sea, a = t.sea, t.at("altitude", x, y)
    if a <= t.at("water", x, y):
        depth = clamp(sea - a, 0, 40)
        fresh = 0 if t.at("salt", x, y) else 30
        return (20, 90 + fresh - depth * 2, 200 - depth * 3)
    up = t.at("altitude", max(x - 1, 0), max(y - 1, 0))
    light = clamp((a - up) * 12, -60, 60)
    h = clamp(a - sea, 0, 120)
    c = [90 + h, 150 - h // 3, 60 + h // 2]
    f = t.at("flora", x, y)
    if f > 0:
        c = [c[0] // 2, c[1] - 30 - f // 8, c[2] // 2]
    return tuple(clamp(v + light, 0, 255) for v in c)


def terrain_image(t):
    img = Image.new("RGB", (t.vx, t.vy))
    img.putdata([preview_rgb(t, x, y) for y in range(t.vy) for x in range(t.vx)])
    return img.resize((t.vx * SCALE, t.vy * SCALE), Image.NEAREST)


def diff_image(want, got):
    """Grey where every map agrees, red by altitude difference, amber where only water,
    flora or salt differ."""
    px = []
    for y in range(want.vy):
        for x in range(want.vx):
            da = abs(want.at("altitude", x, y) - got.at("altitude", x, y))
            other = any(want.at(m, x, y) != got.at(m, x, y) for m in ("water", "flora", "salt"))
            if da:
                k = min(1.0, 0.35 + da / 16)
                px.append((255, int(255 * (1 - k)), int(255 * (1 - k))))
            elif other:
                px.append(PARTIAL)
            else:
                g = sum(preview_rgb(want, x, y)) // 3
                g = 215 + g * 35 // 255
                px.append((g, g, g))
    img = Image.new("RGB", (want.vx, want.vy))
    img.putdata(px)
    return img.resize((want.vx * SCALE, want.vy * SCALE), Image.NEAREST)


def render(path, sample, rows, footer):
    """`sample` is a DirtCase with both terrains; `rows` is [(section, label, rate, detail)]."""
    panels = [("Original SIMDIRT.DLL (emulated)", terrain_image(sample.original)),
              ("OpenSC3K port", terrain_image(sample.port)),
              ("Difference", diff_image(sample.original, sample.port))]
    pw, ph = panels[0][1].size
    title_f, label_f, body_f, small_f = font(22), font(14), font(14), font(12)
    row_h, section_h = 22, 30
    sections = len({r[0] for r in rows})
    width = PAD * 2 + pw * 3 + GAP * 2
    height = (PAD + 34 + 20 + 20 + ph + 28 + sections * section_h + len(rows) * row_h
              + 30 + PAD)
    img = Image.new("RGB", (width, height), BG)
    d = ImageDraw.Draw(img)
    y = PAD
    d.text((PAD, y), "Decomp accuracy: original vs. port", font=title_f, fill=INK)
    y += 34
    d.text((PAD, y), f"Sample: {sample.label()}", font=small_f, fill=MUTED)
    y += 20
    for i, (label, im) in enumerate(panels):
        x = PAD + i * (pw + GAP)
        d.text((x, y), label, font=label_f, fill=INK)
        img.paste(im, (x, y + 20))
        d.rectangle([x - 1, y + 19, x + pw, y + 20 + ph], outline=RULE)
    y += 20 + ph + 28

    bar_x, bar_w = PAD + 330, pw * 3 + GAP * 2 - 330 - 260
    section = None
    for sec, label, rate, detail in rows:
        if sec != section:
            section = sec
            d.text((PAD, y + 6), sec, font=label_f, fill=INK)
            d.line([PAD, y + section_h - 4, width - PAD, y + section_h - 4], fill=RULE)
            y += section_h
        colour = GOOD if rate == 1.0 else PARTIAL if rate >= 0.99 else BAD
        d.text((PAD + 8, y + 3), label, font=body_f, fill=INK)
        d.rectangle([bar_x, y + 5, bar_x + bar_w, y + 15], fill=RULE)
        d.rectangle([bar_x, y + 5, bar_x + int(bar_w * rate), y + 15], fill=colour)
        d.text((bar_x + bar_w + 10, y + 3), f"{rate * 100:.2f}%", font=body_f, fill=colour)
        d.text((bar_x + bar_w + 80, y + 4), detail, font=small_f, fill=MUTED)
        y += row_h
    y += 10
    stamp = datetime.date.today().isoformat()
    d.text((PAD, y), f"{footer} · {stamp} · tools/diffcheck/run.py", font=small_f, fill=MUTED)
    img.save(path)
