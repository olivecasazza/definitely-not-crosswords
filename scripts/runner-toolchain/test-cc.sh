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

# 3. A linked binary must still resolve the libs the wrapper appends
#    (-lpthread -ldl), i.e. the wrapper must still append them when linking for the
#    host. Guards against "fix the -c case" being implemented by dropping the flags
#    entirely, which links clean and then fails at runtime. pthread_create and
#    dlopen are symbols those two flags are what provide here.
cat >"$tmp/uses_linkflags.c" <<'EOF'
#include <dlfcn.h>
#include <pthread.h>
#include <stdio.h>
static void *noop(void *arg) { return arg; }
int main(void) {
  pthread_t t;
  if (pthread_create(&t, NULL, noop, NULL) != 0) return 1;
  pthread_join(t, NULL);
  void *h = dlopen("libm.so.6", RTLD_NOW);
  if (!h) return 2;
  dlclose(h);
  printf("ok\n");
  return 0;
}
EOF
if ! "$CC" "$tmp/uses_linkflags.c" -o "$tmp/linkflags" 2>"$tmp/linkflags.log"; then
  echo "FAIL: cc could not link a translation unit needing -lpthread/-ldl:" >&2
  cat "$tmp/linkflags.log" >&2
  exit 1
fi
if [ "$("$tmp/linkflags")" != "ok" ]; then
  echo "FAIL: linked binary did not run (host glibc/libstdc++ link flags lost)" >&2
  exit 1
fi

# 4. Host target mapping: cargo passes --target=x86_64-unknown-linux-gnu, which
#    zig rejects. The wrapper maps it onto zig's own triple.
if ! "$CC" --target=x86_64-unknown-linux-gnu -c "$tmp/t.c" -o "$tmp/t.target.o" 2>"$tmp/target.log"; then
  echo "FAIL: cc --target=x86_64-unknown-linux-gnu failed:" >&2
  cat "$tmp/target.log" >&2
  exit 1
fi

# 5. wasm mapping: `client/flake.nix` builds crossword-web for
#    wasm32-unknown-unknown, a triple zig cannot parse at all
#    ("UnknownOperatingSystem"). It must be mapped onto a triple zig accepts, and
#    the host's x86_64 glibc/libstdc++ flags must NOT reach a wasm link.
if ! "$CC" --target=wasm32-unknown-unknown -c "$tmp/t.c" -o "$tmp/t.wasm.o" 2>"$tmp/wasm.log"; then
  echo "FAIL: cc --target=wasm32-unknown-unknown failed:" >&2
  cat "$tmp/wasm.log" >&2
  exit 1
fi
if [ ! -f "$tmp/t.wasm.o" ]; then
  echo "FAIL: wasm compile produced no object file" >&2
  exit 1
fi
if grep -qE 'libstdc\+\+|static link of dynamic object' "$tmp/wasm.log"; then
  echo "FAIL: host glibc/libstdc++ link flags leaked into a wasm compile:" >&2
  cat "$tmp/wasm.log" >&2
  exit 1
fi

echo "PASS: runner-toolchain/cc compiles, links, and maps host + wasm targets"
