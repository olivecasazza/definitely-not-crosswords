# K8s Worker Module (Agent Nodes)
# Configures K3s agent with GPU support and node labels
{
  lib,
  pkgs,
  config,
  ...
}:
let
  cfg = config.services.k8s-worker;
in
{
  options.services.k8s-worker = {
    enable = lib.mkEnableOption "K8s worker (agent node)";

    serverAddr = lib.mkOption {
      type = lib.types.str;
      description = "K8s server address to join (e.g., 192.168.1.100)";
    };

    nodeIP = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Override node IP (e.g., WireGuard tunnel IP for cloud nodes)";
    };

    nodeIPv6 = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Stable IPv6 node IP used during dual-stack rollout.";
    };

    tokenFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = "Path to file containing join token (from SOPS)";
    };
    token = lib.mkOption {
      type = lib.types.str;
      default = "k8s-cluster-token";
      description = "Cluster join token (use tokenFile for secrets)";
    };

    gpuSupport = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Enable NVIDIA GPU support";
    };

    labels = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = { };
      description = "Node labels for scheduling";
      example = {
        "node.kubernetes.io/gpu" = "true";
        "nvidia.com/gpu.product" = "Tesla-P40";
      };
    };

    taints = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      description = "Node taints";
      example = [ "nvidia.com/gpu=true:NoSchedule" ];
    };

    longhornNode = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "This node participates in Longhorn storage";
    };

    kubeletReservation = lib.mkOption {
      type = lib.types.nullOr (
        lib.types.submodule {
          options = {
            systemReserved = lib.mkOption {
              type = lib.types.str;
              example = "cpu=8,memory=16Gi";
              description = "Resources reserved for OS daemons (subtracted from Allocatable).";
            };
            kubeReserved = lib.mkOption {
              type = lib.types.str;
              example = "cpu=8,memory=16Gi";
              description = "Resources reserved for k8s components (kubelet/containerd/k3s-agent).";
            };
          };
        }
      );
      default = null;
      description = ''
        When set, cap k8s pod resource usage at the host level.

        Kubelet writes a hard cgroup limit on `/kubepods` of
        (capacity - systemReserved - kubeReserved) and refuses to schedule
        pods beyond it. Use this on hosts where runaway pod memory pressure
        has historically dropped Longhorn iSCSI sessions / hung the box.

        GPU resources are unaffected — caps only apply to CPU + memory.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    # Enable shared GPU module when gpuSupport is requested
    services.k8s-gpu.enable = lib.mkIf cfg.gpuSupport true;

    # K3s agent configuration
    services.k3s = {
      enable = true;
      role = "agent";
      serverAddr = "https://${cfg.serverAddr}:6443";
      token = cfg.token;
      tokenFile = cfg.tokenFile;

      extraFlags = lib.concatStringsSep " " (
        # Node labels
        (lib.mapAttrsToList (k: v: "--node-label=${k}=${v}") cfg.labels)
        ++
          # Node taints
          (map (t: "--node-taint=${t}") cfg.taints)
        ++
          # Longhorn label
          (lib.optionals cfg.longhornNode [
            "--node-label=node.longhorn.io/create-default-disk=true"
          ])
        ++
          # GPU labels (auto-added alongside services.k8s-gpu, matches k8s-server{,-join})
          (lib.optionals cfg.gpuSupport [
            "--node-label=node.kubernetes.io/gpu=true"
            "--node-label=nvidia.com/gpu.present=true"
          ])
        ++
          # Tell kubelet to use systemd cgroup driver to match containerd's SystemdCgroup = true.
          # K3s's built-in containerd config uses SystemdCgroup = true, so kubelet must match.
          [
            "--kubelet-arg=cgroup-driver=systemd"
            "--resolv-conf=/etc/k3s-resolv.conf"
          ]
        ++ (lib.optionals (cfg.nodeIP != null) [
          "--node-ip=${cfg.nodeIP}${lib.optionalString (cfg.nodeIPv6 != null) ",${cfg.nodeIPv6}"}"
        ])
        ++
          # Hard-cap pod resource usage so a runaway workload can't take down the host.
          (lib.optionals (cfg.kubeletReservation != null) [
            "--kubelet-arg=system-reserved=${cfg.kubeletReservation.systemReserved}"
            "--kubelet-arg=kube-reserved=${cfg.kubeletReservation.kubeReserved}"
            "--kubelet-arg=enforce-node-allocatable=pods"
          ])
      );
    };

    environment.etc."k3s-resolv.conf".text = ''
      nameserver 1.1.1.1
      nameserver 1.0.0.1
    '';

    networking.firewall = {
      allowedTCPPorts = [
        10250 # Kubelet
        4240 # Cilium health
        9100 # Prometheus node_exporter
        9633 # Prometheus smartctl_exporter
        9835 # NVIDIA GPU exporter
        9962 # Cilium agent metrics
        9965 # Hubble metrics
      ];
      allowedUDPPorts = [
        8472 # VXLAN
        51871 # WireGuard
      ];
      # CRITICAL: Disable rpfilter - it breaks Cilium host-to-pod connectivity
      # The NixOS firewall's rpfilter drops packets from pods because the reverse
      # path check fails. Pod IPs (10.42.x.x) route via cilium_host, but packets
      # arrive on lxc* interfaces, causing rpfilter to drop them as "spoofed".
      # This is the root cause of kubelet health check failures.
      checkReversePath = false;

      # CRITICAL: Add MASQUERADE for all pod egress traffic
      # Cilium's default masquerade only covers the local node's pod CIDR
      # (e.g., 10.42.6.0/24), but with VXLAN tunneling, pods need to be masqueraded
      # to the node's IP when accessing external destinations.
      extraInputRules = ''
        meta l4proto { tcp, udp } accept
      '';
    };

    # Local k8s API endpoint redirect.
    # Cilium (kubeProxyReplacement=true) needs to reach the apiserver to bootstrap,
    # before kube-proxy / cluster service IP routing exists. We point Cilium at
    # 127.0.0.1:6443 cluster-wide, but agent nodes don't run the apiserver locally;
    # they run the k3s-agent load balancer on 127.0.0.1:6444 which round-robins
    # across all known servers. This NAT rule rewrites locally-generated traffic
    # to 127.0.0.1:6443 so it lands on the agent LB.
    # Without this, losing any single CP node strands every node that restarts.
    # Implemented via iptables (firewall.extraCommands) because this cluster uses
    # the iptables-backend NixOS firewall, not native nftables.
    networking.firewall.extraCommands = ''
      iptables -t nat -C OUTPUT -p tcp -d 127.0.0.1 --dport 6443 -j REDIRECT --to-ports 6444 2>/dev/null \
        || iptables -t nat -A OUTPUT -p tcp -d 127.0.0.1 --dport 6443 -j REDIRECT --to-ports 6444
    '';
    networking.firewall.extraStopCommands = ''
      iptables -t nat -D OUTPUT -p tcp -d 127.0.0.1 --dport 6443 -j REDIRECT --to-ports 6444 2>/dev/null || true
    '';

    # (Removed obsolete static routes for game relay tunnel.
    # Cilium handles SNAT for LoadBalancer traffic automatically,
    # so return traffic naturally routes back to the ingress node.)

    # ── Metrics: Node Exporter + SMART ──────────────────────────────────
    services.prometheus.exporters.node = {
      enable = true;
      port = 9100;
      listenAddress = "0.0.0.0";
      enabledCollectors = [
        "systemd"
        "cpu"
        "diskstats"
        "filesystem"
        "loadavg"
        "meminfo"
        "netdev"
        "stat"
      ];
    };

    services.prometheus.exporters.smartctl = {
      enable = true;
      port = 9633;
      listenAddress = "0.0.0.0";
      maxInterval = "2m";
      devices = [ ];
    };

    # CRITICAL: Deny dhcpcd on virtual interfaces to prevent buffer overflow
    # dhcpcd opens a raw ICMP socket on all interfaces for Duplicate Address Detection.
    # When running on Cilium virtual interfaces (lxc*, cilium_host), it receives all
    # ICMP traffic destined for pods, fills up its receive buffer, and drops packets.
    # This causes host-to-pod connectivity failures (kubelet health checks fail).
    networking.dhcpcd.extraConfig = ''
      # Deny Kubernetes/Cilium virtual interfaces
      denyinterfaces lxc* cilium* veth* cni* docker* flannel* kube*
    '';

    # Use systemd-resolved with DNS over TLS (DoT) to bypass ISP DNS interception
    # CenturyLink blocks/hijacks UDP 53, but DoT on TCP 853 works
    # resolved manages /etc/resolv.conf itself - don't set networking.nameservers
    services.resolved = {
      enable = true;
      dnssec = "false";
      dnsovertls = "opportunistic";
      domains = [ "~." ]; # Use resolved for all domains
      fallbackDns = [
        "1.1.1.1#one.one.one.one"
        "1.0.0.1#one.one.one.one"
      ];
      # Primary DNS servers with DoT
      extraConfig = ''
        DNS=1.1.1.1#one.one.one.one 1.0.0.1#one.one.one.one
      '';
    };

    # System packages
    environment.systemPackages =
      with pkgs;
      [
        kubectl
      ]
      ++ lib.optionals cfg.longhornNode [
        # Longhorn requirements
        openiscsi
        nfs-utils
      ];

    # Longhorn requirements
    services.openiscsi = lib.mkIf cfg.longhornNode {
      enable = true;
      name = config.networking.hostName;
      # Tolerate transient memory/CPU pressure without dropping Longhorn replicas.
      # Defaults are noop_out_interval=5 / noop_out_timeout=5: any 5-second stall
      # of the iSCSI initiator (swap thrash, OOM-kill loop, kernel pressure)
      # severs the session, which then trips replacement_timeout and ends with
      # ext4 going read-only on every Longhorn-backed mount on this host —
      # the exact cascade that took seir down on 2026-05-02.
      extraConfig = ''
        node.conn[0].timeo.noop_out_interval = 30
        node.conn[0].timeo.noop_out_timeout = 30
        node.session.timeo.replacement_timeout = 600
      '';
    };

    # Longhorn expects binaries in /usr/bin (uses nsenter to find them)
    systemd.tmpfiles.rules = lib.optionals cfg.longhornNode [
      "L+ /usr/bin/iscsiadm - - - - ${pkgs.openiscsi}/bin/iscsiadm"
    ];
  };
}
