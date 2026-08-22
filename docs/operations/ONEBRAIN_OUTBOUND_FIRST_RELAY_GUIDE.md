# OneBrain Outbound-First Relay Operations

This guide deploys the permissionless relay sidecar used by outbound-first OBP.
Running a relay requires no owner approval and grants no authority over node
identity, payloads, routes, or knowledge. A relay contributes availability only.
Ordinary nodes initiate outbound connections and never require a public port,
UPnP, manual NAT, or a central OneBrain server.

## Closed three-runner topology

- runner-a: outbound-only node behind provider NAT. Real bidirectional probing
  proved that `204.12.245.228:10042 -> 192.168.122.4:41000` admits inbound UDP
  but does not return traffic on the same public tuple, so it is not a valid
  relay advertisement.
- runner-b: relay TLS/TCP `163.61.111.23:443`; both other physical hosts proved
  bidirectional reachability before this topology was selected.
- runner-c: relay UDP `103.77.214.30:41000` and TLS/TCP
  `103.77.214.30:443`.
- all three runners remain ordinary OBP nodes and reach both public relays
  through outbound connections.
- every node reserves at both relay NodeIDs. No private or NAT-derived endpoint
  is advertised: the co-resident nodes on runner-b/c must authenticate the same
  descriptor keys as remote nodes, while runner-a reaches both relays outbound.

Only endpoints proven remotely from both other physical hosts enter the signed
inventory. A relay descriptor is non-authoritative and expires; the controller
retains its contiguous same-key/same-config sequence chain.

## Verify the immutable bundle

Set `EXPECTED_GENERATION` to the installed content-addressed generation. Never
render a unit from the `current` selector.

```bash
candidate_generation="$(sudo readlink -f /opt/onebrain/base-v1/current)"
sudo test "$candidate_generation" = "/opt/onebrain/base-v1/$EXPECTED_GENERATION"
sudo "$candidate_generation/scripts/verify.sh" --root "$candidate_generation"
sudo "$candidate_generation/bin/p5_multi_host_agent_v2" --print-compiled-binding
sudo "$candidate_generation/bin/relay_preflight_probe" --print-compiled-binding
```

## Install relay-b or relay-c

Copy exactly one reviewed bundle config to a temporary mode-0644 file, compare
it byte-for-byte with `config/relay-b.json` or `config/relay-c.json`, then run:

```bash
sudo useradd --system --home /var/lib/onebrain/relay-p5 --shell /usr/sbin/nologin onebrain-relay
sudo install -d -o onebrain-relay -g onebrain-relay -m 0700 /var/lib/onebrain/relay-p5
sudo install -d -o root -g root -m 0755 /etc/onebrain
candidate_generation="$(sudo readlink -f /opt/onebrain/base-v1/current)"
sudo test "$candidate_generation" = "/opt/onebrain/base-v1/$EXPECTED_GENERATION"
sudo install -o root -g root -m 0644 "$candidate_generation/config/$RELAY_CONFIG" /etc/onebrain/relay-p5.json
relay_unit_tmp="$(mktemp)"
sed "s|@CANDIDATE_GENERATION@|$candidate_generation|g" "$candidate_generation/units/onebrain-relay-p5.service" > "$relay_unit_tmp"
! grep -Fq '@CANDIDATE_GENERATION@' "$relay_unit_tmp"
grep -Fq "ExecStart=$candidate_generation/bin/onebrain-relay " "$relay_unit_tmp"
sudo install -o root -g root -m 0644 "$relay_unit_tmp" /etc/systemd/system/onebrain-relay-p5.service
rm -f "$relay_unit_tmp"
sudo -u onebrain-relay "$candidate_generation/bin/onebrain-relay" generate-identity --output /var/lib/onebrain/relay-p5/identity.key
sudo test "$(sudo stat -c '%a %U:%G' /var/lib/onebrain/relay-p5/identity.key)" = '600 onebrain-relay:onebrain-relay'
sudo -u onebrain-relay "$candidate_generation/bin/onebrain-relay" initialize-state --config /etc/onebrain/relay-p5.json
sudo "$candidate_generation/bin/onebrain-relay" verify-config --config /etc/onebrain/relay-p5.json
sudo systemctl daemon-reload
sudo systemctl enable --now onebrain-relay-p5.service
sudo systemctl show onebrain-relay-p5.service --property=LoadState --property=ActiveState --property=SubState --no-pager
```

The shipped unit is a read-only reviewed template. The `sed` step binds it to
the verified installed manifest generation; the rendered unit must contain
neither the placeholder nor the mutable `current` selector. Both selected
relay hosts bind TCP 443, so the reviewed unit grants only
`CAP_NET_BIND_SERVICE`; runner-a does not install the relay unit.

## P5 service and SSH boundary

Create the service users `onebrain-p5-agent`,
`onebrain-p5-receipt-signer`, and `onebrain-p5-identity-signer`; create locked
SSH-only users `onebrain-p5-probe-ssh`, `onebrain-p5-control-ssh`, and
`onebrain-p5-admin-ssh`. Homes and `.ssh` directories are 0700 and each
`authorized_keys` is 0600. The same inventory-bound controller public key has
three separate `restrict,command=` lines pointing respectively to immutable
`relay_preflight_probe`, immutable `p5_agent_ctl_v2`, and
`/usr/bin/sudo -n -- <immutable>/bin/p5_admin_ctl_v2`. No interactive shell or
caller-selected remote command is permitted. The sudoers entry names only the
no-argument admin binary, disables `setenv`, and must pass `visudo -cf`.

Generate receipt and identity keys exactly once with each signer's closed
`generate-key` mode, under its own 0700 data root. Private key bytes never enter
the bundle, inventory, evidence, or agent process. Record only canonical
`print-public` outputs. The agent UID must get `EACCES` opening either key while
its fixed signer-socket clients work.

Render and install the six files under `units/` for receipt signer, identity
signer, and agent as root:root 0644. Service `ExecStart` values are bound to the
verified immutable manifest generation at installation; socket files contain
no generation placeholder. Leave them disabled and stopped until a signed P5
session exists.
Bootstrap only verifies authority and atomically installs
`/run/onebrain/p5-v2/current-session.json`; it changes no unit/network state.
A separate signed `PrepareSession` starts receipt signer, builds the namespace,
starts identity signer, then starts the agent and returns a signed receipt only
after the final state is observed.

## Namespace and management safety

The Linux qualification creates `onebrain-p5-v2`, veth
`obp5h0`/`obp5n0`, and `10.254.28.0/29`. It snapshots forwarding, route,
firewall, UFW, and SSH state. The session owns an exact source-limited NAT
postrouting rule. UFW-active hosts receive only the necessary session-commented
routed/co-resident rules; UFW is never enabled, disabled, reset, or reloaded.
No rule changes SSH, TCP/22, provider TCP/10041, or a management interface.
After every mutation both the existing SSH channel and a new pinned-host-key
connection must pass. Fault rules exist only on `obp5n0` in the namespace.

Cleanup first stops agent and identity services, clears exact session-owned
fault/network objects, restores prior forwarding/firewall bytes, and obtains a
signed cleanup receipt while receipt signer and session config remain alive.
Only after the controller durably verifies that receipt may finalization stop
the receipt signer and remove the matching session config. A failed finalizer
makes the host non-reusable; it never invents qualification.

## Relay loss and replacement

Nodes learn relays from bounded signed discovery records and authenticate each
relay NodeID/SPKI. If one relay disappears, already admitted alternatives are
tried deterministically. A newly self-hosted relay does not need owner approval,
but it becomes usable only after signature, global-address, possession,
freshness, resource-budget, and live transport checks. Fake discovery records
cannot redirect an authenticated session to a different relay or peer.
