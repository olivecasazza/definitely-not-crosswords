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

    # The web crate depends on panel-kit by absolute host path, which isn't
    # reachable in the Nix sandbox. Pull it in as an input (pinned to the pushed
    # rev so Hydra/buildbot can fetch it too) and rewrite the path at build time
    # (see `src` below). For local panel-kit iteration, override:
    #   --override-input panel-kit path:/home/olive/Repositories/panel-kit
    panel-kit.url = "github:olivecasazza/panel-kit/dac9f0061b73fa8fdb554a9575985a413facaebb";
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

          # Workspace source. `web/Cargo.toml` pins panel-kit by absolute host
          # path, unreachable in the sandbox. Vendor the `panel-kit` input INTO
          # the source at a relative path and exclude it from this workspace
          # (it's its own workspace — excluding avoids a nested-workspace clash),
          # then rewrite the dep to point there. A relative path keeps the
          # Cargo.toml free of store-path string references (which crane rejects).
          # No working-tree edit — the committed Cargo.toml keeps the absolute path.
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
            # repoint the dep at the vendored copy (relative path)
            sed -i -E 's#path = "/home/olive/Repositories/panel-kit"#path = "../vendor-panel-kit"#' \
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
              mkdir -p $out
              wasm=$(find ${crossword-web-wasm} -name '*.wasm' | head -n1)
              [ -n "$wasm" ] || { echo "no .wasm in ${crossword-web-wasm}" >&2; exit 1; }
              wasm-bindgen --target web --no-typescript \
                --out-dir $out --out-name crossword-web "$wasm"
              wasm-opt -Oz -o $out/crossword-web_bg.wasm $out/crossword-web_bg.wasm || true
              cat > $out/index.html <<'HTML'
              <!doctype html><html><head><meta charset="utf-8" />
              <meta name="viewport" content="width=device-width, initial-scale=1" />
              <title>definitely-not-crosswords</title></head>
              <body><div id="main"></div>
              <script type="module">import init from "./crossword-web.js"; init();</script>
              </body></html>
              HTML
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
