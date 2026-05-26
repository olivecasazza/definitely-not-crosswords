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
      in
      {
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
