#!/usr/bin/env python3
"""Generate the 1200x630 link-preview card (DEF-190).

The card is drawn, not designed in a bitmap editor, so it is regenerable and
diffable: the pixels are a function of this file, and re-running it on a clean
tree reproduces the committed bytes exactly. `nix build` ships the committed PNG;
this script is the record of how those bytes were produced. Determinism is
asserted by scripts/check-og-image.sh.

Why 1200x630: the size every major scraper prefers (Open Graph's own floor is
1200x630, Twitter's summary_large_image is 2:1 with a 300x157 minimum), and
1.91:1, which is the shape a link preview actually reserves.

Palette and mark are the app's own, not invented here:
  --bg-app #121212, --bg-card #18181b, --text-primary #f4f4f5,
  --text-secondary #a1a1aa, --border-app #27272a, --pastel-yellow #feea99
  (client/web/src/styles.rs:21-31, the dark row)
The 6x6 dot grid with the dimmed cross is transcribed from
client/web/src/components/brand.rs:8-20 so the card carries the real logo
instead of a second, drifting one.

Text is the same approved copy the <head> serves (DEF-164/DEF-202), so a card
and a search snippet cannot disagree.

Usage:  scripts/make-og-image.py [OUT.png]
Needs:  Pillow. Fonts resolve from a short list; the build does not depend on
        this script, so a missing font is a local-only failure.
"""

import sys
from PIL import Image, ImageDraw, ImageFont

W, H = 1200, 630

BG = "#121212"           # --bg-app
CARD = "#18181b"         # --bg-card
FG = "#f4f4f5"           # --text-primary
DIM = "#a1a1aa"          # --text-secondary
LINE = "#27272a"         # --border-app
YELLOW = "#feea99"       # --pastel-yellow

TITLE = "definitely-not-crosswords"
TAGLINE = "free real-time co-op crosswords"
# The Free row of README.md "Plans & pricing" (55-62): the same plan claim the
# served description makes, so the card cannot over-promise.
PLAN = "Free plan: $0, unlimited solving."

# The one detail that matters for a preview card: a plain $ is fine, but this
# file is read by a Nix heredoc in some future refactor, and `${` is a Nix
# antiquotation. Keep every literal here free of a dollar before a brace.
assert "${" not in TITLE + TAGLINE + PLAN

FONT_CANDIDATES = [
    "/paperclip/.cache/fonts/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
]
REGULAR_CANDIDATES = [
    "/paperclip/.cache/fonts/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
]
MONO_CANDIDATES = [
    "/paperclip/.cache/fonts/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
]


def load(candidates, size):
    for path in candidates:
        try:
            return ImageFont.truetype(path, size)
        except OSError:
            continue
    raise SystemExit(
        f"no usable font at size {size}; tried {candidates}. "
        "Install DejaVu or drop the TTFs in one of those paths."
    )


def dimmed(x, y):
    """The logo's dimmed plus/cross — transcribed from brand.rs:8-12."""
    return (x == 14 and 2 <= y <= 18) or (y == 10 and 6 <= x <= 22)


def draw_logo(img, cx, cy, size):
    """The 6x6 dot-grid brand mark, r=1.2 on a 24 viewBox, scaled to `size`.

    brand.rs sets opacity 0.3 on the cross dots over an unknown backdrop, so
    the dimmed dots go on their own RGBA layer and are alpha-composited —
    filling them with a flat tint would bake in the wrong colour and lose the
    antialiased edge the browser gives the SVG.
    """
    scale = size / 24.0
    r = 1.2 * scale
    bright = Image.new("RGBA", img.size, (0, 0, 0, 0))
    faint = Image.new("RGBA", img.size, (0, 0, 0, 0))
    fb, ff = ImageDraw.Draw(bright), ImageDraw.Draw(faint)
    rgb = tuple(int(YELLOW[i : i + 2], 16) for i in (1, 3, 5))
    for y in (2, 6, 10, 14, 18, 22):
        for x in (2, 6, 10, 14, 18, 22):
            px = cx + (x - 12) * scale
            py = cy + (y - 12) * scale
            box = [px - r, py - r, px + r, py + r]
            if dimmed(x, y):
                ff.ellipse(box, fill=rgb + (77,))   # 0.3 * 255
            else:
                fb.ellipse(box, fill=rgb + (255,))
    img.paste(bright, (0, 0), bright)
    img.paste(faint, (0, 0), faint)


def fit_font(draw, candidates, text, max_w, start):
    """Largest size <= start whose rendered width fits `max_w`.

    The wordmark is the one string here long enough to reach the card edge
    (983px at 68pt in DejaVu Sans Bold against 1088px of usable width), so it is
    fitted rather than hardcoded — a font swap must not silently push the name
    off the card, and a cropped name is the failure nobody notices until a link
    is already shared.
    """
    size = start
    while size > 16:
        font = load(candidates, size)
        if draw.textlength(text, font=font) <= max_w:
            return font
        size -= 2
    return load(candidates, 16)


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "og-image.png"
    img = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(img)
    # A card surface, inset, so the preview reads as the product's own panel
    # rather than a flat rectangle of background.
    m = 24
    d.rounded_rectangle([m, m, W - m, H - m], radius=20, fill=CARD, outline=LINE, width=2)

    # Usable text width: the card inset, plus the breathing room the app's own
    # panels keep from their borders.
    pad = 64
    left, right = m + pad, W - m - pad
    max_w = right - left

    # Logo centred above the wordmark. Side-by-side was the first layout and
    # it left the title only 972px against a 983px string — the wordmark ran off
    # the right edge. Centred, the title gets the full 1088px.
    draw_logo(img, W // 2, 128, 96)

    title_font = fit_font(d, FONT_CANDIDATES, TITLE, max_w, 68)
    tag_font = load(REGULAR_CANDIDATES, 38)
    plan_font = load(MONO_CANDIDATES, 30)

    d.text((left, 202), TITLE, font=title_font, fill=FG)
    d.text((left, 282), TAGLINE, font=tag_font, fill=DIM)

    # A hairline in --border-app separates the brand from the claim.
    d.line([(left, 352), (right, 352)], fill=LINE, width=2)

    d.text(
        (left, 386),
        "Solve the same grid together,",
        font=load(REGULAR_CANDIDATES, 44),
        fill=FG,
    )
    d.text(
        (left, 444),
        "see every move as it happens.",
        font=load(REGULAR_CANDIDATES, 44),
        fill=FG,
    )

    plan_y = 528
    plan_w = d.textlength(PLAN, font=plan_font)
    pill = [left, plan_y - 12, left + plan_w + 40, plan_y + 48]
    d.rounded_rectangle(pill, radius=30, fill=BG, outline=LINE, width=2)
    d.text((left + 20, plan_y), PLAN, font=plan_font, fill=YELLOW)

    img.save(out, format="PNG", optimize=True)
    print(f"wrote {out} ({W}x{H})")


if __name__ == "__main__":
    main()
