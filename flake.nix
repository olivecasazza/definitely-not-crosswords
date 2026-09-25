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
        crossword-desktop = client.packages.${system}.crossword-desktop;

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

        # buildbot-nix evaluates `.#checks.<system>` per PR and reports a GitHub
        # commit status for each; GitHub Actions runs no Nix. The four
        # deliverables, the deployable image, and the migration gate.
        #
        # x86_64-linux only: the buildbot workers are all x86_64-linux, and the
        # other systems' checks fail at eval time (crossword-client-src is built
        # during evaluation and no darwin/aarch64 builder exists), so they only
        # produced red statuses. Packages for other systems are unaffected.
        checks = nixpkgs.lib.optionalAttrs (system == "x86_64-linux") {
          inherit
            crossword-server
            crossword-web
            crossword-tools
            crossword-desktop
            dockerImage
            ;

          # Migrations are applied by the `migrate-db` init container at pod
          # start, so before this gate the FIRST place any migration ever ran
          # was a real database. That is how prod silently lost
          # `User."vipPass"` and the whole `Discount` table for six weeks (see
          # migrations/20260812000000_repair_adopted_schema_drift). Runs against
          # a throwaway Postgres inside the build sandbox.
          migrations =
            pkgs.runCommand "migrations-apply-cleanly"
              {
                nativeBuildInputs = [ pkgs.postgresql ];
                migrationsDir = ./client/backend/tools/migrations;
              }
              ''
                set -euo pipefail
                # sqlx rejects socket-dir URLs ("empty host"), so loopback TCP.
                export PGDATA=$TMPDIR/pg PGHOST=127.0.0.1 PGPORT=54329
                initdb -U ci --auth=trust >/dev/null
                pg_ctl -o "-k $TMPDIR -c listen_addresses=127.0.0.1 -p $PGPORT" -w start >/dev/null
                createdb -U ci ci
                export DATABASE_URL="postgresql://ci@127.0.0.1:$PGPORT/ci"
                q() { psql "$DATABASE_URL" -At -c "$1"; }

                # 1. Fresh database: every migration must apply from scratch.
                ${crossword-tools}/bin/migrate
                files=$(ls $migrationsDir/*.sql | wc -l)
                applied=$(q 'select count(*) from _sqlx_migrations')
                echo "migration files: $files / applied: $applied"
                [ "$files" -eq "$applied" ] || { echo "$files migration files but $applied applied rows"; exit 1; }
                # The 2026-06-30 corruption signature: a row inserted by a
                # baseline rather than an actual run.
                [ "$(q 'select count(*) from _sqlx_migrations where execution_time = 0')" -eq 0 ] \
                  || { echo "migration(s) recorded applied without running"; exit 1; }
                [ "$(q 'select count(*) from _sqlx_migrations where success is not true')" -eq 0 ] \
                  || { echo "migration(s) recorded as failed"; exit 1; }

                # 2. Re-running is a no-op: the init container runs on every pod start.
                ${crossword-tools}/bin/migrate
                [ "$(q 'select count(*) from _sqlx_migrations')" = "$applied" ] \
                  || { echo "rerun changed the applied count"; exit 1; }

                # 3. The objects prod actually lost.
                check() { [ "$(q "$2")" = t ] || { echo "missing $1"; exit 1; }; echo "ok  $1"; }
                check 'User."vipPass"' "select exists(select 1 from information_schema.columns where table_name='User' and column_name='vipPass')"
                check 'Discount table' "select exists(select 1 from information_schema.tables where table_name='Discount')"
                check 'Team table' "select exists(select 1 from information_schema.tables where table_name='Team')"
                check 'DailyPick table' "select exists(select 1 from information_schema.tables where table_name='DailyPick')"

                pg_ctl -w stop >/dev/null
                touch $out
              '';
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
