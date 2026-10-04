# Runner toolchain for the `client/` workspace

A minimal shim that makes the native `client/` crates buildable in a container
that has no nix dev shell. This exists because `/paperclip/bin/cc` (the default
compiler in this runner) is broken for `-c`, and because `client/web/Cargo.toml`
pins `panel-kit` at an absolute host path.

`nix develop ./client` is the supported way to work on this repo and is
unaffected by any of this. Use the shim only when nix is unavailable or its
bootstrap fails in your environment.

## Usage

### The default `cc` (what most agents should just use)

`install-runner-cc.sh` wires this repo's wrapper in as the runner's default `cc`,
so a fresh session builds with no export and no wrapper script:

```bash
cargo nextest run -p crossword-server
```

If you are on a runner where it is not installed yet, `cc` is still the broken
`zigcc` — install it once (see [The default `cc`](#the-default-cc) below).

### The wrapper script (`with-cargo.sh`)

For everything `install-runner-cc.sh` does not cover — the `panel-kit` path deps
and `ORT_LIB_LOCATION`:

```bash
scripts/runner-toolchain/with-cargo.sh nextest run -p crossword-server
scripts/runner-toolchain/with-cargo.sh build -p crossword-server
scripts/runner-toolchain/with-cargo.sh clippy -p crossword-server
```

It temporarily repoints the two `panel-kit` path deps, puts this directory first
on `PATH` so `cc`/`gcc`/`c++`/`g++` resolve to the wrapper, sets
`ORT_LIB_LOCATION`/`ORT_SKIP_DOWNLOAD`, runs cargo, and restores
`client/web/Cargo.toml` and `client/Cargo.lock` on exit via a trap. The tree is
clean afterwards whatever cargo does.

## The default `cc`

`/paperclip/bin` is **first on `PATH` in every session** and is a writable
Longhorn PVC, not an image layer — so repointing `cc` there is both effective
and persistent, without rebuilding the runner image. `zigcc` itself is left on
disk untouched, which keeps the change reversible:

```bash
scripts/runner-toolchain/install-runner-cc.sh            # install (idempotent)
scripts/runner-toolchain/install-runner-cc.sh --check    # assert, for CI or a wake
scripts/runner-toolchain/install-runner-cc.sh --uninstall # back to zigcc
```

It installs `cc`, `gcc`, `c++` and `g++` as symlinks to the wrapper this repo
owns, so the behaviour agents get is defined by a committed file rather than by
whatever a previous run left in `~/.cache/runner-toolchain` — that directory is
not on `PATH` and is not in disk-guard's `safe_caches`, so it is exactly the kind
of state that silently disappears and then has to be rediscovered. It refuses to
clobber a `cc` it did not install, so a future real distro `gcc` or a proper
runner-side fix is not silently undone.

Verified after install, in a shell with no `export` and a stripped environment:

```console
$ env -i HOME=/paperclip PATH=/paperclip/bin:/paperclip/.cargo/bin:/usr/bin:/bin bash -c 'cc -c g.c -o g.o && cc g.c -o g && ./g'
COMPILE OK
LINK+RUN OK

$ readlink -f "$(command -v cc)"
/…/repo/scripts/runner-toolchain/cc

$ cd client && cargo nextest run -p crossword-server
     Summary [   4.815s] 117 tests run: 117 passed, 15 skipped
```

### Not covered: `bash -l`

`/paperclip/.profile` is a login-shell file that resets `PATH` from scratch, so a
**login** shell loses `/paperclip/bin` entirely. Agent sessions and cargo do not
run as login shells, so this does not affect the toolchain; it only matters if you
reproduce the above with `bash -lc`, where `cc` will not be found. Use `bash -c`.

## Why each piece exists

### 1. `cc` — the actual bug

`/paperclip/bin/cc` was a symlink to `/paperclip/bin/zigcc`, which appends
`-l:libstdc++.so.6 -lm -ldl -lpthread` **unconditionally**. Zig then attempts a
link even for `-c`, and every C dependency in the workspace fails:

```
ld.lld: error: undefined symbol: main
ld.lld: error: attempted static link of dynamic object /usr/lib/x86_64-linux-gnu/libstdc++.so.6
```

Note that *linking* is unaffected — `cc main.c -o prog` always worked — so this
only bites anything that compiles without linking, which is most C build
dependencies. There is no distro `gcc` in the container at all, so `zigcc` was
the only compiler and the broken one was the only one.

`./cc` is `zig cc` with those flags appended **only when actually linking**, so
`-c`, `-S` and `-E` compile clean. It also maps host triples onto triples zig
accepts:

| Incoming | Passed to zig | Why |
|---|---|---|
| `--target=x86_64-unknown-linux-gnu` | `x86_64-linux-gnu` | what cargo passes on Linux |
| `--target=wasm32-unknown-unknown` | `wasm32-freestanding-musl` | zig rejects `*-unknown-*` outright: `unable to parse target query … UnknownOperatingSystem` |

For the wasm mapping the glibc/libstdc++ flags are also dropped: they are x86_64
host libraries and must never reach a wasm link. `client/flake.nix:126,226` builds
`crossword-web` for `wasm32-unknown-unknown`, so this path is real.

Set `ZIG=/path/to/zig` if zig is not at one of the probed locations.

`./test-cc.sh` guards all of it: `-c` does not link, a real link still works and
its binary runs (i.e. the link flags were not simply dropped), the host target
maps, and the wasm target compiles without host libs leaking in.

### 2. `panel-kit` — the workspace manifest

`client/web/Cargo.toml` depends on `panel-kit` and `panel-kit-core` by absolute
path:

```toml
panel-kit = { path = "/home/olive/Repositories/panel-kit" }
panel-kit-core = { path = "/home/olive/Repositories/panel-kit/crates/panel-kit-core" }
```

That path is the release-plz host's checkout and does not exist elsewhere, so
cargo fails at manifest load, before it resolves anything:

```
failed to read /home/olive/Repositories/panel-kit/Cargo.toml: No such file or directory (os error 2)
```

`client/flake.nix` solves this for nix builds by vendoring the pinned
`panel-kit` input into the workspace source and rewriting the paths.
`fetch-panel-kit.sh` is the same idea for a bare container: it checks out
panel-kit at **the revision `client/flake.nix` pins** — read out of the flake,
never hardcoded here, so the runner and the nix build cannot drift — and
`with-cargo.sh` repoints both path deps at that checkout.

This is what makes `crossword-web` type-checkable here at all:

```console
$ scripts/runner-toolchain/with-cargo.sh check -p crossword-web
panel-kit: ~/.cache/runner-toolchain/panel-kit-4aad83c86285c83706e380c054a2b317ff8c6f69
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 32.09s
```

The checkout is cached per revision under `~/.cache/runner-toolchain` (~11M) and
reused. Set `PANEL_KIT_CACHE_DIR` to move it; the first run fetches, later runs
cost nothing.

`panel-kit-shim/` remains as an **offline fallback**: when the fetch fails,
`with-cargo.sh` falls back to it so the native server crates still build, and
says so on stderr. It is a manifest-only `panel-kit` 1.0.0 + `panel-kit-core`
1.0.0 pair with empty `lib.rs` files. **It is not panel-kit** — `crossword-web`
and `crossword-desktop` do not compile against it, and the fallback prints that
warning precisely so a shim build is never mistaken for a real frontend
verification.

`./test-panel-kit.sh` guards all of this: the resolved rev equals the flake's
pin, the checkout is panel-kit and not the shim, the repoint happens while
cargo runs and the host path does not survive it, and the tree is clean after.

`[patch]` in `.cargo/config.toml` is not an alternative here: cargo fails while
loading the manifest, before patch resolution runs.

### 3. `ORT_LIB_LOCATION`

`ort` static-links the vendored `libonnxruntime.a`. Without pointing at it the
link fails with `undefined symbol: OrtGetApiBase`. `with-cargo.sh` defaults it to
`/paperclip/ort-lib`; the nix shell sets it to the FOD output (see
`client/flake.nix`).

## Not covered

`nix develop ./client` does not work in this container — it fails building
bootstrap dependencies:

```
error: builder for '.../mescc-tools-1.9.1.drv' failed with exit code 1;
       > error: executing '.../kaem-unwrapped-1.9.1': No such file or directory
```

So the full dev shell is unavailable, and two things stay unbuildable here:

- **`crossword-desktop`** — needs GTK/WebKit (`glib-sys`, `gio-sys` … fail at
  `pkg-config`; the dev headers are not in the container). It gets past
  panel-kit and stops at GTK.
- **`dx`** — the Dioxus CLI is in the dev shell only, so there is no local
  `dx serve` / bundle build. `cargo check`/`cargo test -p crossword-web` do run
  and cover the frontend's Rust.

`crossword-server`, `-core`, `-db`, `-auth`, `-events`, `-tools` and
`crossword-web` all check and test through this toolchain — 166 tests pass with
it.

`crossword-server` additionally passes on the runner with **only** the `cc` fix
installed (117 tests), because `web/Cargo.toml` now pins `panel-kit` by git tag
instead of by host path. That is the state `install-runner-cc.sh` gets you; the
`with-cargo.sh` wrapper is only needed for the crates that still need `ORT` or a
panel-kit checkout.

Tracked as **DEF-325** (wiring the wrapper in as the default `cc`); the wrapper
and the panel-kit/ORT workarounds are **DEF-244**.
