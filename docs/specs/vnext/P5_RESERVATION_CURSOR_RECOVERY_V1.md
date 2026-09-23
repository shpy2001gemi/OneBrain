# P5 reservation sequence recovery v1

This additive correction covers repeated signed P5 sessions on retained node
and relay identities. It does not change the relay wire objects, enable
networking by default, or relax the three-host production gate.

Relay reservation request sequence is global to a target NodeID at a relay,
not to a P5 qualification session. On relay restart, the service must recover
the highest exact target/sequence key from its existing durable control table
before accepting another request. The next request must be exactly that floor
plus one. Replay, gaps and overflow reject before any grant. The relay must
not clear the old control table or reissue prior grants.

The node-owned P5 agent's reservation cursor uses a stable binding derived
from its persistent public identity and the relay NodeID. Other agent, admin
and signer command cursors remain signed-session-bound. On an existing host,
the first stable cursor must be seeded to at least the maximum preserved legacy
reservation cursor sequence before a new session. If a legacy cursor exists
without this explicit migration, reservation admission fails closed. Migration
must retain the original cursor bytes and record their hashes, bindings and
sequence; it must not reset the relay's durable floor or rotate identities.

Evidence requires a relay-restart test with an old durable request and a
second request at its exact successor, rejection of the replayed sequence,
stable node cursor binding across sessions, a retained-floor migration record
for each existing host/relay pair, and live signed happy-case evidence. A
successful happy case alone remains nonqualifying for the P5 fault gate or the
separate two-consumer-network OBP-QA-001 acceptance gate.
