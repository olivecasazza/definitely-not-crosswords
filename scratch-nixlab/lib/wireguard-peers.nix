# WireGuard peer registry for the GCP game relay tunnel.
#
# Single source of truth for tunnel IPs and public keys.
# Both the NixOS game-relay-wg module and the GCP relay startup script
# derive their configuration from this file.
#
# To add a new node:
#   1. Generate a keypair: nix shell nixpkgs#wireguard-tools -c bash -c 'wg genkey | tee /dev/stderr | wg pubkey'
#   2. Add entry below with tunnelIp, publicKey, and podCidr (from `kubectl get ciliumnode <name> -o jsonpath='{.spec.ipam.podCIDRs}'`)
#   3. Add private key to secrets/wireguard.yaml via: just edit-secret secrets/wireguard.yaml
#   4. Enable game-relay-wg module in the node's NixOS config
#
# Tunnel addressing: 10.99.0.0/24
#   10.99.0.1  = GCP relay
#   10.99.0.2+ = cluster nodes
#
# podCidr: the Cilium-IPAM-assigned pod CIDR for each cluster node (NOT
# k3s's node.spec.podCIDR — this cluster uses Cilium's own IPAM pool,
# which allocates different /24s). Required so the relay can route pod-
# to-pod traffic across the tunnel (each peer gets its pod /24 in
# AllowedIPs — WG validates source IPs symmetrically).
#
# MUST match:
#   kubectl get ciliumnode -o jsonpath='{range .items[*]}{.metadata.name}={.spec.ipam.podCIDRs}{"\n"}{end}'
#
# A mismatch silently drops packets at the WG cryptokey-routing layer
# with no log — manifests as ClusterIP timeouts from pods on the GCP side
# while direct pod-IP connections to the correctly-mapped peers still
# work. Update on node rejoin or Cilium IPAM reallocation.
{
  # GCP relay endpoint (gcp-udp-relay VM).
  # All cluster nodes connect to this endpoint.
  #
  # `endpoint` is the public IP — used by on-prem peers (and is the only
  # reachable address from the internet).
  # `internalEndpoint` is the GCP-internal IP — used by other GCP-hosted
  # peers (gcp-cp, gcp-hydra) so their WG traffic stays on the GCP private
  # network instead of hairpinning out via external IP and back, which GCP
  # bills as `Network Inter Zone Data Transfer Out` ($0.01/GiB) even when
  # source and destination are in the same zone. udp-relay's wg0 listens
  # on 0.0.0.0:51820 so both endpoints are valid simultaneously.
  relay = {
    tunnelIp = "10.99.0.1";
    publicKey = "CGiH1zuirQwfoOW0avQDgUTbHDys5aiXbpJRN5eOs0A=";
    endpoint = "5.78.211.224:51820";
  };

  # Cluster nodes — priority order for failover (game-server nodes first)
  # lanIp: used by relay to route on-prem LAN traffic through the tunnel
  # (cloud CPs need to reach etcd peers at their LAN IPs for TLS cert validation)
  contra = {
    tunnelIp = "10.99.0.2";
    lanIp = "192.168.1.100";
    podCidr = "10.42.9.0/24";
    publicKey = "PoBMJpAuKhMdrLyn0zdhRp4ZtSYTR70HOU9tv4hIqGA=";
  };
  seir = {
    tunnelIp = "10.99.0.3";
    lanIp = "192.168.1.35";
    podCidr = "10.42.10.0/24";
    publicKey = "7pdfZW5abgVqIzxU21IHyk7tZtyJz0/z5FUCVpxg6gk=";
  };
  mm01 = {
    tunnelIp = "10.99.0.11";
    lanIp = "192.168.1.111";
    podCidr = "10.42.6.0/24";
    publicKey = "oBdhCY8YQA67kNPlxkqRKJXC3c6UXmoIZosm3p+xiH0=";
  };
  mm02 = {
    tunnelIp = "10.99.0.12";
    lanIp = "192.168.1.112";
    podCidr = "10.42.5.0/24";
    publicKey = "HrSNKl4RbDMbw1VJsEYok6ewuGQIUOAyIjdkBJWxnUw=";
  };
  mm03 = {
    tunnelIp = "10.99.0.13";
    lanIp = "192.168.1.113";
    podCidr = "10.42.4.0/24";
    publicKey = "UPqRQQ6g6wXHSlkXO6o4CtZXNi/rphaJVn2yEb+lniY=";
  };
  mm04 = {
    tunnelIp = "10.99.0.14";
    lanIp = "192.168.1.114";
    podCidr = "10.42.3.0/24";
    publicKey = "03DDfp/Zv4YF46wuSNR3iZNENCUuGYyfgZpqdzSjHH0=";
  };
  mm05 = {
    tunnelIp = "10.99.0.15";
    lanIp = "192.168.1.115";
    podCidr = "10.42.1.0/24";
    publicKey = "s8TMkmpDtD3f7513m0pZzf8w0fht6TnTL0wV6bOaXik=";
  };
  traitor = {
    tunnelIp = "10.99.0.16";
    lanIp = "192.168.1.130";
    podCidr = "10.42.8.0/24";
    publicKey = "xS8I6tI4Xb+5H1Q1Yq7C/B8H3vK9kF1ZtY0O1J8v1FE=";
  };
  hp01 = {
    tunnelIp = "10.99.0.21";
    lanIp = "192.168.1.121";
    podCidr = "10.42.0.0/24";
    publicKey = "s8N/rBCKx3UgQrbBS4wxRLTxzhWVpQMVPk7fXTgfG2c=";
  };
  hp02 = {
    tunnelIp = "10.99.0.22";
    lanIp = "192.168.1.122";
    podCidr = "10.42.2.0/24";
    publicKey = "OYBqNLBjIgO6XnOJpBJKWhD9LHipmO7cpeozQ/npZjg=";
  };
  hp03 = {
    tunnelIp = "10.99.0.23";
    lanIp = "192.168.1.123";
    podCidr = "10.42.7.0/24";
    publicKey = "9zSWY4+ByeZR/WVLwe3lZedcropEqkX/nBGvohi64HA=";
  };

  # GCP VMs
  gcp-cp = {
    tunnelIp = "10.99.0.30";
    podCidr = "10.42.13.0/24";
    publicKey = "t98dA7VtV25p0T9AZ4/gFv/4c11/E3WlaGNy3LPyhVA=";
  };
  gcp-hydra = {
    tunnelIp = "10.99.0.31";
    publicKey = "q8tlELO5I8v3jXkP4EzukvbfdIj5SqllKuU+xwRZ3Hs=";
  };
}
