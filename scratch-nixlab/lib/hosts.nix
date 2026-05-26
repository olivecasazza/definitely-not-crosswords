# Cluster host registry -- single source of truth for the nixlab fleet.
# All host IPs, users, builder configs, and derived helper functions live here.
# Consumed by: lib/consortium.nix, lib/colmena.nix, nix/bench/, .envrc (via nix eval), nix-machines, modules/k8s/.
{
  # K3s cluster network config
  cluster = {
    serviceCIDR = "10.43.0.0/16";
    clusterCIDR = "10.42.0.0/16";
    serviceCIDRV6 = "fd6e:1eef:1ab0:43::/112";
    clusterCIDRV6 = "fd6e:1eef:1ab0:42::/56";
    nodeCIDRV6 = "fd6e:1eef:1ab0::/64";
    # kube-dns is always at .0.10 of the service CIDR
    dnsIP = "10.43.0.10";
    ulaPrefix = "fd6e:1eef:1ab0::/48";
    bgp = {
      asn = 64512;
      routeReflectors = [
        "contra"
        "gcp-cp"
        "hetzner-relay"
      ];
      # Accept dynamic neighbors from any LAN/WG cluster node (covers all /24s)
      nodePeerCIDR = "0.0.0.0/0";
    };
  };
  # hostname -> IP address
  ips = {
    contra = "192.168.1.100";
    seir = "192.168.1.35";
    hp01 = "192.168.1.121";
    hp02 = "192.168.1.122";
    hp03 = "192.168.1.123";
    # HP iLO BMC management interfaces (IPMI, cipher 3)
    hp-bmc-01 = "192.168.1.101";
    hp-bmc-02 = "192.168.1.102";
    hp-bmc-03 = "192.168.1.103";
    mm01 = "192.168.1.111";
    mm02 = "192.168.1.112";
    mm03 = "192.168.1.113";
    mm04 = "192.168.1.114";
    mm05 = "192.168.1.115";
    # traitor: DHCP-assigned (MAC bc:fc:e7:39:dc:ee on wifi wlp12s0; the
    # wired-eno1 static address in the host config never comes up because
    # the box is on wifi). Lease drifts; previously .130, now .62. Long-term:
    # router DHCP reservation by MAC, then this is documentation.
    traitor = "192.168.1.130";
    # GCE VMs — deploy over WireGuard tunnel
    gcp-cp = "10.99.0.30";
    gcp-hydra = "10.99.0.31";
    # Hetzner Cloud cpx11 in hil. Tunnel IP only — public v4 is 5.78.211.224
    # (managed via nix/tofu/hetzner). Acts as the central WireGuard VPN hub and third BGP route reflector.
    hetzner-relay = "10.99.0.1";
  };

  # hostname -> stable IPv6 address under ulaPrefix + nodeCIDRV6
  ipsV6 = {
    contra = "fd6e:1eef:1ab0::100";
    seir = "fd6e:1eef:1ab0::35";
    hp01 = "fd6e:1eef:1ab0::121";
    hp02 = "fd6e:1eef:1ab0::122";
    hp03 = "fd6e:1eef:1ab0::123";
    mm01 = "fd6e:1eef:1ab0::111";
    mm02 = "fd6e:1eef:1ab0::112";
    mm03 = "fd6e:1eef:1ab0::113";
    mm04 = "fd6e:1eef:1ab0::114";
    mm05 = "fd6e:1eef:1ab0::115";
    traitor = "fd6e:1eef:1ab0::130";
    gcp-cp = "fd6e:1eef:1ab0::30";
    gcp-hydra = "fd6e:1eef:1ab0::31";
    hetzner-relay = "fd6e:1eef:1ab0::1";
  };

  # hostname -> FQDN in nixlab.local
  dnsNames = {
    contra = "contra.nixlab.local";
    seir = "seir.nixlab.local";
    hp01 = "hp01.nixlab.local";
    hp02 = "hp02.nixlab.local";
    hp03 = "hp03.nixlab.local";
    mm01 = "mm01.nixlab.local";
    mm02 = "mm02.nixlab.local";
    mm03 = "mm03.nixlab.local";
    mm04 = "mm04.nixlab.local";
    mm05 = "mm05.nixlab.local";
    traitor = "traitor.nixlab.local";
    gcp-cp = "gcp-cp.nixlab.local";
    gcp-hydra = "gcp-hydra.nixlab.local";
    hetzner-relay = "hetzner-relay.nixlab.local";
  };

  # hostname -> site/rack/zone topology for storage placement and K8s scheduling
  topology = {
    contra = {
      site = "homelab";
      rack = "workstation";
      zone = "local";
      pool = "control-plane";
      fabric = { };
    };
    seir = {
      site = "homelab";
      rack = "workstation";
      zone = "local";
      pool = "gpu-pool";
      fabric = { };
    };
    hp01 = {
      site = "homelab";
      rack = "rack-1";
      zone = "local";
      pool = "gpu-pool";
      fabric = { };
    };
    hp02 = {
      site = "homelab";
      rack = "rack-1";
      zone = "local";
      pool = "gpu-pool";
      fabric = { };
    };
    hp03 = {
      site = "homelab";
      rack = "rack-1";
      zone = "local";
      pool = "gpu-pool";
      fabric = { };
    };
    mm01 = {
      site = "homelab";
      rack = "rack-2";
      zone = "local";
      pool = "cpu-pool";
      fabric = { };
    };
    mm02 = {
      site = "homelab";
      rack = "rack-2";
      zone = "local";
      pool = "cpu-pool";
      fabric = { };
    };
    mm03 = {
      site = "homelab";
      rack = "rack-2";
      zone = "local";
      pool = "cpu-pool";
      fabric = { };
    };
    mm04 = {
      site = "homelab";
      rack = "rack-2";
      zone = "local";
      pool = "cpu-pool";
      fabric = { };
    };
    mm05 = {
      site = "homelab";
      rack = "rack-2";
      zone = "local";
      pool = "cpu-pool";
      fabric = { };
    };
    gcp-cp = {
      site = "gcp-us-west1";
      rack = "cloud";
      zone = "us-west1-a";
      pool = "gcp";
      fabric = { };
    };
    gcp-hydra = {
      site = "gcp-us-west1";
      rack = "cloud";
      zone = "us-west1-a";
      pool = "gcp";
      fabric = { };
    };
    hetzner-relay = {
      site = "hetzner-hil";
      rack = "cloud";
      zone = "hil";
      pool = "hetzner";
      fabric = { };
    };
    traitor = {
      site = "homelab";
      rack = "workstation";
      zone = "local";
      pool = "gpu-pool";
      fabric = { };
    };
  };

  # hostname -> SSH deploy user
  users = {
    contra = "root";
    seir = "root";
    hp01 = "root";
    hp02 = "root";
    hp03 = "root";
    mm01 = "root";
    mm02 = "root";
    mm03 = "root";
    mm04 = "root";
    mm05 = "root";
    traitor = "root";
    gcp-cp = "root";
    gcp-hydra = "root";
    hetzner-relay = "root";
  };

  # hostname -> builder capabilities
  builders = {
    hp01 = {
      maxJobs = 16;
      speedFactor = 2;
      systems = [ "x86_64-linux" ];
      features = [
        "big-parallel"
        "kvm"
        "benchmark"
        "nixos-test"
      ];
    };
    hp02 = {
      maxJobs = 16;
      speedFactor = 2;
      systems = [ "x86_64-linux" ];
      features = [
        "big-parallel"
        "kvm"
        "benchmark"
        "nixos-test"
      ];
    };
    hp03 = {
      maxJobs = 16;
      speedFactor = 2;
      systems = [ "x86_64-linux" ];
      features = [
        "big-parallel"
        "kvm"
        "benchmark"
        "nixos-test"
      ];
    };
    mm01 = {
      maxJobs = 4;
      speedFactor = 1;
      systems = [ "x86_64-linux" ];
      features = [
        "big-parallel"
        "kvm"
        "benchmark"
        "nixos-test"
      ];
    };
    mm02 = {
      maxJobs = 4;
      speedFactor = 1;
      systems = [ "x86_64-linux" ];
      features = [
        "big-parallel"
        "kvm"
        "benchmark"
        "nixos-test"
      ];
    };
    mm03 = {
      maxJobs = 4;
      speedFactor = 1;
      systems = [ "x86_64-linux" ];
      features = [
        "big-parallel"
        "kvm"
        "benchmark"
        "nixos-test"
      ];
    };
    mm04 = {
      maxJobs = 4;
      speedFactor = 1;
      systems = [ "x86_64-linux" ];
      features = [
        "big-parallel"
        "kvm"
        "benchmark"
        "nixos-test"
      ];
    };
    mm05 = {
      maxJobs = 4;
      speedFactor = 1;
      systems = [ "x86_64-linux" ];
      features = [
        "big-parallel"
        "kvm"
        "benchmark"
        "nixos-test"
      ];
    };
    seir = {
      maxJobs = 12;
      speedFactor = 10;
      systems = [ "x86_64-linux" ];
      features = [
        "big-parallel"
        "kvm"
        "benchmark"
        "nixos-test"
      ];
    };
    contra = {
      maxJobs = 8;
      speedFactor = 1;
      systems = [ "x86_64-linux" ];
      features = [
        "big-parallel"
        "kvm"
        "benchmark"
        "nixos-test"
      ];
    };
  };

  # Generate a nix-machines line for a single builder.
  # sshKey: path to the deploy SSH key
  mkBuilderLine =
    sshKey: hostname:
    let
      self = import ./hosts.nix;
      ip = self.ips.${hostname};
      user = self.users.${hostname};
      b = self.builders.${hostname};
      systems = builtins.concatStringsSep "," b.systems;
      features = builtins.concatStringsSep "," b.features;
    in
    "ssh-ng://${user}@${ip} ${systems} ${sshKey} ${toString b.maxJobs} ${toString b.speedFactor} ${features}";

  # Generate a full nix-machines file content for a list of builder hostnames.
  mkMachinesFile =
    sshKey: hostnames:
    let
      self = import ./hosts.nix;
    in
    builtins.concatStringsSep "\n" (map (self.mkBuilderLine sshKey) hostnames);

  # All hostnames that can act as builders.
  builderNames = builtins.attrNames (import ./hosts.nix).builders;
}
