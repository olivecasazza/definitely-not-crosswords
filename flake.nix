{
  description = "A Nix devShell for definitely-not-crosswords";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    sops-nix.url = "github:Mic92/sops-nix";
  };

  outputs = { self, nixpkgs, flake-utils, sops-nix }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          config.allowUnfree = true;
        };

        defaultPackage = pkgs.stdenv.mkDerivation rec {
          pname = "definitely-not-crosswords";
          version = "0.1.0";
          src = ./.;

          nativeBuildInputs = [
            pkgs.nodejs_22
            pkgs.pnpm
            pkgs.pnpmConfigHook
          ];

          pnpmDeps = pkgs.fetchPnpmDeps {
            inherit pname version src;
            fetcherVersion = 3;
            hash = "sha256-KxpYL93VkizF4ODxxr9hlQZizoIzoXBOLEeWzOkvc7M=";
          };

          PRISMA_SCHEMA_ENGINE_BINARY="${pkgs.prisma-engines_6}/bin/schema-engine";
          PRISMA_QUERY_ENGINE_BINARY="${pkgs.prisma-engines_6}/bin/query-engine";
          PRISMA_QUERY_ENGINE_LIBRARY="${pkgs.prisma-engines_6}/lib/libquery_engine.node";
          PRISMA_MIGRATION_ENGINE_BINARY="${pkgs.prisma-engines_6}/bin/migration-engine";
          PRISMA_INTROSPECTION_ENGINE_BINARY="${pkgs.prisma-engines_6}/bin/introspection-engine";
          PRISMA_FMT_BINARY="${pkgs.prisma-engines_6}/bin/prisma-fmt";

          NUXT_TELEMETRY_DISABLED = "1";

          buildPhase = ''
            pnpm prisma generate
            pnpm build
          '';

          installPhase = ''
            mkdir -p $out
            cp -r .output $out/
            cp -r node_modules $out/
            cp -r prisma $out/
            cp -r otel.cjs $out/

            # Fix hardcoded file:///build imports in compiled JS files
            echo "🔧 Fixing sandboxed file:///build imports..."
            cat << 'EOF' > patch.js
            const fs = require("fs");
            const path = require("path");
            
            const outNodeModulesDir = path.resolve(process.argv[3]);
            
            function walk(dir) {
              for (const file of fs.readdirSync(dir)) {
                const fullPath = path.join(dir, file);
                if (fs.statSync(fullPath).isDirectory()) {
                  walk(fullPath);
                } else if (file.endsWith(".mjs") || file.endsWith(".js")) {
                  let content = fs.readFileSync(fullPath, "utf8");
                  if (content.includes("file:///build")) {
                    console.log("   Patching " + fullPath);
                    const relativeNodeModules = path.relative(path.dirname(fullPath), outNodeModulesDir);
                    content = content.replace(/file:\/\/\/build\/[^\/]+\/node_modules\//g, relativeNodeModules + "/");
                    fs.writeFileSync(fullPath, content, "utf8");
                  }
                }
              }
            }
            walk(process.argv[2]);
            EOF
            node patch.js $out/.output $out/node_modules
            rm patch.js

            # Delete dangling/broken symlinks to satisfy Nix's noBrokenSymlinks check
            echo "🧹 Cleaning up dangling symlinks in $out..."
            find $out -type l ! -exec test -e {} \; -delete
          '';
        };

        dockerImagePackage = pkgs.dockerTools.buildLayeredImage {
          name = "us-central1-docker.pkg.dev/casazza-identity/nixlab/definitely-not-crosswords";
          tag = "latest";

          contents = [
            pkgs.nodejs_22
            pkgs.openssl
            pkgs.bash
            pkgs.coreutils
          ];

          config = {
            Cmd = [ "${pkgs.nodejs_22}/bin/node" "--require" "${defaultPackage}/otel.cjs" "${defaultPackage}/.output/server/index.mjs" ];
            Env = [
              "NODE_ENV=production"
              "PORT=3000"
              "OTEL_METRICS_PORT=9464"
            ];
            WorkingDir = "${defaultPackage}";
          };
        };
      in
      {
        packages.default = defaultPackage;
        packages.dockerImage = dockerImagePackage;

        hydraJobs = {
          default = defaultPackage;
          dockerImage = dockerImagePackage;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            nodejs_22
            pnpm
            typescript-language-server
            openssl
            postgresql
            sops
            age
          ];

          shellHook = ''
            export PRISMA_SCHEMA_ENGINE_BINARY="${pkgs.prisma-engines_6}/bin/schema-engine"
            export PRISMA_QUERY_ENGINE_BINARY="${pkgs.prisma-engines_6}/bin/query-engine"
            export PRISMA_QUERY_ENGINE_LIBRARY="${pkgs.prisma-engines_6}/lib/libquery_engine.node"
            export PRISMA_MIGRATION_ENGINE_BINARY="${pkgs.prisma-engines_6}/bin/migration-engine"
            export PRISMA_INTROSPECTION_ENGINE_BINARY="${pkgs.prisma-engines_6}/bin/introspection-engine"
            export PRISMA_FMT_BINARY="${pkgs.prisma-engines_6}/bin/prisma-fmt"

            echo "🚀 definitely-not-crosswords Dev Shell Active!"
            echo "Node: $(node --version)"
            echo "pnpm: $(pnpm --version)"
            echo "Prisma Engines mapped to nixpkgs path."

            # Automatically decrypt secrets.yaml if it exists
            if [ -f secrets.yaml ]; then
              echo "🔒 secrets.yaml detected. Decrypting environment variables..."
              export KEYCLOAK_CLIENT_SECRET=$(sops decrypt --extract '["KEYCLOAK_CLIENT_SECRET"]' secrets.yaml 2>/dev/null || echo "failed-to-decrypt")
              export NEXTAUTH_SECRET=$(sops decrypt --extract '["NEXTAUTH_SECRET"]' secrets.yaml 2>/dev/null || echo "failed-to-decrypt")
              export DATABASE_URL=$(sops decrypt --extract '["DATABASE_URL"]' secrets.yaml 2>/dev/null || echo "failed-to-decrypt")
              echo "✅ Environment variables populated from decrypted secrets.yaml."
            else
              echo "💡 Tip: Copy secrets.yaml.example to secrets.yaml and run 'sops secrets.yaml' to encrypt/decrypt local secrets."
            fi
          '';
        };
      });
}
