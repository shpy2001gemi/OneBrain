# onebrain-extension

> Least-privilege OneBrain Manifest V3 Browser Extension Adapter (`P10-SURFACES-SYNC-001` §4.1).

| Field | Value |
|---|---|
| **Tech** | Zero-dependency TypeScript + Manifest V3 WebExtensions API |
| **Capability Token** | `obc_surface1.<claims>.<mac>` (`surface_kind === "extension"`) |
| **Host Pinning** | `http://127.0.0.1:4280/*` (loopback only) |
| **Status** | Implemented (`P10 §4.1`, `NEG-P10-002`, `T-BCI-04`) |

## Security & Capability Ceiling Invariants

1. **Content-Script Sanitization Before IPC (`AI-003` — `src/sanitizer.ts`):**
   - Strips sensitive HTTP headers case-insensitively (`Cookie`, `Set-Cookie`, `Authorization`, `Proxy-Authorization`, `X-Api-Key`, `X-OneBrain-OBP-Management`).
   - Strips URL userinfo (`user:pass@host`) and sensitive query/hash tokens (`access_token`, `refresh_token`, `id_token`, `token`, `session`, `session_id`, `sig`, `signature`, `code`, `api_key`, `key`, `secret`, `password`).
   - Strips `<input type="password">`, `autocomplete="current-password|new-password|one-time-code|cc-*"`, hidden `<input type="hidden">` CSRF/auth controls, `<script>`, `<iframe>`, and inline `on*` / `javascript:` handlers before bytes leave the content script, recording `redacted_element_count` and `stripped_param_names` in `SanitizedWebCaptureV1`.
2. **No Silent Escalation (`NEG-P10-002` — `src/capabilityClient.ts`):**
   - Rejects `obm1.` admin/archive tokens fail-closed.
   - Blocks bulk `PrivateVault` & Base ABI routes (`/api/kus`, `/api/export`, `/api/backup`, `/api/restore`, `/api/base/v1/*`).
   - Hard-blocks step-2 `PUBLIC` confirmation inside the extension and returns a deterministic `HostConfirmationHandoff` (`http://127.0.0.1:4280/ku?review_draft=...`) so publication confirmation occurs exclusively in `onebrain-desktop` or `onebrain-web`.
3. **Somatic Isolation (`T-BCI-04`):**
   - Hard-blocks all `P9` somatic/BCI endpoints and neural intent message types.
4. **Honest Surface Badges (`RUN-004` / `P0-ADR-001`):**
   - Renders canonical badges (`[Private · Local Only]`, `[Public · Unassessed]`, `[Local Store Only]`) and rejects legacy `FULL`, `GLOBAL`, or `CLOSED` aliases.

## Verification

```bash
node --experimental-strip-types --test src/onebrain-extension/tests/extension.test.ts
```
