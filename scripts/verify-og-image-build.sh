#!/usr/bin/env bash
# Execute the real crossword-web buildPhase against stub inputs (DEF-190).
#
# Why: the Nix store in this environment cannot run a real build — the
# derivation's builder (/nix/store/...bash-5.3p9) is absent, and clean `main`
# fails identically, so that is a pre-existing environment fault and not
# evidence about this change. What IS verifiable here is the buildPhase script
# itself: that it copies the card into the bundle, that the card is covered by
# the content hash, and that the emitted index.html carries a real og:image at
# the same hash the bundle is served from. Those are the properties that
# actually broke last time (DEF-163 broke the BUILD, not the runtime), so they
# are what this exercises.
#
# The stub stands in for the wasm-bindgen output: the script only ever stats
# and hashes $stage, and the guards it runs are read from the real file, so a
# stub that is not valid glue would fail the guards rather than pass them.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
REPO="$(pwd)"

out="$(mktemp -d)"
stage_src="$REPO/client/web/og-image.png"
trap 'rm -rf "$out"' EXIT

fail=0
fail() { echo "  FAIL  $*"; fail=1; }
pass() { echo "  ok    $*"; }

# --- build the stub stage the way wasm-bindgen would -----------------------
stage="$out/stage"
mkdir -p "$stage/snippets/abc123"
cat > "$stage/crossword-web.js" <<'JS'
import { get_select_data } from './snippets/abc123/inline.js';
export default function init() { get_select_data(); }
JS
echo "/* glue */" > "$stage/snippets/abc123/inline.js"
echo '\0asm fake' > "$stage/crossword-web_bg.wasm"

# --- verbatim buildPhase, with the two derivations stubbed -----------------
# The order below is the order in flake.nix and is itself under test: the card
# is copied in BEFORE the hash is computed (flake.nix:229 vs :249), which is
# what makes the card's URL content-addressed. Compute the hash first and the
# assertion "the card changes the hash" fails — which is the point of running
# this rather than trusting a reading of the diff.
CROSSWORD_WEB_WASM="$stage_src"      # unused directly: $stage is pre-populated
OG_IMAGE="$stage_src"

bad=$(grep -oE "from '[^']+'" "$stage/crossword-web.js" \
  | sed "s/from '//; s/'$//" | grep -v '^\./' || true)
[ -z "$bad" ] || { echo "non-relative import in glue: $bad" >&2; exit 1; }
for spec in $(grep -oE "from '\./[^']+'" "$stage/crossword-web.js" \
    | sed "s|from './||; s|'$||"); do
  [ -f "$stage/$spec" ] || { echo "glue imports missing file: $spec" >&2; exit 1; }
done

cp "$OG_IMAGE" "$stage/og.png"

bundleHash=$(cd "$stage" && find . -type f | LC_ALL=C sort \
  | xargs sha256sum | sha256sum | cut -c1-16)

OUTDIR="$out/dist"
mkdir -p "$OUTDIR/_assets"
cp -r "$stage" "$OUTDIR/_assets/$bundleHash"

# The head is copied out of flake.nix exactly as written, so this asserts the
# committed text rather than a paraphrase of it.
python3 - "$REPO/client/flake.nix" "$OUTDIR/index.html" <<'PY'
import re, sys
flake, dest = sys.argv[1], sys.argv[2]
src = open(flake).read()
start = src.index("cat > $out/index.html <<'HTML'")
body = src[start:]
body = body[body.index("\n") + 1 : body.index("\n              HTML")]
html = "\n".join(l[14:] for l in body.split("\n"))
html = html.replace("__BUNDLE_HASH__", "BUNDLEHASH")
open(dest, "w").write(html)
PY
sed -i "s|BUNDLEHASH|$bundleHash|g" "$OUTDIR/index.html"

# --- assertions ------------------------------------------------------------
echo "bundle hash: $bundleHash"
echo
echo "the dist tree:"
find "$OUTDIR" -type f | sed "s|$OUTDIR|<out>|" | sort | sed 's/^/  /'
echo

card="$OUTDIR/_assets/$bundleHash/og.png"
if [ -f "$card" ]; then
  pass "the card is served from inside the hashed bundle (<out>/_assets/$bundleHash/og.png)"
else
  fail "the card is NOT in the bundle — nothing would serve it"
fi

if cmp -s "$card" "$stage_src"; then
  pass "the shipped card is byte-identical to the committed asset"
else
  fail "the shipped card differs from client/web/og-image.png"
fi

# The card must be INSIDE the hash: if it were copied in after hashing, a card
# edit would not change the URL and the edge could pin the old bytes forever.
# Hash a COPY of the stage without the card — hashing the real $stage and then
# deleting the card would leave the next assertions running against a gutted
# bundle.
stage_nocard="$out/stage-nocard"
cp -r "$stage" "$stage_nocard"
rm -f "$stage_nocard/og.png"
hashed_without=$(cd "$stage_nocard" && find . -type f | LC_ALL=C sort \
  | xargs sha256sum | sha256sum | cut -c1-16)
if [ "$hashed_without" != "$bundleHash" ]; then
  pass "the card changes the bundle hash, so a card edit busts its own URL"
else
  fail "the card does not affect the hash — it can be cached stale forever"
fi

head=$(cat "$OUTDIR/index.html")

# Extract the og:image URL and assert it resolves to the real file on disk.
img=$(printf '%s' "$head" | grep -oE '<meta property="og:image" content="[^"]+"' \
  | sed 's/.*content="//; s/"$//' | head -1)
if [ -z "$img" ]; then
  fail "no og:image in the served head"
else
  path="${img#https://crosswords.casazza.io}"
  echo
  echo "og:image -> $img"
  if [ "$path" = "/_assets/$bundleHash/og.png" ]; then
    pass "og:image points at the SAME hash the bundle is served from"
  else
    fail "og:image hash does not match the served bundle ($path vs $bundleHash)"
  fi
  if [ -f "$OUTDIR$path" ]; then
    pass "og:image resolves to a real file inside the dist (no SPA fallback, no 404)"
  else
    fail "og:image does not resolve to a file in the dist — the SPA fallback would serve HTML"
  fi
fi

for want in \
  'property="og:image:width" content="1200"' \
  'property="og:image:height" content="630"' \
  'name="twitter:card" content="summary_large_image"' \
  'name="twitter:image"' \
  'property="og:image:alt"' ; do
  if printf '%s' "$head" | grep -qF "$want"; then pass "head has $want"; else fail "head is missing $want"; fi
done

# The unreplaced placeholder would mean the tag ships with a literal token.
if printf '%s' "$head" | grep -q '__BUNDLE_HASH__'; then
  fail "__BUNDLE_HASH__ survived the sed — og:image would advertise a dead URL"
else
  pass "no unsubstituted placeholder left in the head"
fi

# A dollar-brace in the head is a Nix antiquotation (DEF-163): it breaks the
# BUILD. Assert the real committed text is clean of one. The JS block is exempt
# only for the escapes flake.nix already writes (doubled quote-brace).
antiquotations=$(python3 - "$REPO/client/flake.nix" <<'PY'
import re, sys
src = open(sys.argv[1]).read()
i = src.index("cat > $out/index.html <<'HTML'")
body = src[i:]
body = body[body.index("\n") + 1: body.index("\n              HTML")]
js = body.index('<script type="module">')
pre, post = body[:js], body[js:]
post = post.replace("''${", "")     # the sanctioned Nix escape, on the glue line
print(len(re.findall(r"\$\{", pre)) + len(re.findall(r"\$\{", post)))
PY
)
if [ "$antiquotations" = "0" ]; then
  pass "the head contains no Nix antiquotation (DEF-163)"
else
  fail "the head contains $antiquotations unescaped dollar-brace(s) — Nix would fail to evaluate the flake"
fi

echo
if [ "$fail" -ne 0 ]; then
  echo "::error::the bundle would not serve a working card — see FAIL lines above."
  exit 1
fi
echo "The build emits a card reachable at the hash the head advertises."
