# Runner toolchain for the `client/` workspace

A minimal shim that makes the native `client/` crates buildable in a container
that has no nix dev shell. This exists because `/paperclip/bin/cc` (the default
compiler in this runner) is broken for `-c`, and because `client/web/Cargo.toml`
pins `panel-kit` at an absolute host path.

`nix develop ./client` is the supported way to work on this repo and is
unaffected by any of this. Use the shim only when nix is unavailable or its
bootstrap fails in your environment.

## Usage

From anywhere in the repo:

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

## Why each piece exists

### 1. `cc` — the actual bug

`/paperclip/bin/cc` is a symlink to `/paperclip/bin/zigcc`, which appends
`-l:libstdc++.so.6 -lm -ldl -lpthread` **unconditionally**. Zig then attempts a
link even for `-c`, and every C dependency in the workspace fails:

```
ld.lld: error: undefined symbol: main
ld.lld: error: attempted static link of dynamic object /usr/lib/x86_64-linux-gnu/libstdc++.so.6
```

`./cc` is `zig cc` with those flags appended **only when actually linking**, so
`-c`, `-S` and `-E` compile clean. `gcc`, `c++` and `g++` are symlinks to it.
Set `ZIG=/path/to/zig` if zig is not at one of the probed locations.

`./test-cc.sh` guards this: it asserts `-c` does not link, and that a real link
still works.

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
