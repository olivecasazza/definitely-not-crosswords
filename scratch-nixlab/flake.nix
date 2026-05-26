{
  description = "NixOS and NixDarwin Fleet Management";

  # Configure binary cache for all machines using this flake
  nixConfig = {
    extra-substituters = [
      "https://cache.nixos.org"
      "https://nix-community.cachix.org"
      "https://nixlab.cachix.org"
      "https://info.cachix.org"
      "https://consortium.cachix.org"
      "https://hephaestus.cachix.org"
      "https://t2linux.cachix.org"
      "https://cuda-maintainers.cachix.org"
      "https://cache.garnix.io"
    ];
    extra-trusted-public-keys = [
      "cache.nixos.org-1:6NCHdDEVpk303a2OPlyNMZ8MCHEU3FWksXnsZ0+NJwE="
      "nix-community.cachix.org-1:mB9FSh9qf2dCimDSUo8Zy7bkq5CX+/rkCWyvRCYg3Fs="
      "nixlab.cachix.org-1:AlBO5aKvajGvAGhqJ0+C4c+aryVQtDNGNmNHoGtp0wY="
      "info.cachix.org-1:gb/JA8qqndpsceDIWY51GdpQ//Aymbzl7g3H1kpbAfI="
      "consortium.cachix.org-1:myjKQPrhGDQaCtG2YB+xzveMDT/4MXFNwfBDQ4DJYls="
      "hephaestus.cachix.org-1:JGPRmUCM+V+czeVCRTCvX1u205uBrDGTUpJTznmI/qY="
      "t2linux.cachix.org-1:P733c5Gt1qTcxsm+Bae0renWnT8OLs0u9+yfaK2Bejw="
      "cuda-maintainers.cachix.org-1:0dq3bujKpuEPMCX6U4WylrUDZ9JyUG0VpVZa7CNfq5E="
      "cache.garnix.io:CTFPyKSLcx5RMJKfLo5EEPUObbA78b0YQ2DTCJXqr9g="
    ];
  };

  inputs = {
    # Core Nixpkgs channels
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    # Snowfall Lib - opinionated flake structure and helpers
    snowfall-lib = {
      url = "github:snowfallorg/lib";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # OpenCode - AI code editor
    opencode = {
      url = "github:AodhanHayter/opencode-flake";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # System utilities
    systems.url = "github:nix-systems/default";

    # Platform support
    nix-darwin = {
      url = "github:LnL7/nix-darwin";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # Snowfall Lib expects a `darwin` input for macOS virtual systems
    darwin.follows = "nix-darwin";

    # Home Manager
    home-manager = {
      # Keep Home Manager aligned with the NixOS 25.11 channel.
      url = "github:nix-community/home-manager/release-25.11";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # Disk management
    disko = {
      url = "github:nix-community/disko";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # Secrets management
    sops-nix = {
      url = "github:Mic92/sops-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # Terranix — Nix-native Terraform/OpenTofu configuration
    terranix = {
      url = "github:terranix/terranix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # Development tools
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # Deployment tool (custom fork) — kept during migration to consortium
    colmena = {
      url = "github:ocasazza/colmena";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # Consortium — Rust-based cluster deployment (replaces colmena).
    # Tracks `consortium-autoresearch` (the active downstream where the
    # cascade primitive + nh-style live UI + cast --cascade flag landed).
    # The thinner upstream `consortium` repo is the original ClusterShell-
    # derived skeleton; autoresearch is where deploy work happens.
    consortium = {
      # Use github: so Hydra can fetch via its configured GitHub access token
      # instead of requiring an SSH deploy key for this private input.
      url = "github:olivecasazza/consortium-autoresearch?ref=master";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # Hardware detection
    nixos-facter-modules.url = "github:numtide/nixos-facter-modules";

    # Hardware quirks
    nixos-hardware.url = "github:NixOS/nixos-hardware/master";

    # ISO/netboot image generation
    nixos-generators = {
      url = "github:nix-community/nixos-generators";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # Kubenix for declarative Kubernetes manifests
    kubenix = {
      url = "github:hall/kubenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # Hephaestus — bare metal K8s autoscaling operator (private repo)
    hephaestus = {
      url = "github:casazza-info/hephaestus";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # rerun-operator — Kubernetes operator for Rerun dashboards
    rerun-operator = {
      url = "github:casazza-info/rerun-operator";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # Antigravity - AI code editor
    antigravity-nix = {
      url = "github:jacopone/antigravity-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # Niri - scrollable tiling Wayland compositor (AeroSpace-like for Linux)
    niri = {
      url = "github:sodiboo/niri-flake";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # Catppuccin — consistent colorscheme across home-manager programs
    catppuccin = {
      url = "github:catppuccin/nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

  };

  outputs =
    inputs:
    let
      # Import custom libraries
      versionInfo = import ./lib/version.nix;
      util = import ./lib/util.nix;
      colmenaLib = import ./lib/colmena.nix {
        inherit (inputs) nixpkgs;
      };
      consortiumLib = import ./lib/consortium.nix {
        inherit (inputs) nixpkgs;
        consortium = inputs.consortium;
      };
      outputsLib = import ./nix/outputs {
        inherit (inputs)
          self
          nixpkgs
          systems
          ;
        inherit inputs util;
      };

      # Initialize Snowfall Lib
      lib = inputs.snowfall-lib.mkLib {
        inherit inputs;
        src = ./.;
        snowfall = {
          # Treat the flake root as the Snowfall root so that `lib/`,
          # `systems/`, `homes/`, etc. are all discovered here.
          root = ./.;
          namespace = "nixlab";
          meta = {
            name = "nixlab";
            title = "nixlab Fleet";
          };
        };
      };

      # Generate Snowfall outputs
      snowfallOutputs = lib.mkFlake {
        # Global Nixpkgs configuration
        overlays = with inputs; [
          (_final: prev: {
            nixlab = (prev.nixlab or { }) // {
              vscode-latest =
                (import inputs.nixpkgs-unstable {
                  system = prev.stdenv.hostPlatform.system;
                  config.allowUnfree = true;
                }).vscode;
            };
          })
          # Exposes pkgs.niri-stable / pkgs.niri-unstable for system and
          # home-manager configs (see seir and olive@seir niri modules).
          niri.overlays.niri
        ];
        channels-config = {
          allowUnfree = true;
          # NOTE: Do NOT set allowUnsupportedSystem here; it forces aarch64-only
          # CUDA redists like cuda_compat into x86_64 builds and triggers
          # the cuda12.8-cuda_compat-12.8.39468522 failure.
          # allowUnsupportedSystem = true;
          #
          # NOTE: cudaSupport stays OFF globally. Snowfall builds the pkgs
          # instance from this channels-config and ignores per-host
          # `nixpkgs.config.cudaSupport`, so the moment we flip it here
          # *every* host's closure starts evaluating CUDA-flavored
          # variants of any package whose derivation reads
          # `config.cudaSupport`. We'll flip this when seir/traitor
          # actually start running services.vllm via the Snowfall package
          # (ni-9ig); until then, the K8s vllm pods consume a prebuilt
          # upstream image and don't need the CUDA toolchain in any
          # NixOS closure.
        };
        # NixOS system modules (global)
        systems.modules.nixos = with inputs; [
          disko.nixosModules.disko
          sops-nix.nixosModules.sops
        ];
        # Darwin system modules
        systems.modules.darwin = with inputs; [
          sops-nix.darwinModules.sops
        ];
        # Home Manager modules (global)
        #
        # niri.homeModules.niri is intentionally NOT here. niri-flake's NixOS
        # module auto-injects it into `home-manager.sharedModules` on the
        # NixOS-integrated path (nixos-rebuild of any host that imports niri,
        # currently seir); adding it here too produces a double declaration
        # of `programs.niri.*` and breaks NixOS eval.
        #
        # The standalone path (`home-manager switch --flake .#olive@seir`)
        # gets niri injected separately, post-Snowfall, by extending each
        # `homeConfigurations.<user>@<host>` with `niri.homeModules.niri`
        # below the snowfallOutputs let-binding. `homeConfigurations` is only
        # consumed by standalone HM, so this avoids the NixOS conflict.
        # Trying to gate via `osConfig == null` inside this list triggers
        # `_module.args` infinite recursion. See commits df37cc8 / e36f76a /
        # 68ee2eb / 0c1c132 / 2905ce3 for the full saga.
        homes.modules = with inputs; [
          sops-nix.homeManagerModules.sops
          catppuccin.homeModules.catppuccin
          # catppuccin.opencode writes programs.opencode.tui.theme, but the
          # opencode-flake module used in this repo doesn't declare `.tui`.
          # Disable the submodule and stub-declare the option so the pruned
          # `lib.mkIf false` definition has a valid target during eval.
          (
            { lib, ... }:
            {
              options.programs.opencode.tui.theme = lib.mkOption {
                type = lib.types.str;
                default = "";
                internal = true;
                visible = false;
              };
              config.catppuccin.opencode.enable = false;
            }
          )
        ];
      };

      # Colmena deployment configuration
      defaultSystem = "x86_64-linux";
      getHostTags = util.getHostTags inputs.nixpkgs.lib;
      autoHosts = snowfallOutputs.nixosConfigurations // (snowfallOutputs.darwinConfigurations or { });
      colmena = colmenaLib.mkColmena {
        inherit
          autoHosts
          getHostTags
          defaultSystem
          ;
      };

      # Consortium fleet configuration (parallel to colmena during migration)
      fleet = consortiumLib.mkFleet {
        inherit
          autoHosts
          getHostTags
          defaultSystem
          ;
      };
    in
    snowfallOutputs
    // {
      # Version information
      nixlab = {
        inherit (versionInfo)
          version
          releaseDate
          major
          minor
          patch
          ;
      };

      # System configurations (managed by Snowfall Lib)
      nixosConfigurations = snowfallOutputs.nixosConfigurations;
      darwinConfigurations = snowfallOutputs.darwinConfigurations or { };

      # Standalone home-manager configurations need niri.homeModules.niri
      # imported (the NixOS path auto-injects it; standalone does not). We
      # extend each homeConfiguration here rather than via homes.modules to
      # avoid the double-declaration that breaks NixOS eval. See the comment
      # on homes.modules above.
      homeConfigurations = builtins.mapAttrs (
        _name: hmCfg:
        hmCfg.extendModules {
          modules = [ inputs.niri.homeModules.niri ];
        }
      ) snowfallOutputs.homeConfigurations;
      # Deployment configurations
      colmenaHive = colmenaLib.mkColmenaHive colmena;

      # Consortium fleet (migration target — replaces colmenaHive)
      inherit fleet;
      # Standard flake outputs
      inherit (outputsLib)
        formatter
        checks
        devShells
        apps
        inventories
        ;

      # Merge all packages including kubenix outputs
      packages =
        inputs.nixpkgs.lib.recursiveUpdate
          (inputs.nixpkgs.lib.recursiveUpdate snowfallOutputs.packages outputsLib.packages)
          {
            # Kubenix - Kubernetes manifest generation
            # Build: nix build .#k8s-manifests
            # Apply: nix run .#kubenix
            x86_64-linux.k8s-manifests =
              (inputs.kubenix.evalModules.x86_64-linux {
                module = import ./modules/k8s;
                specialArgs = {
                  flake = inputs.self;
                };
              }).config.kubernetes.result;

            x86_64-linux.kubenix = inputs.kubenix.packages.x86_64-linux.default.override {
              module = import ./modules/k8s;
              specialArgs = {
                flake = inputs.self;
              };
            };

            # aarch64-darwin (for building on Mac)
            aarch64-darwin.k8s-manifests =
              (inputs.kubenix.evalModules.aarch64-darwin {
                module = import ./modules/k8s;
                specialArgs = {
                  flake = inputs.self;
                };
              }).config.kubernetes.result;

            aarch64-darwin.kubenix = inputs.kubenix.packages.aarch64-darwin.default.override {
              module = import ./modules/k8s;
              specialArgs = {
                flake = inputs.self;
              };
            };
          };

      # Hydra CI jobs
      hydraJobs.x86_64-linux = {
        # GCE images for all GCP VMs
        gce-images = builtins.listToAttrs (
          map
            (host: {
              name = host;
              value = snowfallOutputs.nixosConfigurations.${host}.config.system.build.googleComputeImage;
            })
            [
              "gcp-cp"
              "gcp-hydra"
            ]
        );

        # NixOS system closures (validates all hosts build)
        systems = builtins.listToAttrs (
          map
            (host: {
              name = host;
              value = snowfallOutputs.nixosConfigurations.${host}.config.system.build.toplevel;
            })
            [
              "seir"
              "contra"
              "gcp-cp"
              "gcp-hydra"
              "mm01"
              "mm02"
              "mm03"
              "mm04"
              "mm05"
              "hp01"
              "hp02"
              "hp03"
            ]
        );

        # Kubernetes manifests
        k8s-manifests =
          (inputs.kubenix.evalModules.x86_64-linux {
            module = import ./modules/k8s;
            specialArgs = {
              flake = inputs.self;
            };
          }).config.kubernetes.result;
      };

      # Convenience: expose the flake source path so internal packages can refer
      # to repo-relative paths even when evaluated from /nix/store.
      nixlab.src = ./.;
    };
}
