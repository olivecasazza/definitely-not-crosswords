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
    # flake-parts evaluates each option against the SAME attrset, but that
    # attrset is not `rec`, so two options cannot see each other by bare name
    # (`perSystem` referencing `packagesForImpl` is an "undefined variable",
    # not a typo). A `let` around the `mkFlake` application gives both wrappers
    # one shared scope — which is also what keeps the default outputs and
    # `packagesFor` provably identical in how they resolve a sha, since they
    # call the same function.
    let
      fp = inputs.flake-parts.lib;
      # flake-parts' own nixpkgs lib: `lib.genAttrs` shapes `flake.<name>`
      # exactly like its transposition module does.
      lib = inputs.flake-parts.inputs.nixpkgs-lib.lib;
      systems = import inputs.systems;

      # The commit this build came from, for `BUILD_SHA` (the server's
      # `/api/config.buildSha` and the bundle's `data-build`).
      #
      # THE CONTRACT (DEF-317). `packagesFor` is the provenance-aware entry
      # point; `perSystem` (i.e. `nix build ./client#…`) is the same code with
      # no caller-supplied sha. Resolution order, most trustworthy first:
      #
      #   1. `CROSSWORDS_BUILD_SHA` — an explicit override, for a direct
      #      `nix build ./client` from a shell that knows the commit.
      #   2. `fallbackBuildSha` — what the caller passed to `packagesFor`. The
      #      root flake passes its own `self.rev` here (DEF-317).
      #   3. `self.rev`, then `self.dirtyRev` — set only when THIS flake is
      #      itself evaluated from a git checkout.
      #
      # (3) is why (2) is needed. The ROOT flake consumes this one as
      # `client.url = "path:./client"`, and a `path:` input carries NEITHER
      # `rev` NOR `dirtyRev` — only `sourceInfo`/`narHash`. So under the root
      # flake, step (3) can never fire and any bare `client.packages.${system}`
      # yields "unknown" no matter how clean the checkout is. It is the CALLER
      # that knows the commit when the callee is a `path:` input. Verified in
      # isolation on nix 2.20.6 (two-flake repro, root + nested `path:./inner`):
      #
      #     root, clean checkout    -> self.rev = 7e1696f…, dirtyRev = ABSENT
      #     nested path:./inner     -> rev = false,     dirtyRev = false
      #     CROSSWORDS_BUILD_SHA set -> inner sees "abcdef1234567890"
      #
      # Threading it down fixes buildbot, `nix build .#dockerImage` and
      # `nix develop` in one place, and reduces ci.yaml's CROSSWORDS_BUILD_SHA
      # handoff to a redundant convenience rather than the load-bearing fix it
      # was (#241).
      #
      # `lib` is an argument, not captured from flake scope, so this stays a
      # plain function that `nix eval` can exercise on its own.
      resolveBuildSha = lib: fallback:
        let
          fromEnv = builtins.getEnv "CROSSWORDS_BUILD_SHA";
          fromSelf = inputs.self.rev or inputs.self.dirtyRev or "";
          raw = if fromEnv != "" then fromEnv else (if fallback != "" then fallback else fromSelf);
          # AC4 (DEF-317): never ship `<rev>-dirty` as a buildSha. A dirty tree
          # is NOT the commit it names, and `<rev>-dirty` can never equal the
          # `${TAG:0:12}` a deploy gate compares against, so shipping it would
          # just convert a loud failure into a 30-minute red poll. Strip the
          # suffix: the rev alone still names the base commit, which is the
          # most useful thing to report, and an unresolvable tree still lands on
          # "unknown" and is caught by the gates. Buildbot checks out a git ref,
          # so its trees are clean and this arm never fires there.
          stripped = lib.strings.removeSuffix "-dirty" raw;
        in
        if stripped == "" then "unknown" else builtins.substring 0 12 stripped;

      # The one real implementation. `fallbackBuildSha` is resolved by
      # `resolve`; everything else is the per-system package set.
      packagesForImpl = resolve: fallbackBuildSha: system:
        let
          pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ inputs.rust-overlay.overlays.default ];
          };
          inherit (pkgs) lib;
          buildSha = resolve lib fallbackBuildSha;

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
            BUILD_SHA = buildSha;
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
            BUILD_SHA = buildSha;
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

              # The link-preview card (DEF-190) joins the bundle BEFORE it is
              # hashed, and that ordering is load-bearing twice over.
              #
              # 1. It has to sit under /_assets to be reachable AT ALL. The
              #    server mounts exactly one static dir and answers every other
              #    path with the SPA fallback (backend/server/src/main.rs:157-196),
              #    which returns index.html as 200 text/html. A card at the dist
              #    root therefore "succeeds" with 200 and hands the scraper HTML
              #    instead of an image: measured on staging before this change,
              #    GET /og-image.png answered 200 text/html, 14923 bytes. That is
              #    worse than no tag, which is the failure this issue exists to
              #    prevent — so the card is placed where the server actually
              #    serves files from, not where it would be tidier.
              #
              # 2. Being inside the hash gives it the bundle's immutability
              #    guarantee for free: bytes at a hashed URL never change, so the
              #    edge's max-age=31536000 immutable stamp (assets.rs:50-61) can
              #    never pin a stale card, and editing the card changes the hash
              #    and busts every URL that points at it. A stable /og-image.png
              #    at the dist root could not have either property, and adding a
              #    cache-busting query to it is what DEF-140/145/152 exist to
              #    stop.
              #
              # Copied in as og.png (a fixed name) because the head references one
              # file; the content addressing is the hashed directory around it.
              # The source is a committed asset, not generated here: the build must
              # not depend on Pillow or a font, which are not in this closure.
              # scripts/make-og-image.py regenerates these bytes,
              # scripts/check-og-image.sh --regen asserts the committed file is
              # exactly what it produces, and scripts/verify-og-image-build.sh
              # runs the steps above and asserts the head advertises this path.
              cp ${./web/og-image.png} $stage/og.png

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
              <noscript><style>#boot{display:none!important}
              /* DEF-183 D5: .light-mode is set by the pre-boot script below, which by
                 definition does not run here, so a light-mode user with scripting off got
                 the dark card in every case. This override lives INSIDE <noscript>, so it
                 is parsed only when scripting is off and cannot fight the app. Values are
                 the .light-mode row of the boot palette below. */
              @media (prefers-color-scheme: light){
                body{--b-bg:#ededf0;--b-card:#f7f7f8;--b-fg:#18181b;--b-dim:#52525b;--b-line:#8a8a93;--b-err:#b02a20;--b-primary:#775600}}
              </style></noscript>
              <!-- Machine-readable copy (DEF-164). This <head> is the ONLY descriptive
                    content a crawler or a link-preview scraper ever sees: every word of
                    positioning and the price are rendered client-side by the WASM bundle,
                    after load, so the copy that decides whether anyone clicks has to live
                    here — in the build, not in the app. A <title> that is just the repo
                    slug is all a search result or a Discord/Slack/iMessage card can
                    carry. The description is the approved home.rs tagline, so the snippet
                    and the product cannot drift apart.

                    The plan tail is not decoration, it is the plan (DEF-202). The snippet is
                    the ONLY copy a searcher ever reads, so a tail of "Free to play." on its
                    own read as a description of a free product and was vaguer than what the
                    repo actually ships. "Free plan: $0, unlimited solving." is the Free row
                    of README.md "Plans & pricing" (55-62) verbatim, and it is 150 chars —
                    inside the ~160 a search engine will show. "Free to play." survives at
                    home.rs:150, where it is qualified on the same line by "No card
                    required." and is read next to the live pricing panel; truncated to
                    these 150 characters it loses that qualifier and becomes a claim about
                    the whole product. Change either half of this description only
                    together with the other, and only against README.md 55-62.

                    Two escapes bite in this block, and both are build breaks, not runtime
                    ones. buildPhase is a Nix indented string, so a dollar before an
                    opening brace is an antiquotation Nix evaluates as an expression
                    (DEF-163) — which is how this very sentence has to escape it. The
                    heredoc is quoted, so the shell leaves the JS template literals alone,
                    but Nix still parses the string. The copy below contains none of them —
                    keep it that way.

                    rel=canonical and og:url carry the __ORIGIN__ placeholder, substituted by
                    the server when it reads index.html at startup (spa.rs + origin.rs) rather
                    than named here. There is no build-time origin to name: one image is built
                    and deployed to staging AND production (ci.yaml pushes :latest and :<sha>
                    to a single GAR repository, and both releases pull the same tag), so a
                    literal written here is one host's answer baked into an artifact the other
                    host also serves.

                    And rel=canonical is a per-URL assertion about where THIS page is
                    canonically located, not a hint that consolidates two hosts onto one.
                    Naming production is correct on production and simply false on staging,
                    where it asserts that staging is a duplicate of production — a
                    consolidation signal pointing the wrong way for any future decision to
                    surface staging, and inert on production. The version of this comment
                    that argued the opposite ("a canonical pointing at the preferred copy is
                    correct from both") was wrong; one artifact is two answers, and each host
                    has to give its own.

                    Staging stays noindex, nofollow via X-Robots-Tag either way, so this is a
                    correctness fix, not a leak fix. Deliberately no <meta name="robots">: a
                    client-side index would fight the server header that closes staging.

                    og:image points at the bundle's OWN hashed directory, and it must be an
                    absolute URL: a scraper resolves og:image against the page it found. It
                    takes __ORIGIN__ for the same reason canonical does — the origin this
                    build is served from is the only one whose /_assets actually contains
                    it. Its path therefore repeats the __BUNDLE_HASH__ placeholder the glue
                    line below uses, and the same single sed substitutes both, so the card
                    can never be advertised at a hash the bundle is not actually served
                    from. Note the two substitutions are different mechanisms and both are
                    needed: the sed runs in THIS build, the __ORIGIN__ one runs when the
                    server reads the finished index.html.

                    og:image is inside /_assets because that is the only prefix the server
                    serves as files; anything else resolves to the SPA fallback and answers
                    200 text/html (see the copy step in this buildPhase). og:image:alt is not
                    decoration: it is the text a scraper shows when it cannot render the
                    image at all, so it must read as the product rather than describe a file
                    ("1200x630 PNG" is the failure mode). It repeats the tagline and the Free
                    plan claim already made above, in that order.

                    twitter:card is summary_large_image now that there is an image to fill
                    the large box. Before DEF-190 it was summary, and deliberately so: with
                    no og:image, summary_large_image reserves a large empty area and previews
                    WORSE than a compact text card. The tag, the image and the card type move
                    together in one change; splitting them re-creates the empty box. -->
              <title>definitely-not-crosswords — free real-time co-op crosswords</title>
              <meta name="description" content="Cooperative, real-time crosswords. Solve the same grid together, see every move as it happens, and finish as a team. Free plan: $0, unlimited solving." />
              <link rel="canonical" href="__ORIGIN__/" />
              <meta property="og:type" content="website" />
              <meta property="og:site_name" content="definitely-not-crosswords" />
              <meta property="og:title" content="definitely-not-crosswords — free real-time co-op crosswords" />
              <meta property="og:description" content="Cooperative, real-time crosswords. Solve the same grid together, see every move as it happens, and finish as a team. Free plan: $0, unlimited solving." />
              <meta property="og:url" content="__ORIGIN__/" />
              <meta property="og:image" content="__ORIGIN__/_assets/__BUNDLE_HASH__/og.png" />
              <meta property="og:image:width" content="1200" />
              <meta property="og:image:height" content="630" />
              <meta property="og:image:alt" content="definitely-not-crosswords — free real-time co-op crosswords. Solve the same grid together, see every move as it happens. Free plan: $0, unlimited solving." />
              <meta name="twitter:card" content="summary_large_image" />
              <meta name="twitter:image" content="__ORIGIN__/_assets/__BUNDLE_HASH__/og.png" />
              <meta name="twitter:image:alt" content="definitely-not-crosswords — free real-time co-op crosswords. Solve the same grid together, see every move as it happens. Free plan: $0, unlimited solving." />
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
              body{margin:0;background:var(--b-bg);color:var(--b-fg)}
              /* DEF-183 D9: this shorthand was on `body`, and a font shorthand sets
                 font-weight:500 with it. panel-kit's `body,html,#main` rule and DESIGN's
                 `body` rule both override family/size/line-height but NOT weight, and this
                 <style> outlives the card (only #boot is removed at handoff) — so from this
                 release on, every element that does not set its own weight rendered at 500.
                 Scoped to the two surfaces that exist before the app mounts. */
              #boot,noscript .boot-card{font:500 .875rem/1.6 Montserrat,system-ui,-apple-system,'Segoe UI',Roboto,sans-serif}
              #boot{position:fixed;inset:0;z-index:400;display:flex;align-items:center;justify-content:center;
                padding:max(1.5rem,env(safe-area-inset-top)) max(1.5rem,env(safe-area-inset-right))
                       max(1.5rem,env(safe-area-inset-bottom)) max(1.5rem,env(safe-area-inset-left))}
              .boot-card{width:100%;max-width:26rem;background:var(--b-card);border:1px solid var(--b-line);
                padding:1.25rem;display:flex;flex-direction:column;gap:.75rem}
              /* DEF-183 D4: the JS boot card is centred by #boot's flex; this card sat in
                 normal flow at margin:4rem auto, so the two no-app cards disagreed (64px
                 above / 533px below at a 720px viewport). Full-bleed flex centring; the
                 card keeps its own max-width:26rem above. */
              .boot-noscript{position:fixed;inset:0;display:flex;align-items:center;justify-content:center;padding:1.5rem}
              .boot-mark{margin:0;font:700 .625rem/1.2 Inconsolata,ui-monospace,monospace;
                letter-spacing:.05em;text-transform:uppercase;color:var(--b-dim)}
              .boot-title{margin:0;font-size:1rem;font-weight:700}
              .boot-title:focus{outline:none}
              .boot-body{margin:0;font-size:.75rem;color:var(--b-dim);
                /* DEF-183 D3: the card is centre-anchored, so a row that grows moves the
                   whole card. Two lines is the longest copy the three states produce, and
                   this row INHERITS the card's 1.6 line-height, so a line here is
                   1.6 x .75rem = 2.4rem, not 1.4 x (that is .boot-err, below). */
                min-height:2.4rem}
              .boot-build{margin:0;font:500 .6875rem/1.4 Inconsolata,ui-monospace,monospace;color:var(--b-dim)}
              .boot-err{margin:0;font:700 .75rem/1.4 Inconsolata,ui-monospace,monospace;color:var(--b-err);
                overflow-wrap:anywhere;min-height:3.15rem}
              .boot-actions{display:flex;gap:.5rem;flex-wrap:wrap}
              /* DEF-183 D3: the error line and the action row are ALWAYS rendered and
                 toggled by visibility, so the space they need is reserved before the user
                 ever needs it (measured: Loading 122.8px -> failed 294.0px, CLS 0.056).
                 The min-heights above are load-bearing: visibility:hidden reserves the
                 box but an EMPTY <p> still collapses to a zero-height line box.
                 3.15rem is THREE lines, not one, because the reason a real failure
                 carries is a URL: "Failed to fetch dynamically imported module:
                 https://host/_assets/<64-hex>/crossword-web.js?r=<ms>" measures 137
                 characters, i.e. 3 lines at this width. One reserved line left the
                 failed card 38px taller than the loading one (CLS 0.006) when measured
                 against real fonts. A pathological message longer than 3 lines would
                 still grow the card by one line. */
              #boot-err,#boot-actions{visibility:hidden}
              /* Border is --b-dim, not --b-line: a control boundary needs 3:1 and --b-line is
                 1.19:1 on the dark card. --b-dim is 6.91:1 dark / 7.22:1 light. */
              .boot-btn{font:600 .875rem/1 inherit;padding:.6rem .9rem;min-height:2.75rem;cursor:pointer;
                background:var(--b-card);color:var(--b-dim);border:1px solid var(--b-dim);text-decoration:none}
              .boot-btn:hover,.boot-btn:focus-visible{color:var(--b-fg);border-color:var(--b-fg)}
              .boot-btn:focus-visible{outline:2px solid var(--b-fg);outline-offset:2px}
              /* The one action that can fix this: the house .app-btn-active idiom
                 (styles.rs:221), 14.7:1 dark / 6.3:1 light. */
              .boot-btn-primary{color:var(--b-fg);border-color:var(--b-primary)}
              /* DEF-183 D3: the #boot half of this rule is gone — nothing inside #boot uses
                 `hidden` any more (the error and action rows are visibility-toggled so
                 their space is reserved). Kept for noscript-only content: an author rule
                 beating the UA [hidden] rule is why !important is needed, and a global one
                 would reach into app styles post-boot. */
              noscript [hidden]{display:none !important}
              </style>
              </head>
              <body>
              <div id="main"></div>
              <div id="boot" role="status" aria-live="polite">
                <div class="boot-card">
                  <p class="boot-mark">definitely-not-crosswords</p>
                  <h1 class="boot-title" id="boot-title" tabindex="-1">Loading</h1>
                  <p class="boot-body" id="boot-body">Fetching the app&#8230;</p>
                  <p class="boot-build">Build <span data-build="__BUILD_SHA__">__BUILD_SHA__</span></p>
                  <p class="boot-err" id="boot-err" aria-hidden="true"></p>
                  <div class="boot-actions" id="boot-actions" aria-hidden="true">
                    <button class="boot-btn boot-btn-primary" id="boot-retry" type="button">Retry</button>
                    <button class="boot-btn" id="boot-reload" type="button">Reload</button>
                  </div>
                </div>
              </div>
              <noscript><div class="boot-noscript"><div class="boot-card">
                <p class="boot-mark">definitely-not-crosswords</p>
                <h1 class="boot-title">JavaScript is required</h1>
                <p class="boot-body">This is a WebAssembly app, so it needs JavaScript enabled to run.</p>
              </div></div></noscript>
              <script type="module">
              // Pre-boot loader. Runs BEFORE the wasm exists, so nothing here may reference
              // the app's stylesheet — panel_kit::CSS and styles::DESIGN are injected by
              // client/web/src/main.rs only after the app mounts.
              const $ = (id) => document.getElementById(id);
              const setTitle = (t) => { const n = $("boot-title"); if (n) n.textContent = t; };
              const setBody = (t) => { const n = $("boot-body"); if (n) n.textContent = t; };
              const showActions = (on) => {
                // DEF-183 D3: the rows are always rendered — that is what reserves their
                // space — so "show" is visibility + aria-hidden. The `hidden` attribute
                // would collapse the row back to nothing and move the card again.
                for (const id of ["boot-err", "boot-actions"]) {
                  const n = $(id);
                  if (n) { n.style.visibility = on ? "visible" : ""; n.setAttribute("aria-hidden", on ? "false" : "true"); }
                }
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
                // DEF-183 D2: no role swap. An explicit aria-live beats the implicit value
                // of `role`, so swapping status->alert changed nothing for assistive tech
                // while the e2e asserted the attribute instead of the announcement. The
                // card announces politely once, as a status, in every state.
                $("boot-retry").focus(); mark("failed");
              }

              function reset() {
                setTitle("Loading"); setBody("Fetching the app…"); showActions(false);
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
              sed -i "s|__BUNDLE_HASH__|$bundleHash|g; s|__BUILD_SHA__|$BUILD_SHA|g" $out/index.html
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
        # Protocol-level load test for the multiplayer API. Needs a LIVE host,
        # so it lives in `packages` and is run manually / from a scheduled GHA
        # job — never in `checks` (see load/README.md).
        loadScripts = pkgs.runCommand "crossword-load-scripts" { } ''
          mkdir -p "$out"
          cp -r ${../load} "$out/load"
        '';
        crossword-load = pkgs.writeShellScriptBin "crossword-load" ''
          set -euo pipefail
          # `nix run ./client#crossword-load -- run multiplayer.js` and
          # `... -- version` both work: bare k6 args pass through, and any
          # `*.js` argument is resolved against the packaged load/ directory.
          script_dir="${loadScripts}/load"
          if [ "$#" -gt 0 ]; then
            args=()
            for arg in "$@"; do
              case "$arg" in
                *.js) args+=("$script_dir/$arg") ;;
                *) args+=("$arg") ;;
              esac
            done
            exec ${pkgs.k6}/bin/k6 "''${args[@]}"
          fi
          exec ${pkgs.k6}/bin/k6 version
        '';

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
            # `cargo-test` above can only reach the four crates in
            # `commonArgs.cargoExtraArgs` (core/db/auth/events = 23 tests): the
            # server is excluded there because it static-links onnxruntime, and
            # the wasm crate has no native test runner. That left the backend's
            # 58 tests — the auth gate, the wire format, the generator, the grid
            # JSON, and the whole `tests/` suite — unexecuted by any pipeline
            # (DEF-133). This check builds the server with `ortEnv` so `ort-sys`
            # links the same vendored onnxruntime the release binary does, and
            # runs its tests.
            #
            # Nothing here needs a network, a database, or the generator assets:
            # sqlx is used with runtime queries only (no `query!` macros, so no
            # DATABASE_URL at build time), and the embedding tests self-skip via
            # `test_model()` returning None when `data/crossword` is absent.
            # The DB-backed `job_create` tests stay `#[ignore]`d and are not part
            # of this count.
            cargo-test-server = craneLib.cargoTest (
              serverArgs
              // {
                cargoArtifacts = serverCargoArtifacts;
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

          packages = {
            default = crossword-web;
            inherit
              crossword-web
              crossword-desktop
              crossword-server
              crossword-tools
              crossword-load
              ;
          };
        in
        {
          inherit checks packages;

          # Returned so `packagesFor` callers and `nix eval` can read back the
          # sha these packages bake, with no builder involved.
          buildSha = buildSha;

          # `nix develop ./client` runs these same checks on `enterShell`. This is
          # the `checks` binding in the `let` above rather than flake-parts'
          # `self'.checks`: there is no `self` in scope here (this function is
          # what `perSystem` wraps, so referring back to it would be a cycle).
          devShells.default = craneLib.devShell (
            {
              checks = checks;
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
    in
    fp.mkFlake { inherit inputs; }
    {
      systems = import inputs.systems;

      # The default outputs: this flake consumed directly
      # (`nix build ./client#…`, `nix develop ./client`). Empty fallback sha, so
      # `resolveBuildSha` falls through to CROSSWORDS_BUILD_SHA, then self.rev.
      perSystem =
        { system, ... }:
        builtins.removeAttrs (packagesForImpl resolveBuildSha "" system) [ "buildSha" ];

      # DEF-317: the two provenance outputs, keyed by system.
      #
      # Published through the `flake` option because it is freeform — "Any
      # attribute can be set here" — so no `mkOption` declaration is needed and
      # nothing has to be transposed. Measured, because the obvious routes all
      # fail here:
      #   * an extra key on `perSystem`'s result is dropped ("The option
      #     `perSystem.x86_64-linux.buildSha' does not exist"): flake-parts only
      #     publishes declared options;
      #   * a custom top-level `options`/`config` attrset makes flake-parts
      #     reject the module outright when the flake also sets `flake` —
      #     "Module `:anon-17:anon-1' has an unsupported attribute `flake'" — so
      #     declaring options here would mean deleting `flake.hydraJobs` (the
      #     nixlab Hydra jobset) and `flake.om`;
      #   * `mkTransposedPerSystemModule` type-checks its transposed `flake.<name>`
      #     against the option type and rejects a plain string ("A definition for
      #     option `flake.myStr.x86_64-linux' is not of type `string'"), so it
      #     cannot carry a build sha.
      # `lib.genAttrs` over `systems` is the shape flake-parts' own
      # transposition module produces, so this reads the same as a declared one.
      flake =
        {
          # The sha THIS flake resolves with no caller, per system.
          # `nix eval .#buildSha.x86_64-linux` — no builder required, which is
          # the only kind of proof a builderless CI runner can produce.
          buildSha = lib.genAttrs systems (
            system: (packagesForImpl resolveBuildSha "" system).buildSha
          );

          # client.packagesFor.${system} { buildSha = self.rev or ""; }
          #
          # A FUNCTION, not a resolved attrset: the sha is an INPUT to it, not an
          # output. So `nix flake check` never coerces or builds it, and `nix
          # eval` reads `.buildSha` straight off the result. A caller that passes
          # no sha gets exactly what `perSystem` exposes, so the two paths cannot
          # drift.
          packagesFor = lib.genAttrs systems (
            system: { buildSha ? "" }: packagesForImpl resolveBuildSha buildSha system
          );
        }
        // {
          # Hydra builds the `hydraJobs` output (NOT `checks`/`packages`), so the
          # nixlab Hydra jobset that points at this flake (gcp-hydra,
          # definitely-not-crosswords project, `dioxus-migration` jobset) would
          # build nothing without this. Surface every check (which already includes
          # the `crossword-web` bundle) as a Hydra job on the on-prem linux builders.
          hydraJobs.x86_64-linux = inputs.self.checks.x86_64-linux;

          # `om ci run` builds every flake check + package across the configured
          # systems. The root subflake covers this flake.
          om.ci.default.root = {
            dir = ".";
            steps.build.enable = true;
          };
        };
    };
}
