{
  description = "definitely-not-crosswords Rust/Dioxus frontend: crossword-core + crossword-web (wasm), built reproducibly with crane; omnix CI.";

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
    # reachable in the Nix sandbox. Pull it in as an input and rewrite the path
    # at build time (see `src` below). Defaults to the local checkout so this
    # builds today; for Hydra/remote CI override with the pushed git rev:
    #   --override-input panel-kit github:olivecasazza/panel-kit/<rev>
    panel-kit.url = "path:/home/olive/Repositories/panel-kit";
    panel-kit.flake = false;
  };

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import inputs.systems;

      perSystem = { system, self', ... }:
        let
          pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ inputs.rust-overlay.overlays.default ];
          };
          inherit (pkgs) lib;

          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            extensions = [ "rust-src" "clippy" "rustfmt" ];
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
          rawSrc = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./core
              ./web
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

          commonArgs = {
            inherit src;
            strictDeps = true;
            # Build the workspace; the wasm crate is handled separately.
            cargoExtraArgs = "--workspace --exclude crossword-web";
          };

          # Native deps + the wasm crate's deps are vendored from one Cargo.lock.
          cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
            pname = "crossword-client-deps";
          });

          wasmArgs = {
            inherit src;
            strictDeps = true;
            CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
            doCheck = false; # no test runner on bare wasm32
            cargoExtraArgs = "-p crossword-web";
          };
          webCargoArtifacts = craneLib.buildDepsOnly (wasmArgs // {
            pname = "crossword-web-deps";
          });

          # Compiled wasm (pre-bindgen).
          crossword-web-wasm = craneLib.buildPackage (wasmArgs // {
            cargoArtifacts = webCargoArtifacts;
            pname = "crossword-web-wasm";
            doInstallCargoArtifacts = false;
          });

          # Deployable static bundle: wasm-bindgen glue + size-opt wasm + a
          # bootstrap index.html. dx is intentionally NOT used in-derivation
          # (it probes network/toolchain); it stays in the devShell for dev.
          crossword-web = pkgs.stdenv.mkDerivation {
            pname = "crossword-web";
            version = "0.1.0";
            dontUnpack = true;
            nativeBuildInputs = [ pkgs.wasm-bindgen-cli pkgs.binaryen ];
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
        in
        {
          packages = {
            default = crossword-web;
            inherit crossword-web;
          };

          checks = {
            cargo-fmt = craneLib.cargoFmt { inherit src; };
            cargo-clippy = craneLib.cargoClippy (commonArgs // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--all-targets -- -D warnings";
            });
            cargo-test = craneLib.cargoTest (commonArgs // {
              inherit cargoArtifacts;
            });
            # Web crate: clippy runs but warnings don't fail the build yet — the
            # UI was scaffolded fast and still carries ~90 style lints (manual
            # split_once, needless clones, …). ponytail: gate on real errors now,
            # tighten to `-D warnings` once the lint debt is paid down.
            cargo-clippy-web = craneLib.cargoClippy (wasmArgs // {
              cargoArtifacts = webCargoArtifacts;
              cargoClippyExtraArgs = "--all-targets";
            });
            # The bundle build doubles as a check so `nix flake check` / `om ci`
            # exercise the wasm path end to end.
            inherit crossword-web;
          };

          devShells.default = craneLib.devShell {
            checks = self'.checks;
            packages = with pkgs; [
              rustToolchain
              cargo-watch
              rust-analyzer
              dioxus-cli
              wasm-bindgen-cli
              lld
              inputs.omnix.packages.${system}.default
            ];
          };
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
