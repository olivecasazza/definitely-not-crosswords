#!/bin/bash
# Guards the bug runner-toolchain/cc exists to fix: the link flags must be
# dropped for -c/-S/-E and kept for a real link. Run it with a `zig` on PATH
# (or ZIG=/path/to/zig). Exits non-zero on regression.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
CC="$HERE/cc"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
printf 'int f(void) { return 0; }\n' >"$tmp/t.c"

# 1. Compiling must not link. The unpatched zigcc fails here with
#    "attempted static link of dynamic object libstdc++.so.6".
if ! "$CC" -c "$tmp/t.c" -o "$tmp/t.o" 2>"$tmp/compile.log"; then
  echo "FAIL: cc -c failed:" >&2
  cat "$tmp/compile.log" >&2
  exit 1
fi
if grep -qE 'attempted static link|undefined symbol: main' "$tmp/compile.log"; then
  echo "FAIL: cc -c attempted a link:" >&2
  cat "$tmp/compile.log" >&2
  exit 1
fi
[ -f "$tmp/t.o" ] || { echo "FAIL: cc -c produced no object file" >&2; exit 1; }

# 2. Linking must still work, otherwise nothing downstream can build.
cat >"$tmp/main.c" <<'EOF'
int f(void);
int main(void) { return f(); }
EOF
if ! "$CC" "$tmp/main.c" "$tmp/t.c" -o "$tmp/prog" 2>"$tmp/link.log"; then
  echo "FAIL: cc link failed:" >&2
  cat "$tmp/link.log" >&2
  exit 1
fi
"$tmp/prog"

echo "PASS: runner-toolchain/cc compiles and links"
