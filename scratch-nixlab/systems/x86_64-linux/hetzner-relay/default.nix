# Hetzner Cloud relay node — NixOS system configuration.
#
# Third iBGP route reflector in the cluster BGP fabric. K3s agent (not
# control-plane — keeps etcd at 3 voters). Reaches the GCP udp-relay
# hub over the public internet (no GCP-internal endpoint available).
#
# Provisioned by `nix/tofu/hetzner` (hcloud cpx11 in hil). Reimaged from
# Ubuntu to NixOS via `nixos-anywhere --flake .#hetzner-relay`.
{
  config,
  lib,
  pkgs,
  ...
}:
let
  peers = import ../../../lib/wireguard-peers.nix;
  hosts = import ../../../lib/hosts.nix;
  keys = import ../../../lib/keys.nix;

  onPremPeers = [
    {
      name = "contra";
      inherit (peers.contra)
        tunnelIp
        lanIp
        podCidr
        publicKey
        ;
    }
    {
      name = "seir";
      inherit (peers.seir)
        tunnelIp
        lanIp
        podCidr
        publicKey
        ;
    }
    {
      name = "mm01";
      inherit (peers.mm01)
        tunnelIp
        lanIp
        podCidr
        publicKey
        ;
    }
    {
      name = "mm02";
      inherit (peers.mm02)
        tunnelIp
        lanIp
        podCidr
        publicKey
        ;
    }
    {
      name = "mm03";
      inherit (peers.mm03)
        tunnelIp
        lanIp
        podCidr
        publicKey
        ;
    }
    {
      name = "mm04";
      inherit (peers.mm04)
        tunnelIp
        lanIp
        podCidr
        publicKey
        ;
    }
    {
      name = "mm05";
      inherit (peers.mm05)
        tunnelIp
        lanIp
        podCidr
        publicKey
        ;
    }
    {
      name = "traitor";
      inherit (peers.traitor)
        tunnelIp
        lanIp
        podCidr
        publicKey
        ;
    }
    {
      name = "hp01";
      inherit (peers.hp01)
        tunnelIp
        lanIp
        podCidr
        publicKey
        ;
    }
    {
      name = "hp02";
      inherit (peers.hp02)
        tunnelIp
        lanIp
        podCidr
        publicKey
        ;
    }
    {
      name = "hp03";
      inherit (peers.hp03)
        tunnelIp
        lanIp
        podCidr
        publicKey
        ;
    }
  ];

  cloudPeers = [
    {
      name = "gcp-cp";
      inherit (peers.gcp-cp) tunnelIp podCidr publicKey;
    }
    {
      name = "gcp-hydra";
      inherit (peers.gcp-hydra) tunnelIp publicKey;
    }
  ];

  clusterPeers = onPremPeers ++ cloudPeers;
  peerIpList = builtins.concatStringsSep " " (map (p: p.tunnelIp) clusterPeers);

  cpPeers = [
    peers.contra
    peers.seir
    peers.gcp-cp
    peers.gcp-hydra
  ];
  cpIpSet = builtins.concatStringsSep ", " (
    (map (p: p.tunnelIp) cpPeers) ++ (map (p: p.lanIp) (builtins.filter (p: p ? lanIp) cpPeers))
  );

  gamePorts = [
    "tcp:25565:minecraft-ftb"
    "tcp:25566:minecraft-tekkit2"
    "udp:2456:valheim-game"
    "udp:2457:valheim-query"
    "udp:2458:valheim-rcon"
    "udp:16261:zomboid-game"
    "udp:16262:zomboid-query"
    "tcp:27015:zomboid-rcon"
    "udp:8766:zomboid-steam-query"
    "udp:27016:zomboid-steam-master"
  ];

  nftDnatRules = builtins.concatStringsSep "\n" (
    map (
      rule:
      let
        parts = builtins.split ":" rule;
        proto = builtins.elemAt parts 0;
        port = builtins.elemAt parts 2;
      in
      "    ${pkgs.nftables}/bin/nft add rule ip game_relay prerouting ${proto} dport ${port} counter dnat to \$TARGET_PEER:${port}"
    ) gamePorts
  );

  peerLookup = builtins.concatStringsSep "\n" (
    map (p: "    \"${p.publicKey}\") echo \"${p.name}\" ;;") clusterPeers
  );

  healthCheckScript = pkgs.writeShellScript "game-relay-healthcheck" ''
    PEERS="${peerIpList}"
    STATE_FILE="/var/run/game-relay-active-peer"
    FAILOVER_COUNT="/var/run/game-relay-failover-count"
    CURRENT=$(cat "$STATE_FILE" 2>/dev/null || echo "")

    if [ -n "$CURRENT" ] && ${pkgs.iputils}/bin/ping -c1 -W2 "$CURRENT" >/dev/null 2>&1; then
      exit 0
    fi

    for peer in $PEERS; do
      if ${pkgs.iputils}/bin/ping -c1 -W2 "$peer" >/dev/null 2>&1; then
        if [ "$peer" != "$CURRENT" ]; then
          logger -t game-relay "Failover: switching from $CURRENT to $peer"
          export TARGET_PEER="$peer"
          ${pkgs.nftables}/bin/nft flush chain ip game_relay prerouting
    ${nftDnatRules}
          echo "$peer" > "$STATE_FILE"
          COUNT=$(cat "$FAILOVER_COUNT" 2>/dev/null || echo "0")
          echo $((COUNT + 1)) > "$FAILOVER_COUNT"
        fi
        exit 0
      fi
    done

    logger -t game-relay "CRITICAL: No healthy peers found!"
  '';

  metricsScript = pkgs.writeShellScript "wireguard-metrics" ''
    PROM_FILE="/var/lib/prometheus/node-exporter/wireguard.prom"
    ACTIVE_PEER=$(cat /var/run/game-relay-active-peer 2>/dev/null || echo "")
    FAILOVER_COUNT=$(cat /var/run/game-relay-failover-count 2>/dev/null || echo "0")
    NOW=$(date +%s)
    HEALTHY=0

    peer_name() {
      case "$1" in
    ${peerLookup}
        *) echo "unknown" ;;
      esac
    }

    {
      echo "# HELP wireguard_peer_handshake_age_seconds Seconds since last WireGuard handshake"
      echo "# TYPE wireguard_peer_handshake_age_seconds gauge"
      echo "# HELP wireguard_peer_transfer_rx_bytes Total bytes received from peer"
      echo "# TYPE wireguard_peer_transfer_rx_bytes counter"
      echo "# HELP wireguard_peer_transfer_tx_bytes Total bytes sent to peer"
      echo "# TYPE wireguard_peer_transfer_tx_bytes counter"
      echo "# HELP wireguard_peer_active Whether this peer is the current DNAT target"
      echo "# TYPE wireguard_peer_active gauge"

      while IFS=$'\t' read -r pubkey _ endpoint allowed_ips handshake rx tx _; do
        [ -z "$pubkey" ] && continue
        NAME=$(peer_name "$pubkey")
        TIP=$(echo "$allowed_ips" | cut -d/ -f1)
        if [ "$handshake" != "0" ]; then
          AGE=$((NOW - handshake))
        else
          AGE=999999
        fi
        [ "$AGE" -lt 180 ] && HEALTHY=$((HEALTHY + 1))
        ACTIVE=0
        [ "$TIP" = "$ACTIVE_PEER" ] && ACTIVE=1
        echo "wireguard_peer_handshake_age_seconds{peer=\"$NAME\",tunnel_ip=\"$TIP\"} $AGE"
        echo "wireguard_peer_transfer_rx_bytes{peer=\"$NAME\",tunnel_ip=\"$TIP\"} $rx"
        echo "wireguard_peer_transfer_tx_bytes{peer=\"$NAME\",tunnel_ip=\"$TIP\"} $tx"
        echo "wireguard_peer_active{peer=\"$NAME\",tunnel_ip=\"$TIP\"} $ACTIVE"
      done < <(${pkgs.wireguard-tools}/bin/wg show wg0 dump | tail -n +2)

      echo "# HELP wireguard_healthy_peers_total Number of peers with recent handshake"
      echo "# TYPE wireguard_healthy_peers_total gauge"
      echo "wireguard_healthy_peers_total $HEALTHY"
      echo "# HELP wireguard_failover_events_total Total number of failover events"
      echo "# TYPE wireguard_failover_events_total counter"
      echo "wireguard_failover_events_total $FAILOVER_COUNT"
    } > "$PROM_FILE.tmp"
    mv "$PROM_FILE.tmp" "$PROM_FILE"
  '';
in
{
  imports = [
    ./disk.nix
    ../../../modules/nixos/frr.nix
  ];

  networking.hostName = "hetzner-relay";

  # ── Bootloader (Hetzner Cloud cpx11 is BIOS, not UEFI) ─────────────
  # disko derives `boot.loader.grub.devices` from the EF02 BIOS-boot
  # partition in disk.nix. No ESP on this host, so systemd-boot is off.
  boot.loader.grub.efiSupport = false;
  boot.loader.systemd-boot.enable = false;

  # Hetzner Cloud uses virtio drivers in the KVM hypervisor.
  boot.initrd.availableKernelModules = [
    "virtio_net"
    "virtio_pci"
    "virtio_scsi"
    "virtio_blk"
    "9p"
    "9pnet_virtio"
    "ata_piix"
    "uhci_hcd"
    "ehci_pci"
    "sd_mod"
    "sr_mod"
  ];
  boot.kernelModules = [ "kvm-intel" ];

  # ── Networking ─────────────────────────────────────────────────────
  # The public interface is enp1s0 on hcloud cpx11 (configured by Hetzner
  # DHCP). Don't touch it — only declare the WireGuard tunnel here.
  networking.useDHCP = lib.mkDefault true;
  boot.kernel.sysctl."net.ipv4.ip_forward" = 1;
  boot.kernel.sysctl."net.ipv6.conf.all.forwarding" = 1;

  # ── SSH + users ────────────────────────────────────────────────────
  services.openssh.enable = true;
  services.openssh.settings.PermitRootLogin = "yes";
  security.sudo.wheelNeedsPassword = false;
  programs.bash.enable = true;
  users.users.olive = {
    isNormalUser = true;
    shell = pkgs.bash;
    extraGroups = [ "wheel" ];
    openssh.authorizedKeys.keys = [ keys.olive ];
  };
  users.users.root.openssh.authorizedKeys.keys = [
    keys.olive
    keys.root
    keys.repoAdmin
  ];

  # ── SOPS ───────────────────────────────────────────────────────────
  sops.age.sshKeyPaths = [ "/etc/ssh/ssh_host_ed25519_key" ];
  sops.secrets."wireguard/private_key" = {
    sopsFile = ../../../secrets/wireguard.yaml;
    key = "hetzner-relay";
  };
  sops.secrets."bgp/password" = {
    sopsFile = ../../../secrets/bgp.yaml;
    key = "password";
    owner = "frr";
    group = "frr";
    mode = "0400";
  };

  # ── BGP route reflector ────────────────────────────────────────────
  services.nixlab-frr = {
    enable = true;
    bgpPasswordFile = config.sops.secrets."bgp/password".path;
  };

  # ── WireGuard Hub (all cluster nodes connect here) ─────────────────
  networking.wireguard.enable = true;
  networking.wireguard.interfaces.wg0 = {
    ips = [ "${peers.relay.tunnelIp}/24" ];
    listenPort = 51820;
    mtu = 1420;
    privateKeyFile = config.sops.secrets."wireguard/private_key".path;
    peers =
      (map (peer: {
        publicKey = peer.publicKey;
        allowedIPs = [
          "${peer.tunnelIp}/32"
          "${peer.lanIp}/32"
          peer.podCidr
        ]
        ++ lib.optionals (peer.name == "seir") [
          "192.168.1.0/24"
        ];
      }) onPremPeers)
      ++ (map (peer: {
        publicKey = peer.publicKey;
        allowedIPs = [
          "${peer.tunnelIp}/32"
        ]
        ++ lib.optionals (peer ? podCidr) [ peer.podCidr ];
      }) cloudPeers);
  };

  # Disable the spoke WG module
  nixlab.services.game-relay-wg.enable = false;

  # Route on-prem LAN + ULA over the tunnel.
  networking.interfaces.wg0.ipv4.routes = [
    {
      address = "192.168.1.0";
      prefixLength = 24;
    }
  ];
  networking.interfaces.wg0.ipv6.routes = [
    {
      address = "fd6e:1eef:1ab0::";
      prefixLength = 48;
    }
  ];

  # ── nftables (game DNAT + CP NAT bypass + local K8s redirection) ───
  networking.nftables.enable = true;
  networking.firewall.checkReversePath = false;
  networking.nftables.tables.game_relay = {
    family = "ip";
    content = ''
      chain prerouting {
        type nat hook prerouting priority dstnat; policy accept;
      }
      chain output {
        type nat hook output priority dstnat; policy accept;
        ip daddr 127.0.0.1 tcp dport 6443 counter redirect to :6444
      }
      chain postrouting {
        type nat hook postrouting priority srcnat; policy accept;
        ip saddr { ${cpIpSet} } ip daddr { ${cpIpSet} } return
        oifname "wg0" masquerade
      }
      chain forward {
        type filter hook forward priority filter; policy accept;
        oifname "wg0" tcp flags syn tcp option maxseg size set 1344
        iifname "wg0" tcp flags syn tcp option maxseg size set 1344
      }
    '';
  };

  # ── K3s agent ──────────────────────────────────────────────────────
  services.k8s-topology.enable = true;
  services.k8s-worker = {
    enable = true;
    serverAddr = peers.contra.lanIp;
    token = "k8s-cluster-token";
    gpuSupport = false;
    longhornNode = false;
    labels = {
      "node.kubernetes.io/cloud" = "true";
      "topology.nixlab/site" = "hetzner-hil";
    };
    # Cloud-node taint — workloads must opt-in to land here. Same defense
    # against accidental cross-zone egress as gcp-cp.
    taints = [ "node.kubernetes.io/cloud=true:NoSchedule" ];
    nodeIP = peers.relay.tunnelIp;
    nodeIPv6 = hosts.ipsV6.hetzner-relay;
  };

  # ── Firewall ───────────────────────────────────────────────────────
  # Open SSH + WireGuard + Prometheus node_exporter + all game TCP/UDP ports
  # Override k8s-worker's iptables extraCommands with native nftables output chain redirection.
  networking.firewall = {
    enable = true;
    extraCommands = lib.mkForce "";
    extraStopCommands = lib.mkForce "";
    allowedTCPPorts = [
      22
      9100
      25565
      25566
      27015
    ];
    allowedUDPPorts = [
      51820
      2456
      2457
      2458
      16261
      16262
      8766
      27016
    ];
  };

  # ── Prometheus node_exporter ───────────────────────────────────────
  # k8s-worker enables node_exporter on 0.0.0.0; override the listen
  # address so we only expose it on the WireGuard interface (Prometheus
  # scrapes via the tunnel). Wait for wg0 before binding.
  services.prometheus.exporters.node.listenAddress = lib.mkForce peers.relay.tunnelIp;
  services.prometheus.exporters.node.enabledCollectors = lib.mkForce [
    "systemd"
    "cpu"
    "diskstats"
    "filesystem"
    "loadavg"
    "meminfo"
    "netdev"
    "stat"
    "textfile"
  ];
  services.prometheus.exporters.node.extraFlags = [
    "--collector.textfile.directory=/var/lib/prometheus/node-exporter"
  ];
  systemd.tmpfiles.rules = [ "d /var/lib/prometheus/node-exporter 0755 root root -" ];

  systemd.services.prometheus-node-exporter = {
    after = [ "wireguard-wg0.service" ];
    wants = [ "wireguard-wg0.service" ];
  };

  # ── Game Relay Health Check ────────────────────────────────────────
  systemd.services.game-relay-healthcheck = {
    description = "Game relay health check";
    after = [ "wireguard-wg0.service" ];
    wants = [ "wireguard-wg0.service" ];
    serviceConfig = {
      Type = "oneshot";
      ExecStart = healthCheckScript;
    };
  };
  systemd.timers.game-relay-healthcheck = {
    wantedBy = [ "timers.target" ];
    timerConfig = {
      OnBootSec = "10";
      OnUnitActiveSec = "5";
      AccuracySec = "1";
      Persistent = true;
    };
  };

  # ── WireGuard Metrics ──────────────────────────────────────────────
  systemd.services.relay-metrics = {
    description = "Collect WireGuard metrics";
    serviceConfig = {
      Type = "oneshot";
      ExecStart = metricsScript;
    };
  };
  systemd.timers.relay-metrics = {
    wantedBy = [ "timers.target" ];
    timerConfig = {
      OnBootSec = "15";
      OnUnitActiveSec = "15";
      AccuracySec = "1";
      Persistent = true;
    };
  };

  # ── Packages + Nix ─────────────────────────────────────────────────
  environment.systemPackages = with pkgs; [
    wireguard-tools
    nftables
    jq
    curl
    htop
    bat
    kubectl
  ];

  nixpkgs.hostPlatform = "x86_64-linux";
  nix.settings = {
    experimental-features = [
      "nix-command"
      "flakes"
    ];
    trusted-users = [
      "root"
      "olive"
    ];
  };

  system.stateVersion = "25.11";
}
