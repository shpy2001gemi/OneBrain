# Local KU CLI projection v1

KU-CLI-001 projects the approved eleven operations in
[KU product workflow](KU_PRODUCT_WORKFLOW_PROFILE_V1.md) through the existing
[authenticated REST transport](KU_LOCAL_REST_PROFILE_V1.md). This is a CLI
adapter specification; no new service operation, DTO, authority or wire field.

`onebrain ku` provides `reserve`, `prepare`, `preview`, `save`, `get`, `list`,
`search`, `revise`, `export`, `status`, `cancel`, and `reconcile`. `reserve` is
the existing Base reservation transport, not a twelfth KU operation.

- `prepare`, `revise`, `save`, and `export` take `--payload-file PATH`, containing
  exactly the corresponding generated request DTO. Read at most 1 MiB; reject
  unknown/duplicate fields, explicit null optionals, malformed typed IDs and
  invalid bounds before any network request. The path is never sent to the node.
- `preview`, `cancel`, `reconcile` take `--operation-id`; `get` takes
  `--object-cid`; `status` optionally takes `--operation-id`.
- `list` and `search` take `--limit` (1–256, default 100) and optional opaque
  `--continuation`; `search` requires `--query`.
- Global `--max-items`, `--max-bytes`, `--max-work-units` use REST bounds and
  ceilings. `--api-url` defaults to `http://127.0.0.1:4280`; only loopback URLs
  without credentials, query or fragment are admitted. Redirects and proxies
  are disabled. `--api-token` or `ONEBRAIN_API_TOKEN` supplies authentication.
- Read status for the current session before each POST. Do not auto-retry a
  mutation, replace operation/idempotency IDs, replay extraction, or auto-save.
  After lost replies, refresh and reconcile the original operation explicitly.
- Save is an explicit private action with the exact object set and idempotency
  key. Cancel requires typing the exact operation ID on stdin; no `--yes` or
  confirmation flag bypass. Export never publishes: public exchange can read
  only already-public records, and private archive requires separate Base
  management authority. No CLI grant or plaintext fallback is introduced.
- stdout is the full JSON envelope, retaining typed identities, canonical
  previews, lifecycle, partial coverage, limitations, continuations and Base
  failure policies. stderr explains local/private/pending scope. A transport
  failure reports unknown outcome and reconcile-before-retry, without replay.
  API failures exit nonzero while preserving the bounded failure envelope.
- No source intake or semantic-draft lowering is invented. Preparation requires
  host-admitted sources and a resolved draft or supported encoder. Readiness
  does not qualify a model. Empty local results say nothing about the network.

Tests must cover parsing/validation, exact cancellation, response bounds,
failure/reconcile projection and a real local API/node workflow using isolated
test custody and Registry fixtures. No live owner data/model is required.
