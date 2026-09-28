#!/usr/bin/env bash
# Assert the committed og-image is a real, well-formed, reproducible 1200x630
# link-preview card (DEF-190).
#
# Why a check and not a look: a preview card is judged on properties no human
# review reliably catches — the exact dimensions a scraper crops to, that it
# is not a flat fill, that the wordmark is not clipped, and that the committed
# bytes are the ones the generator produces. Each of those failed once while
# this card was being built (the cross drew at full opacity, the title ran off
# the right edge), so each is now asserted.
#
# Usage: scripts/check-og-image.py <path-to-og-image.png>
#        scripts/check-og-image.py --regen   # re-render and diff the bytes
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

IMAGE="${1:-client/web/og-image.png}"
REGEN=0
[ "${1:-}" = "--regen" ] && { REGEN=1; IMAGE="client/web/og-image.png"; }

fail=0
fail() { echo "  FAIL  $*"; fail=1; }
pass() { echo "  ok    $*"; }

if [ ! -f "$IMAGE" ]; then
  echo "::error::$IMAGE is missing — the card is committed, not generated at build time" >&2
  exit 1
fi

echo "Checking $IMAGE"
python3 - "$IMAGE" <<'PY'
import sys
from PIL import Image

path = sys.argv[1]
img = Image.open(path)
fails = []

def check(ok, msg):
    print(("  ok    " if ok else "  FAIL  ") + msg)
    if not ok:
        fails.append(msg)

# --- format & geometry -----------------------------------------------------
check(img.format == "PNG", f"format is PNG (got {img.format})")
w, h = img.size
check((w, h) == (1200, 630), f"1200x630, the OG/scraper size (got {w}x{h})")
check(img.mode == "RGB", f"RGB (got {img.mode}; a scraper that skips alpha is fine)")

# --- it is a designed card, not a placeholder ------------------------------
rgb = img.convert("RGB")
colors = rgb.getcolors(maxcolors=1 << 20) or []
check(len(colors) > 20, f"many distinct colours, so it is drawn (got {len(colors)})")
top = max(colors)[1] if colors else None
share = max(c for c, _ in colors) / float(w * h)
check(share < 0.9, f"no single colour covers the card (top is {share:.1%})")

# --- the brand's own palette ----------------------------------------------
present = {rgb for _, rgb in colors}
for name, hexv in [("--bg-app", "#121212"), ("--bg-card", "#18181b"),
                   ("--text-primary", "#f4f4f5"), ("--text-secondary", "#a1a1aa"),
                   ("--border-app", "#27272a"), ("--pastel-yellow", "#feea99")]:
    t = tuple(int(hexv[i:i + 2], 16) for i in (1, 3, 5))
    check(t in present, f"{name} {hexv} is in the card, so it matches the app")

# --- nothing is clipped ----------------------------------------------------
# Ink = every pixel INSIDE the card that differs from the surfaces the card is
# built from. The region outside the card is background by construction, and
# the card's own 2px border stroke is decoration rather than content, so both
# are excluded — scanning either reports the frame as clipped content. What is
# left is text and the logo; ink reaching the interior's edge is a real clip.
m = 24
border = 6          # 2px stroke, antialiased
px = rgb.load()
xs, ys = [], []
for y in range(m + border, h - m - border):
    for x in range(m + border, w - m - border):
        if px[x, y] not in ((18, 24, 27), (24, 24, 27), (39, 39, 42)):
            xs.append(x)
            ys.append(y)
if xs:
    lo_x, hi_x, lo_y, hi_y = m + border, w - m - border, m + border, h - m - border
    check(min(xs) > lo_x and max(xs) < hi_x and min(ys) > lo_y and max(ys) < hi_y,
          f"all content inside the card (ink x {min(xs)}..{max(xs)}, y {min(ys)}..{max(ys)}, "
          f"content area {lo_x}..{hi_x}/{lo_y}..{hi_y})")
else:
    check(False, "card has no content at all")

# --- the logo's dimmed cross is actually dimmed ---------------------------
# brand.rs draws the plus at opacity 0.3; a full-opacity cross means the
# transcription lost the alpha and the mark is wrong.
cx, cy, s = w // 2, 128, 96
scale = s / 24.0
def at(gx, gy):
    return px[int(cx + (gx - 12) * scale), int(cy + (gy - 12) * scale)]
bright = at(2, 2)
cross = at(14, 2)
check(bright[0] > 200, f"a full-strength dot is bright (rgb {bright})")
check(cross[0] < 150, f"the cross dot is dimmed, not full-strength (rgb {cross})")
check(cross[0] < bright[0] / 2, f"cross is materially dimmer than the field ({cross[0]} vs {bright[0]})")

sys.exit(1 if fails else 0)
PY
geom=$?

if [ "$REGEN" = "1" ]; then
  echo
  echo "reproducibility (committed bytes == generator output):"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  if python3 scripts/make-og-image.py "$tmp/og.png" >/dev/null; then
    if cmp -s "$tmp/og.png" "$IMAGE"; then
      pass "re-running make-og-image.py reproduces the committed bytes exactly"
    else
      fail "generator output differs from the committed PNG — re-run and commit it"
    fi
  else
    fail "could not run make-og-image.py (needs Pillow + a DejaVu TTF)"
  fi
fi

echo
if [ "$fail" -ne 0 ]; then
  echo "::error::$IMAGE is not a valid link-preview card — see the FAIL lines above."
  exit 1
fi
echo "$IMAGE is a valid 1200x630 card."
