# OBP-MIG-001 — legacy seed migration and rollback map

2026-09-24. Work is on `codex/obp-mig-001-retire-legacy-seed`, branched from
published main `940fa61` after D-041 merged OBP-QA-001. The owner accepted
functional QA and explicitly skipped consumer-NAT qualification for task 18;
that skip is not a network qualification. This migration changes the normal
startup path without activating OBP by default or altering old durable data.
Validated implementation `1d649ce` is published on the retained task branch
and merged under D-042 as `0604b55`.

## Call-site and packaging inventory

| Location | Before | Current treatment |
|---|---|---|
| `onebrain-cli/src/main.rs` `start` | Always dialed legacy `--seeds`, mDNS, UPnP and hard-coded `SeedClient` domains after startup | These operations run only with `--legacy-seed-compat`. `--seeds` requires that flag. When a vNext feature is explicitly requested without compatibility, CLI starts the node-owned vNext runtime without a legacy TCP listener. |
| `onebrain-cli/src/cli/network.rs` REPL `connect` | Direct legacy peer handshake | Dispatch rejects it unless startup selected `--legacy-seed-compat`. Status and offline KU commands remain available. |
| `onebrain-node/src/node.rs` | `start_network()` starts the legacy TCP listener plus an active vNext runtime | Existing method remains for compatibility and callers such as stock Desktop. New `start_vnext_network_only()` starts only the node-owned vNext runtime. Shutdown joins either mode. |
| `onebrain-node/src/seed_client.rs`, `NodeConfig.seeds`, Desktop TOML `seeds` | Legacy client and configuration | Retained byte-for-byte as compatibility interfaces; no automatic trust, identity or source migration. Desktop does not call `SeedClient`; stock Desktop remains local-only for OBP without trusted host ports. |
| `onebrain-seed` standalone binary | Could bind TCP/JSON immediately | Requires `--legacy-seed-compat` before binding. The package remains buildable for rollback, distinct from vNext `onebrain-relay`. |
| Linux systemd/Docker and Windows NSSM examples | Legacy daemon startup without a compatibility marker | Explicitly label the daemon and pass the flag in operator-run examples. The Docker image itself has no opt-in flag in its default command. |

The workspace `onebrain-seed` package, service/unit examples, Windows/Linux
deployment notes, Arduino proposal and CLI reference were audited. The
daemon is not in the P5 source-free bundle or a vNext package. No current
call-site/packaging audit supports deleting the legacy crate, CLI config
field, peer-memory bytes, listener or on-disk records, so they are retained.

## Normal and rollback paths

Normal OBP clients (`onebrain obp`, local Web and Desktop) continue to use
the accepted shared local API and node-owned service. They neither invoke
`SeedClient` nor treat the legacy daemon as a vNext bootstrap/relay. An
explicit CLI vNext feature request uses the vNext-only startup method. It
still needs its existing caller-owned Vault, policy, signer and host grants;
this task introduces no provisioning endpoint or implicit opt-in.

For an approved rollback window, operators can start the CLI with
`onebrain start --legacy-seed-compat` and optionally `--seeds <addr>`, then
use the legacy REPL `connect` command. The standalone daemon requires
`onebrain-seed --legacy-seed-compat`. These flags deliberately select old
TCP/JSON behavior. They do not turn on an OBP lane, import a seed into signed
vNext bootstrap trust, clear durable kill state or delete any dataset bytes.
Removing the flag on the next startup returns to the default no-seed path;
old configuration and peer memory remain available for another explicit
rollback. Existing legacy TCP listener semantics on a plain `onebrain start`
remain outside this seed-path migration, so no claim is made that all legacy
peer networking is removed.

## Verification and limits

- CLI with `vnext-outbound-first`: 52 unit and 2 integration tests passed.
  Default/base-only CLI: 43 unit and 2 integration tests passed. The new
  command parser rejects `--seeds` without explicit compatibility and keeps
  the opt-in rollback syntax. An additional real CLI process test passed in
  both feature modes: default `start` exited without attempting legacy seed
  discovery.
- `onebrain-seed`: integration test confirms invocation without the flag
  exits before startup/binding. Its test suite passed.
- Node `vnext_node_runtime`: 8 tests passed, including the new vNext-only
  listener and explicit compatibility restart case. Node library: 253 passed
  with two test threads. A first parallel run had 252 pass and one unrelated
  PoMV timeout; that test passed alone and in the two-thread rerun.
- Desktop lifecycle: 6 tests passed, confirming its retained local/legacy
  start behavior. CLI `vnext-network-runtime` without defaults passes
  `cargo check`. Focused Rust formatting, aggregate vNext contract validator,
  whitespace and local link validation pass.
- No live host, daemon service, firewall, NAT, dataset or remote P5 state was
  modified. Real platform migration and the duration of any operator rollback
  window are not claimed by these local tests.
