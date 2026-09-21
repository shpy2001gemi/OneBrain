# Local OBP CLI projection v1

OBP-CLI-001 registers these spellings before implementation. This is a thin
projection of the accepted D-029 API; it adds no operation or host intake port.

Build with `--features vnext-outbound-first`. Compilation does not request or
start networking. `onebrain obp` never constructs a node or modifies a live host.

Global options: `--api-url` (default `http://127.0.0.1:4280`, numeric loopback
HTTP(S) origin only), `--api-token` (otherwise `ONEBRAIN_API_TOKEN`). JSON is the
stable stdout format; guidance and confirmation use stderr. Exit 0 means a valid
successful API response, not delivery; exit 1 is API/client failure, 2 is syntax.

| CLI command after `onebrain obp` | API operation / route | Required options |
|---|---|---|
| `status` | GET status | none |
| `source-list` | source_list / query | --context-file, --payload-file |
| `reservation-list` | reservation_list / query | --context-file, --payload-file |
| `route-status` | route_status / query | --context-file, --payload-file |
| `intent-status` | intent_status / query | --context-file, --payload-file |
| `configure` | configure / commands | --context-file, --payload-file |
| `source-admit` | source_admit / commands | --context-file, --payload-file |
| `source-set-enabled` | source_set_enabled / commands | --context-file, --payload-file |
| `refresh` | refresh / commands | --context-file, --payload-file |
| `route-request` | route_request / commands | --context-file, --payload-file |
| `intent-retry` | intent_retry / commands | --context-file, --payload-file |
| `network-kill` | network_kill / commands | --context-file, --payload-file |
| `network-reenable` | network_reenable / commands | --context-file, --payload-file |
| `reconcile` | POST reconcile, metadata only | --context-file, --idempotency-key |

The context file is an exact saved successful `status` envelope. Payload files
contain only the exact accepted request DTO, including caller-retained
idempotency_key and expected_generation for commands. IDs are full lowercase
64-hex. Pages use limit 1..256 and optional opaque obc1 continuation. Files and
HTTP bodies are bounded to 1 MiB; duplicate/unknown fields and null optionals
reject. Files are read locally only as DTOs, never uploaded as discovery bytes.

All eight mutations display the exact operation and payload, and require stdin
to supply the exact idempotency key. There is no --yes bypass. Configure preview
includes both independent opt-ins; advertising requires the existing host-side
public-fields/expiry preview before the host grants management authority.
Host-management commands additionally require `ONEBRAIN_OBP_MANAGEMENT_TOKEN`,
sent only as X-OneBrain-OBP-Management on that command. Tokens are never printed.
The host must mint that scoped, expiring capability and register typed inputs
in-process. This CLI does not mint grants, read private keys, import raw invitation
files, or supply raw addresses. Missing/expired host inputs or grants fail closed.

Source removal means the registered `source_set_enabled` with enabled=false;
it preserves replay floors. The accepted API has no deletion, intent cancellation,
peer directory or outbox-list operation. Inspect an exact route/intent instead.
No aliases claim those unsupported actions.

After any unknown transport outcome retain the original payload/key/context.
Read fresh status, check dataset identity, then reconcile the original key using
the new context. Never automatically replace context, generation or key, replay,
or restart an expired page. local_not_found proves only current-dataset absence.
Exact command replay (including after kill) preserves the old expected_generation;
the server checks recorded identity before rejecting generation. Metadata completed
is not a full result or permission to repeat effects.

Output preserves API states, IDs, limitations and reconcile flags. Connected must
match the requested expected peer; acknowledged requires the contract checkpoint.
Empty pages/path_limited are bounded local results, not global absence. Discovery,
reservation, socket success and command completion imply no delivery, trust,
publication, truth or reward. No model, mobile, rollout or NAT qualification work.
