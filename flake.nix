{
  description = "definitely-not-crosswords — Rust/Dioxus crossword app (Axum server + wasm frontend + Tauri desktop). The Rust workspace lives in ./client; this flake packages the deployable server image.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    sops-nix.url = "github:Mic92/sops-nix";
    # The Rust workspace (crossword-server / crossword-web / crossword-desktop)
    # is its own flake; this one consumes its packages for deployment.
    client.url = "path:./client";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      sops-nix,
      client,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          config.allowUnfree = true;
        };

        crossword-server = client.packages.${system}.crossword-server;
        crossword-web = client.packages.${system}.crossword-web;
        crossword-tools = client.packages.${system}.crossword-tools;

        # Runtime assets for the generator (embedding model + WordNet dictionary),
        # fetched and verified by hash. This is the Rust/Nix replacement for
        # scripts/prepare_crossword_assets.mjs — the files are gitignored, so they
        # can't be vendored from source. Hashes mirror data/crossword/manifest.json.
        modelBase = "https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main";
        modelFile =
          path: hash:
          pkgs.fetchurl {
            url = "${modelBase}/${path}";
            inherit hash;
          };
        wordnetTar = pkgs.fetchurl {
          url = "https://wordnetcode.princeton.edu/wn3.1.dict.tar.gz";
          hash = "sha256-P32L6O9uzHFn05sQ1mlU7HNCgLW9zVf32er+Qp0Rwio=";
        };
        crosswordAssets = pkgs.runCommand "crossword-assets" { } ''
          model=$out/crossword/models/all-MiniLM-L6-v2
          mkdir -p $out/crossword/wordnet $model/onnx
          tar -xzf ${wordnetTar} -C $out/crossword/wordnet            # creates dict/
          cp ${modelFile "config.json" "sha256-cTUUn3z/oaVzRmxuTYQj7XO2L9IzLFdb9zig0DP3Dfc="}            $model/config.json
          cp ${modelFile "tokenizer.json" "sha256-2g55kzue1ReYo64niT08X6SiARJs73VYYpbfm00sYqA="}         $model/tokenizer.json
          cp ${modelFile "tokenizer_config.json" "sha256-kmHn15tEyBlcHK2itFPlWwCuuB6QemZkl0tNd3YXKrM="}  $model/tokenizer_config.json
          cp ${modelFile "special_tokens_map.json" "sha256-ttNGvjZqfR1IMy28n987+JYLXYeVIrd5ndulnnYjfuM="} $model/special_tokens_map.json
          cp ${modelFile "vocab.txt" "sha256-B+ztN1zsFE0nyQAkHz4zlHjeyVj5L928VR8pXJkgOKM="}              $model/vocab.txt
          cp ${modelFile "onnx/model_quantized.onnx" "sha256-r9tvGg5FtxXQu5sRdy8DLDmbq9I7/DH+0cFwr8hIvbE="} $model/onnx/model_quantized.onnx
        '';

        # Deployable OCI image: the Axum server serving the wasm bundle on one
        # origin, with the generator assets at the runtime path the server expects
        # (data/crossword relative to WorkingDir). DATABASE_URL / NEXTAUTH_SECRET
        # are injected at runtime (k8s secret).
        dockerImage = pkgs.dockerTools.buildLayeredImage {
          name = "us-central1-docker.pkg.dev/casazza-identity/nixlab/definitely-not-crosswords";
          tag = "latest";
          # cacert for outbound TLS; crossword-tools puts `migrate`/`seed` on PATH
          # for an init job (seed reads data/crossword/wordnet from WORDNET_DICT_DIR
          # / the bundled assets below).
          contents = [
            pkgs.cacert
            crossword-tools
          ];
          config = {
            Cmd = [ "${crossword-server}/bin/crossword-server" ];
            Env = [
              "PORT=3000"
              "WEB_DIST=${crossword-web}"
              "RUST_LOG=info"
              "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
            ];
            WorkingDir = "/app";
            ExposedPorts."3000/tcp" = { };
          };
          extraCommands = ''
            mkdir -p app/data
            cp -r ${crosswordAssets}/crossword app/data/crossword
          '';
        };
      in
      {
        packages = {
          default = crossword-server;
          inherit crossword-server crossword-web dockerImage;
          assets = crosswordAssets;
        };

        hydraJobs = {
          inherit crossword-server dockerImage;
          web = crossword-web;
        };

        # App development happens in the Rust workspace: `nix develop ./client`.
        # This shell carries DB + secrets tooling (migrations, sops) for repo ops.
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            postgresql
            sops
            age
          ];
          shellHook = ''
            echo "definitely-not-crosswords — Rust workspace dev shell is in ./client"
            echo "  nix develop ./client     # cargo / dx / tauri"
            echo "  this shell: psql, sops, age for DB + secrets"
            if [ -f secrets.yaml ]; then
              export NEXTAUTH_SECRET=$(sops decrypt --extract '["NEXTAUTH_SECRET"]' secrets.yaml 2>/dev/null || echo "")
              export DATABASE_URL=$(sops decrypt --extract '["DATABASE_URL"]' secrets.yaml 2>/dev/null || echo "")
            fi
          '';
        };
      }
    );
}
