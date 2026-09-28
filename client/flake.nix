{
  description = "definitely-not-crosswords Rust/Dioxus frontend: crossword-core + crossword-web (wasm), built reproducibly with crane; omnix CI.";

  # NOTE: `src` is a derivation (it vendors panel-kit in), so crane reads its
  # Cargo manifests via import-from-derivation. `nix build`/Hydra allow IFD by
  # default; only `nix flake check`'s pure-eval mode blocks it — run it with
  # `--option allow-import-from-derivation true` locally.

  nixConfig = {
    extra-substituters = [
      "https://nix-community.cachix.org"
      "https://crane.cachix.org"
    ];
    extra-trusted-public-keys = [
      "nix-community.cachix.org-1:mB9FSh9qf2dCimDSUo8Zy7bkq5CX+/rkCWyvRCYg3Fs="
      "crane.cachix.org-1:8Scfpmn9w+hGdXH/Q9tTLiYAE/2dnJYRJP7kl80GuRk="
    ];
  };

  inputs = {
    # Pinned to the exact nixpkgs (unstable) rev panel-kit builds against, which
    # ships wasm-bindgen-cli 0.2.121 — it MUST equal the `=0.2.121` wasm-bindgen
    # crate pin in web/Cargo.toml or the bundle fails to load. (nixos-25.05 ships
    # 0.2.100, which mismatches.)
    nixpkgs.url = "github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb";
    flake-parts.url = "github:hercules-ci/flake-parts";
    systems.url = "github:nix-systems/default";
    crane.url = "github:ipetkov/crane";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    omnix.url = "github:juspay/omnix";

    # The web crate depends on panel-kit and panel-kit-core by the absolute
    # release-plz host path, which isn't reachable in the Nix sandbox. Pull the
    # repository in as one input (pinned only to a pushed rev so Hydra can fetch
    # it) and rewrite both path deps at build time (see `src` below). Local macOS
    # verification uses the synced fork without changing this committed pin:
    #   --override-input panel-kit path:/Users/casazza/Repositories/olivecasazza/panel-kit
    panel-kit.url = "github:olivecasazza/panel-kit/4aad83c86285c83706e380c054a2b317ff8c6f69";
    panel-kit.flake = false;
  };

  outputs =
    inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import inputs.systems;

      perSystem =
        { system, self', ... }:
        let
          pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ inputs.rust-overlay.overlays.default ];
          };
          inherit (pkgs) lib;

          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            extensions = [
              "rust-src"
              "clippy"
              "rustfmt"
            ];
            targets = [ "wasm32-unknown-unknown" ];
          };
          craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;

          # Workspace source. `web/Cargo.toml` pins panel-kit and its core crate
          # by the absolute release-plz host path, unreachable in the sandbox.
          # Vendor the one `panel-kit` input INTO the source at a relative path
          # and exclude it from this workspace (it's its own workspace —
          # excluding avoids a nested-workspace clash), then rewrite both deps
          # to point there. A relative path keeps Cargo.toml free of store-path
          # string references (which crane rejects). No working-tree edit — the
          # committed Cargo.toml keeps the release-plz-compatible absolute path.
          # Ship every workspace member so cargo can resolve the workspace; each
          # nix build below compiles just one crate (-p ...). All of web, desktop,
          # server, and tools are built as packages; backend/desktop must be
          # present for workspace resolution regardless.
          rawSrc = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./core
              ./web
              # desktop crate source. Its frontend bundle (desktop/dist) is
              # gitignored — absent from the flake source — and the flake
              # repopulates it from the crossword-web derivation at build time.
              ./desktop
              ./backend
            ];
          };
          src = pkgs.runCommand "crossword-client-src" { } ''
            cp -r ${rawSrc} $out
            chmod -R +w $out
            cp -r ${inputs.panel-kit} $out/vendor-panel-kit
            chmod -R +w $out/vendor-panel-kit
            # exclude the vendored copy from this workspace
            sed -i -E 's#(members = \[.*\])#\1\nexclude = ["vendor-panel-kit"]#' $out/Cargo.toml
            # repoint both panel-kit path deps at the vendored copy
            sed -i -E 's#/home/olive/Repositories/panel-kit#../vendor-panel-kit#g' \
              $out/web/Cargo.toml
          '';

          # Vendor deps from the plain lockfile (a real path), NOT from the
          # runCommand-built `src` derivation — reading the lock out of a
          # derivation at eval time is import-from-derivation, which pure eval
          # (the github panel-kit input) forbids. Explicit pname/version below
          # keep crane from import-from-deriving the crate name out of `src` too.
          cargoVendorDir = craneLib.vendorCargoDeps { src = ./.; };

          # onnxruntime for the generator's `ort` crate. ort rc.12's pregenerated
          # bindings target onnxruntime 1.24.2; nixpkgs only has 1.22.2 (ABI
          # mismatch), so we vendor the exact MS prebuilt `ort` would otherwise
          # download — a raw-LZMA2 tarball of a static libonnxruntime.a. ort-sys
          # then static-links it via ORT_LIB_LOCATION, with no network/openssl.
          ortLib = pkgs.stdenvNoCC.mkDerivation {
            pname = "onnxruntime-ort-prebuilt";
            version = "1.24.2";
            src = pkgs.fetchurl {
              url = "https://cdn.pyke.io/0/pyke:ort-rs/ms@1.24.2/x86_64-unknown-linux-gnu.tar.lzma2";
              hash = "sha256-rMHLp5wzdZTq0diMpyUWFHqmAFTIQhe1M5mjHKpbpnE=";
            };
            nativeBuildInputs = [ pkgs.python3 ];
            dontUnpack = true;
            buildPhase = ''
              mkdir -p $out/lib
              python3 -c "import lzma,tarfile,io; raw=lzma.decompress(open('$src','rb').read(), format=lzma.FORMAT_RAW, filters=[{'id':lzma.FILTER_LZMA2,'dict_size':1<<26}]); tarfile.open(fileobj=io.BytesIO(raw)).extractall('$out/lib')"
            '';
            dontInstall = true;
          };
          ortEnv = {
            ORT_LIB_LOCATION = "${ortLib}/lib";
            ORT_SKIP_DOWNLOAD = "1";
          };

          commonArgs = {
            inherit src cargoVendorDir;
            pname = "crossword-client";
            version = "0.1.0";
            strictDeps = true;
            # Pure native crates that build in a bare sandbox (no onnxruntime, no
            # GTK). crossword-web is wasm (separate), crossword-server needs
            # onnxruntime (Phase F), crossword-desktop needs WebKit (its own pkg).
            cargoExtraArgs = "-p crossword-core -p crossword-db -p crossword-auth -p crossword-events";
          };

          # Native deps + the wasm crate's deps are vendored from one Cargo.lock.
          cargoArtifacts = craneLib.buildDepsOnly (
            commonArgs
            // {
              pname = "crossword-client-deps";
            }
          );

          wasmArgs = {
            inherit src cargoVendorDir;
            pname = "crossword-web";
            version = "0.1.0";
            strictDeps = true;
            CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
            doCheck = false; # no test runner on bare wasm32
            cargoExtraArgs = "-p crossword-web";
          };
          webCargoArtifacts = craneLib.buildDepsOnly (
            wasmArgs
            // {
              pname = "crossword-web-deps";
            }
          );

          # Compiled wasm (pre-bindgen).
          crossword-web-wasm = craneLib.buildPackage (
            wasmArgs
            // {
              cargoArtifacts = webCargoArtifacts;
              pname = "crossword-web-wasm";
              doInstallCargoArtifacts = false;
            }
          );

          # Deployable static bundle: wasm-bindgen glue + size-opt wasm + a
          # bootstrap index.html. dx is intentionally NOT used in-derivation
          # (it probes network/toolchain); it stays in the devShell for dev.
          crossword-web = pkgs.stdenv.mkDerivation {
            pname = "crossword-web";
            version = "0.1.0";
            dontUnpack = true;
            nativeBuildInputs = [
              pkgs.wasm-bindgen-cli
              pkgs.binaryen
            ];
            buildPhase = ''
              stage=$(mktemp -d)
              wasm=$(find ${crossword-web-wasm} -name '*.wasm' | head -n1)
              [ -n "$wasm" ] || { echo "no .wasm in ${crossword-web-wasm}" >&2; exit 1; }
              wasm-bindgen --target web --no-typescript \
                --out-dir $stage --out-name crossword-web "$wasm"
              wasm-opt -Oz -o $stage/crossword-web_bg.wasm $stage/crossword-web_bg.wasm || true

              # Content-address the WHOLE bundle under one immutable prefix.
              #
              # The previous scheme put a ?v=<hash> query on the glue and the
              # wasm only. But the glue imports its wasm-bindgen snippets by
              # HARDCODED RELATIVE PATH with no query — ./snippets/<crate-hash>/…
              # — and that crate-hash tracks the crate version, not the build. So
              # nothing could bust them: an edge holding snippets/* from an older
              # release pairs them with the new glue and the app dies at import
              # ("does not provide an export named get_select_data") → blank page.
              # That took prod down. Cloudflare also caches a separate variant per
              # content-encoding, so the poisoned copy was the zstd one Chrome
              # negotiates while identity/gzip/br all looked healthy to curl.
              #
              # Relocating the glue relocates its ./snippets/… too, so a release
              # can only ever serve a matched set; a stale object isn't evicted,
              # its URL is simply never requested again. Identical bundles keep
              # the same URL, so a rollback reuses a warm cache — safe precisely
              # because bytes at a hashed URL never change.
              bundleHash=$(cd $stage && find . -type f | LC_ALL=C sort \
                | xargs sha256sum | sha256sum | cut -c1-16)

              # Guard: every glue import must be relative and resolve inside the
              # bundle. If a future wasm-bindgen/dioxus emits an absolute or
              # out-of-tree reference the content hash can't cover it — fail the
              # build rather than ship another un-bustable asset.
              bad=$(grep -oE "from '[^']+'" $stage/crossword-web.js \
                | sed "s/from '//; s/'$//" | grep -v '^\./' || true)
              [ -z "$bad" ] || { echo "non-relative import in glue: $bad" >&2; exit 1; }
              for spec in $(grep -oE "from '\./[^']+'" $stage/crossword-web.js \
                  | sed "s|from './||; s|'$||"); do
                [ -f "$stage/$spec" ] || { echo "glue imports missing file: $spec" >&2; exit 1; }
              done

              mkdir -p $out/_assets
              cp -r $stage $out/_assets/$bundleHash

              # Self-contained boot shell (DEF-148, spec DEF-146). The heredoc is
              # QUOTED (<<'HTML') so the shell does not expand the JS template literals
              # below; the bundle hash is a placeholder that sed substitutes afterwards.
              # An unquoted heredoc eats dollar-brace expansions, backticks and command
              # substitutions, and ships a 0-byte index.html. The doubled quote-brace on
              # the glue line is the Nix escape for a literal dollar-brace, not a typo.
              cat > $out/index.html <<'HTML'
              <!doctype html><html><head><meta charset="utf-8" />
              <meta name="viewport" content="width=device-width, initial-scale=1" />
              <meta name="color-scheme" content="dark light" />
              <!-- Scripting disabled: #boot can never run, so it would sit on "Loading" forever
                   and cover this page. Hide it so the <noscript> card below is what is read.
                   Parsed only when scripting is off, so the JS path is untouched. -->
              <noscript><style>#boot{display:none!important}</style></noscript>
              <title>definitely-not-crosswords</title>
              <style>
              /* BOOT CSS - pre-wasm only. panel_kit::CSS and styles::DESIGN are injected by the
                 app (client/web/src/main.rs:107-108), so none of the tokens or atoms below exist
                 until the app mounts. These values are copied from styles.rs and MUST move
                 with them: --bg-app:14 --bg-card:15 --text-primary:18 --text-secondary:19
                 --border-app:20 --pastel-red:22 | light: --bg-app:104 --bg-card:105
                 --text-primary:108 --text-secondary:112 --border-app:117 --pastel-red:122.
                 Nothing here may be relied on after boot; #boot is removed at handoff. */
              :root{--b-bg:#121212;--b-card:#18181b;--b-fg:#f4f4f5;--b-dim:#a1a1aa;--b-line:#27272a;--b-err:#ff8c8c;--b-primary:#feea99}
              .light-mode{--b-bg:#ededf0;--b-card:#f7f7f8;--b-fg:#18181b;--b-dim:#52525b;--b-line:#8a8a93;--b-err:#b02a20;--b-primary:#775600}
              html,body{height:100%}
              body{margin:0;background:var(--b-bg);color:var(--b-fg);
                font:500 .875rem/1.6 Montserrat,system-ui,-apple-system,'Segoe UI',Roboto,sans-serif}
              #boot{position:fixed;inset:0;z-index:400;display:flex;align-items:center;justify-content:center;
                padding:max(1.5rem,env(safe-area-inset-top)) max(1.5rem,env(safe-area-inset-right))
                       max(1.5rem,env(safe-area-inset-bottom)) max(1.5rem,env(safe-area-inset-left))}
              .boot-card{width:100%;max-width:26rem;background:var(--b-card);border:1px solid var(--b-line);
                padding:1.25rem;display:flex;flex-direction:column;gap:.75rem}
              .boot-mark{margin:0;font:700 .625rem/1.2 Inconsolata,ui-monospace,monospace;
                letter-spacing:.05em;text-transform:uppercase;color:var(--b-dim)}
              .boot-title{margin:0;font-size:1rem;font-weight:700}
              .boot-title:focus{outline:none}
              .boot-body{margin:0;font-size:.75rem;color:var(--b-dim)}
              .boot-err{margin:0;font:700 .75rem/1.4 Inconsolata,ui-monospace,monospace;color:var(--b-err);
                overflow-wrap:anywhere}
              .boot-actions{display:flex;gap:.5rem;flex-wrap:wrap}
              /* Border is --b-dim, not --b-line: a control boundary needs 3:1 and --b-line is
                 1.19:1 on the dark card. --b-dim is 6.91:1 dark / 7.22:1 light. */
              .boot-btn{font:600 .875rem/1 inherit;padding:.6rem .9rem;min-height:2.75rem;cursor:pointer;
                background:var(--b-card);color:var(--b-dim);border:1px solid var(--b-dim);text-decoration:none}
              .boot-btn:hover,.boot-btn:focus-visible{color:var(--b-fg);border-color:var(--b-fg)}
              .boot-btn:focus-visible{outline:2px solid var(--b-fg);outline-offset:2px}
              /* The one action that can fix this: the house .app-btn-active idiom
                 (styles.rs:221), 14.7:1 dark / 6.3:1 light. */
              .boot-btn-primary{color:var(--b-fg);border-color:var(--b-primary)}
              /* Scoped to the boot card: an author rule beating the UA [hidden] rule is why
                 !important is needed, and a global one would reach into app styles post-boot. */
              #boot [hidden],noscript [hidden]{display:none !important}
              </style>
              </head>
              <body>
              <div id="main"></div>
              <div id="boot" role="status" aria-live="polite">
                <div class="boot-card">
                  <p class="boot-mark">definitely-not-crosswords</p>
                  <h1 class="boot-title" id="boot-title" tabindex="-1">Loading</h1>
                  <p class="boot-body" id="boot-body">Fetching the app&#8230;</p>
                  <p class="boot-err" id="boot-err" hidden></p>
                  <div class="boot-actions" id="boot-actions" hidden>
                    <button class="boot-btn boot-btn-primary" id="boot-retry" type="button">Retry</button>
                    <button class="boot-btn" id="boot-reload" type="button">Reload</button>
                  </div>
                </div>
              </div>
              <noscript><div class="boot-card" style="margin:4rem auto;max-width:26rem">
                <p class="boot-mark">definitely-not-crosswords</p>
                <h1 class="boot-title">JavaScript is required</h1>
                <p class="boot-body">This is a WebAssembly app, so it needs JavaScript enabled to run.</p>
              </div></noscript>
              <script type="module">
              // Pre-boot loader. Runs BEFORE the wasm exists, so nothing here may reference
              // the app's stylesheet — panel_kit::CSS and styles::DESIGN are injected by
              // client/web/src/main.rs only after the app mounts.
              const $ = (id) => document.getElementById(id);
              const setTitle = (t) => { const n = $("boot-title"); if (n) n.textContent = t; };
              const setBody = (t) => { const n = $("boot-body"); if (n) n.textContent = t; };
              const showActions = (on) => {
                for (const id of ["boot-err", "boot-actions"]) { const n = $(id); if (n) n.hidden = !on; }
              };
              const delay = (ms) => new Promise((r) => setTimeout(r, ms));

              // Theme continuity: main.rs:83-88 reads the same key from inside the app, so
              // setting the class here (before first paint) just removes the dark flash.
              try { if (localStorage.getItem("theme") === "light")
                      document.documentElement.classList.add("light-mode"); } catch {}

              let tries = 0, tSlow = 0, tFail = 0, gen = 0, booted = false;
              const mark = (state) => { const b = $("boot"); if (b) b.dataset.state = state; };

              // Arm the two timers for the CURRENT attempt. `g` is the attempt's generation:
              // a newer boot() bumps `gen`, so a slow timer or a 20s timeout belonging to an
              // abandoned attempt can no longer touch the card.
              function arm(g) {
                clearTimeout(tSlow); clearTimeout(tFail);
                tSlow = setTimeout(() => {
                  const b = $("boot");
                  if (g === gen && b && (b.dataset.state === "boot" || b.dataset.state === "retrying"))
                    setBody("Still loading — this takes a moment on a slow connection.");
                }, 4000);
                tFail = setTimeout(() => { if (g === gen) fail("Timed out after 20s"); }, 20000);
              }

              function fail(reason) {
                if (!$("boot")) return;                 // handed off already: never touch a detached card
                clearTimeout(tSlow); clearTimeout(tFail);
                setTitle("Couldn't load the app");
                setBody("The app didn't download. That's on us, not your connection — try again.");
                const err = $("boot-err"); if (err) err.textContent = reason;
                showActions(true);
                $("boot").setAttribute("role", "alert");
                $("boot-retry").focus(); mark("failed");
              }

              function reset() {
                setTitle("Loading"); setBody("Fetching the app…"); showActions(false);
                $("boot").setAttribute("role", "status");
              }

              async function boot(auto) {
                const g = ++gen;
                reset();
                mark(auto ? "retrying" : "boot");
                arm(g);
                try {
                  // Exactly ONE automatic retry, per page load. ?r= busts a pinned edge 404
                  // (DEF-145) and is safe here because the path is content-addressed
                  // (client/flake.nix:219-232): the glue's own ./snippets/<crate-hash>/…
                  // imports resolve against the PATH, not the query. Verified: a relative
                  // import from a URL with ?r= resolves to the identical pathname. Do not
                  // reintroduce query params on anything but this one top-level glue URL.
                  const glue = `/_assets/__BUNDLE_HASH__/crossword-web.js''${auto || tries ? `?r=''${Date.now()}` : ""}`;
                  const mod = await import(glue);
                  await mod.default();
                } catch (e) {
                  if (g !== gen || booted) return;       // a newer attempt, or the app itself, owns the card now
                  if (!auto && tries++ < 1) { mark("retrying"); await delay(1500); return boot(true); }
                  fail(e && e.message ? e.message : String(e));
                }
              }

              $("boot-retry").addEventListener("click", () => boot(false));
              $("boot-reload").addEventListener("click", () => location.reload());

              // Handoff: #main is the app's mount root. #boot is a SIBLING of #main on
              // purpose — inside it, the canary's `#main is not empty` check would pass on a
              // failed boot. Fixed + full-bleed, so the app is already laid out behind the
              // card and removal causes no layout shift.
              new MutationObserver(() => {
                if ($("main").childElementCount) {
                  // Latch before removing: a late failure from the attempt that was in flight
                  // must not auto-retry and boot a SECOND copy of the app into #main.
                  booted = true;
                  clearTimeout(tSlow); clearTimeout(tFail);
                  const b = $("boot");
                  if (b) { b.remove(); mark("booted"); }
                }
              }).observe($("main"), { childList: true });

              boot(false);
              </script>
              </body></html>
              HTML
              sed -i "s|__BUNDLE_HASH__|$bundleHash|g" $out/index.html
            '';
            dontInstall = true;
          };

          # Desktop client: a Tauri v2 shell that loads the wasm bundle into a
          # native WebKit webview. Scoped to `-p crossword-desktop` so it never
          # compiles the onnxruntime/Postgres server. The GTK/WebKit stack is the
          # only extra over a plain Rust build.
          desktopNativeDeps = [ pkgs.pkg-config ];
          desktopBuildInputs = with pkgs; [
            webkitgtk_4_1
            libsoup_3
            gtk3
            glib
            cairo
            pango
            atk
            gdk-pixbuf
          ];
          desktopArgs = commonArgs // {
            pname = "crossword-desktop";
            cargoExtraArgs = "-p crossword-desktop";
            nativeBuildInputs = desktopNativeDeps;
            buildInputs = desktopBuildInputs;
          };
          desktopCargoArtifacts = craneLib.buildDepsOnly (
            desktopArgs
            // {
              pname = "crossword-desktop-deps";
            }
          );
          crossword-desktop = craneLib.buildPackage (
            desktopArgs
            // {
              cargoArtifacts = desktopCargoArtifacts;
              doInstallCargoArtifacts = false;
              # tauri-build embeds frontendDist (gitignored) — fill it from the bundle.
              # NOTE: this reuses the web bundle (relative API base); a functional
              # desktop build must rebuild the wasm with CROSSWORD_API_BASE set.
              preConfigure = ''
                mkdir -p desktop/dist
                cp -r ${crossword-web}/. desktop/dist/
              '';
            }
          );

          # The Axum backend (tRPC + auth + ONNX generator). Static-links the
          # vendored onnxruntime via ortEnv; the model/WordNet assets in `data/`
          # are provided at runtime (mounted), not baked into the binary.
          serverArgs =
            commonArgs
            // ortEnv
            // {
              pname = "crossword-server";
              cargoExtraArgs = "-p crossword-server";
            };
          serverCargoArtifacts = craneLib.buildDepsOnly (
            serverArgs
            // {
              pname = "crossword-server-deps";
            }
          );
          crossword-server = craneLib.buildPackage (
            serverArgs
            // {
              cargoArtifacts = serverCargoArtifacts;
              doInstallCargoArtifacts = false;
            }
          );

          # DB tooling (migrate + seed bins) — the Rust replacement for the Prisma
          # migrate + WordNet seed scripts. Pure sqlx/tokio, builds in a bare
          # sandbox. The migrate bin embeds backend/tools/migrations.
          toolsArgs = commonArgs // {
            pname = "crossword-tools";
            cargoExtraArgs = "-p crossword-tools";
          };
          toolsCargoArtifacts = craneLib.buildDepsOnly (
            toolsArgs
            // {
              pname = "crossword-tools-deps";
            }
          );
          crossword-tools = craneLib.buildPackage (
            toolsArgs
            // {
              cargoArtifacts = toolsCargoArtifacts;
              doInstallCargoArtifacts = false;
            }
          );
        in
        {
          packages = {
            default = crossword-web;
            inherit
              crossword-web
              crossword-desktop
              crossword-server
              crossword-tools
              ;
          };

          checks = {
            cargo-fmt = craneLib.cargoFmt {
              inherit src;
              pname = "crossword-client";
              version = "0.1.0";
            };
            cargo-clippy = craneLib.cargoClippy (
              commonArgs
              // {
                inherit cargoArtifacts;
                cargoClippyExtraArgs = "--all-targets -- -D warnings";
              }
            );
            cargo-test = craneLib.cargoTest (
              commonArgs
              // {
                inherit cargoArtifacts;
              }
            );
            # Web crate: clippy runs but warnings don't fail the build yet — the
            # UI was scaffolded fast and still carries ~90 style lints (manual
            # split_once, needless clones, …). ponytail: gate on real errors now,
            # tighten to `-D warnings` once the lint debt is paid down.
            cargo-clippy-web = craneLib.cargoClippy (
              wasmArgs
              // {
                cargoArtifacts = webCargoArtifacts;
                cargoClippyExtraArgs = "--all-targets";
              }
            );
            # The bundle build doubles as a check so `nix flake check` / `om ci`
            # exercise the wasm path end to end; the desktop build exercises the
            # Tauri/WebKit path; the server build exercises the ONNX/onnxruntime path.
            inherit
              crossword-web
              crossword-desktop
              crossword-server
              crossword-tools
              ;
          };

          devShells.default = craneLib.devShell (
            {
              checks = self'.checks;
              packages = with pkgs; [
                rustToolchain
                cargo-watch
                rust-analyzer
                dioxus-cli
                wasm-bindgen-cli
                lld
                inputs.omnix.packages.${system}.default
                # Desktop (Tauri) toolchain — `cargo-tauri` for `tauri dev/build`
                # plus the GTK/WebKit libs the native crate links against.
                cargo-tauri
                pkg-config
                webkitgtk_4_1
                libsoup_3
                gtk3
              ];
              # Local dev is the "local" environment: the server registers the
              # dev-admin bypass route and /api/config turns on its button.
              # staging/prod set APP_ENV via the Helm chart (default production).
              APP_ENV = "local";
              # So `cargo build -p crossword-server` finds the vendored onnxruntime
              # (no download-binaries, no network) inside `nix develop`.
            }
            // ortEnv
          );
        };

      # Hydra builds the `hydraJobs` output (NOT `checks`/`packages`), so the
      # nixlab Hydra jobset that points at this flake (gcp-hydra,
      # definitely-not-crosswords project, `dioxus-migration` jobset) would
      # build nothing without this. Surface every check (which already includes
      # the `crossword-web` bundle) as a Hydra job on the on-prem linux builders.
      flake.hydraJobs.x86_64-linux = inputs.self.checks.x86_64-linux;

      # `om ci run` builds every flake check + package across the configured
      # systems. The root subflake covers this flake.
      flake.om.ci.default.root = {
        dir = ".";
        steps.build.enable = true;
      };
    };
}
