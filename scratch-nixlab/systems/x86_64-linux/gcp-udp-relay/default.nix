# GCP Game Relay VM -- WireGuard hub + nftables DNAT for game servers
#
# All cluster nodes connect to this VM's WireGuard endpoint.
# Health-check failover selects the active game-server peer.
#
# Build GCE image:
#   nix build .#nixosConfigurations.gcp-udp-relay.config.system.build.googleComputeImage
{
  lib,
  pkgs,
  inputs,
  ...
}:
let
  peers = import ../../../lib/wireguard-peers.nix;

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
      # gcp-hydra is not a k3s node — no podCidr
      inherit (peers.gcp-hydra) tunnelIp publicKey;
    }
    {
      name = "hetzner-relay";
      # Third BGP route reflector + k3s agent on Hetzner Cloud.
      # Connects to the relay over the public internet (no GCP-internal path).
      inherit (peers.hetzner-relay) tunnelIp podCidr publicKey;
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
    "${inputs.nixpkgs}/nixos/modules/virtualisation/google-compute-image.nix"
  ];

  nixlab.gcp-base = {
    enable = true;
    hostname = "gcp-udp-relay";
  };

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

  # ── WireGuard Hub (all cluster nodes connect here) ──────────────
  networking.wireguard.enable = true;
  networking.wireguard.interfaces.wg0 = {
    ips = [ "${peers.gcp-cp.tunnelIp}/24" ];
    listenPort = 51820;
    mtu = 1420;
    privateKey = "ON2RoGWAI7V4t5PmHp+OBThTS4xeH4qw0hVZL1IcQH0=";
    peers =
      # Each peer gets its own pod /24 so the relay can route pod traffic
      # to the right node (WG AllowedIPs must be non-overlapping across
      # peers on this side). seir retains 192.168.1.0/24 as the LAN
      # bridge for cloud→on-prem LAN access.
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

  boot.kernel.sysctl."net.ipv4.ip_forward" = 1;

  networking.interfaces.wg0.ipv4.routes = [
    {
      address = "192.168.1.0";
      prefixLength = 24;
    }
  ];

  # ── nftables (game DNAT + CP NAT bypass) ────────────────────────
  networking.nftables.enable = true;
  networking.firewall.checkReversePath = false;
  networking.nftables.tables.game_relay = {
    family = "ip";
    content = ''
      chain prerouting {
        type nat hook prerouting priority dstnat; policy accept;
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

  networking.firewall = {
    enable = true;
    allowedTCPPorts = [
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

  # ── Game Relay Health Check ─────────────────────────────────────
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

  # ── WireGuard Metrics ───────────────────────────────────────────
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

  environment.systemPackages = with pkgs; [
    nftables
  ];
}
