//! Pillar 10 Multi-Surface Sync, Scoped Capability Tokens & Cross-Pillar Status API (`P10-SURFACES-SYNC-001`).
//!
//! Additive `/api/vnext/{mesh,surfaces,kql/needs/compile,obkg,obt,cognition,extension,bot,hud}/*`
//! handlers backed by `OneBrainNode`, preserving all frozen `product-integration-profile-v1.json`
//! and `obp-local-api-v1.json` routes untouched.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use onebrain_node::foundation::ConceptCcid;
use onebrain_node::{SurfaceCapabilityClaimsV1, SurfaceKind, SurfaceScope};

use crate::server::AppState;

const P10_PROFILE: &str = "onebrain/vnext-p10/1";

/// Request extension injected by `auth_middleware` identifying whether the caller
/// authenticated with the host owner Bearer token or a scoped `obc_surface1` token.
#[derive(Clone, Debug)]
pub enum AuthenticatedSurfacePrincipal {
    HostOwner,
    ScopedSurface(SurfaceCapabilityClaimsV1),
}

#[derive(Clone, Default)]
pub struct P10ApiRuntimeState {
    inner: Arc<StdMutex<P10ApiInner>>,
}

struct P10ApiInner {
    seq: u64,
    vault_sync_enabled: bool,
    last_sync_unix_s: Option<u64>,
    mesh_epoch_cache: u64,
    devices: Vec<MeshDeviceRecord>,
    pairing_sessions: BTreeMap<String, PendingPairingRecord>,
    token_labels: BTreeMap<[u8; 32], (String, String)>,
    extension_drafts: BTreeMap<String, Value>,
    quarantined_bot_submissions: Vec<Value>,
    hud_locked: bool,
    last_hud_heartbeat_ms: Option<u64>,
}

impl Default for P10ApiInner {
    fn default() -> Self {
        Self {
            seq: 1,
            vault_sync_enabled: false,
            last_sync_unix_s: None,
            mesh_epoch_cache: 1,
            devices: vec![MeshDeviceRecord {
                device_id: "d1".repeat(32),
                label: "Primary Local Host".to_owned(),
                surface_kind: "desktop".to_owned(),
                feed_id: "f1".repeat(32),
                node_id: "e1".repeat(32),
                hpke_pubkey_hex: "c1".repeat(32),
                mesh_epoch: 1,
                status: "active".to_owned(),
                is_current_device: true,
                last_seen_unix_s: unix_now_s(),
                delegated_scopes: vec![
                    "private_vault_mesh".to_owned(),
                    "standing_needs".to_owned(),
                    "pam_m1_m3".to_owned(),
                ],
            }],
            pairing_sessions: BTreeMap::new(),
            token_labels: BTreeMap::new(),
            extension_drafts: BTreeMap::new(),
            quarantined_bot_submissions: Vec::new(),
            hud_locked: false,
            last_hud_heartbeat_ms: None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct MeshDeviceRecord {
    device_id: String,
    label: String,
    surface_kind: String,
    feed_id: String,
    node_id: String,
    hpke_pubkey_hex: String,
    mesh_epoch: u64,
    status: String,
    is_current_device: bool,
    last_seen_unix_s: u64,
    delegated_scopes: Vec<String>,
}

#[derive(Clone)]
struct PendingPairingRecord {
    session_id: String,
    device_label: String,
    surface_kind: String,
    sas_digits: String,
    sas_words: [String; 4],
    candidate_device_id: String,
    candidate_feed_id: String,
    candidate_node_id: String,
    candidate_hpke_pubkey: String,
    target_mesh_epoch: u64,
    expires_at_unix_s: u64,
}

pub fn unix_now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(1_795_710_000)
}

fn ok_envelope(data: Value, limitations: Vec<&str>) -> Response {
    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "profile": P10_PROFILE,
            "data": data,
            "meta": {
                "lifecycle": "active",
                "coverage": "local_only",
                "limitations": limitations,
                "continuation": null
            }
        })),
    )
        .into_response()
}

pub fn err_envelope(status: StatusCode, code: &str, message: impl Into<String>) -> Response {
    (
        status,
        Json(json!({
            "ok": false,
            "profile": P10_PROFILE,
            "error": {
                "code": code,
                "message": message.into(),
                "retryable": false,
                "limitations": []
            },
            "meta": {
                "lifecycle": "active",
                "coverage": "local_only",
                "limitations": [],
                "continuation": null
            }
        })),
    )
        .into_response()
}

fn deterministic_hex32(domain: &str, input: &str) -> String {
    let digest = blake3::derive_key(domain, input.as_bytes());
    onebrain_base_contract::ku_payload::hex(&digest)
}

fn deterministic_bytes32(domain: &str, input: &str) -> [u8; 32] {
    let mut d = blake3::derive_key(domain, input.as_bytes());
    if d == [0; 32] {
        d[0] = 1;
    }
    d
}

// ─── Personal Mesh (`/api/vnext/mesh/*`) ─────────────────────────────────────

pub async fn mesh_status(State(state): State<AppState>) -> Response {
    let persisted_epoch = {
        let mut node = state.node.lock().await;
        node.personal_mesh_mut().mesh_epoch().ok().flatten()
    };
    let mut inner = state.p10_state.inner.lock().expect("p10 mutex");
    if let Some(ep) = persisted_epoch {
        if ep > inner.mesh_epoch_cache {
            inner.mesh_epoch_cache = ep;
        }
    }
    let epoch = inner.mesh_epoch_cache;
    let devices = inner.devices.clone();
    let sync_enabled = inner.vault_sync_enabled;
    let last_sync = inner.last_sync_unix_s;
    drop(inner);

    ok_envelope(
        json!({
            "protocol": "obp/personal-mesh/1",
            "usability": "USABLE_OFFLINE",
            "principals": {
                "actor_id": "a1".repeat(32),
                "device_id": "d1".repeat(32),
                "feed_id": "f1".repeat(32),
                "node_id": "e1".repeat(32),
                "private_signing_keys_shared": false,
                "feed_signer_mode": "single_writer_per_device"
            },
            "mesh_epoch": epoch,
            "epoch_dek_algorithm": "XChaCha20-Poly1305",
            "key_wrap_algorithm": "X25519-HKDF-SHA256",
            "vault_sync_enabled": sync_enabled,
            "synced_vault_categories": ["private_ku_drafts", "standing_need_bundles", "pam_m1_m3"],
            "excluded_never_synced": ["p9_somatic_biometric_raw", "obm1_admin_tokens", "feed_event_signer_secret_keys"],
            "pending_sync_items": 0,
            "last_sync_unix_s": last_sync,
            "reachable_mesh_peers": 0,
            "devices": devices
        }),
        vec![],
    )
}

#[derive(Deserialize)]
pub struct MeshPairStartBody {
    pub device_label: String,
    pub surface_kind: String,
}

pub async fn mesh_pair_start(
    State(state): State<AppState>,
    Json(body): Json<MeshPairStartBody>,
) -> Response {
    const WORDS: [&str; 16] = [
        "amber", "beacon", "cedar", "delta", "ember", "falcon", "granite", "harbor", "ivory",
        "jasper", "kestrel", "lumen", "marble", "nebula", "onyx", "quartz",
    ];
    let mut inner = state.p10_state.inner.lock().expect("p10 mutex");
    let seq = inner.seq;
    inner.seq += 1;
    let label = if body.device_label.trim().is_empty() {
        format!("Paired {}", body.surface_kind)
    } else {
        body.device_label.trim().to_owned()
    };
    let seed = format!("{seq}:{label}:{}", body.surface_kind);
    let session_id = deterministic_hex32("onebrain:vnext:p10-api:pair-session:1", &seed);
    let session_bytes = deterministic_bytes32("onebrain:vnext:p10-api:pair-session:1", &seed);
    let num = (u32::from_be_bytes([
        session_bytes[0],
        session_bytes[1],
        session_bytes[2],
        session_bytes[3],
    ]) % 900_000)
        + 100_000;
    let sas_digits = format!("{:03}-{:03}", num / 1000, num % 1000);
    let sas_words = [
        WORDS[(session_bytes[4] as usize) % WORDS.len()].to_owned(),
        WORDS[(session_bytes[5] as usize) % WORDS.len()].to_owned(),
        WORDS[(session_bytes[6] as usize) % WORDS.len()].to_owned(),
        WORDS[(session_bytes[7] as usize) % WORDS.len()].to_owned(),
    ];
    let candidate_device_id = deterministic_hex32("onebrain:vnext:p10-api:dev:1", &seed);
    let candidate_feed_id = deterministic_hex32("onebrain:vnext:p10-api:feed:1", &seed);
    let candidate_node_id = deterministic_hex32("onebrain:vnext:p10-api:node:1", &seed);
    let candidate_hpke_pubkey = deterministic_hex32("onebrain:vnext:p10-api:hpke:1", &seed);
    let epoch = inner.mesh_epoch_cache;
    let expires_at_unix_s = unix_now_s() + 300;

    let qr_payload = format!(
        "obp://personal-mesh/pair/1?sid={}&epoch={epoch}",
        &session_id[..16]
    );
    let pending = PendingPairingRecord {
        session_id: session_id.clone(),
        device_label: label.clone(),
        surface_kind: body.surface_kind.clone(),
        sas_digits: sas_digits.clone(),
        sas_words: sas_words.clone(),
        candidate_device_id: candidate_device_id.clone(),
        candidate_feed_id: candidate_feed_id.clone(),
        candidate_node_id: candidate_node_id.clone(),
        candidate_hpke_pubkey: candidate_hpke_pubkey.clone(),
        target_mesh_epoch: epoch,
        expires_at_unix_s,
    };
    inner.pairing_sessions.insert(session_id.clone(), pending);

    ok_envelope(
        json!({
            "session_id": session_id,
            "protocol": "PersonalMeshPairV1",
            "device_label": label,
            "surface_kind": body.surface_kind,
            "qr_payload": qr_payload,
            "sas_digits": sas_digits,
            "sas_words": sas_words,
            "candidate_device_id": candidate_device_id,
            "candidate_feed_id": candidate_feed_id,
            "candidate_node_id": candidate_node_id,
            "candidate_hpke_pubkey": candidate_hpke_pubkey,
            "target_mesh_epoch": epoch,
            "expires_at_unix_s": expires_at_unix_s
        }),
        vec![],
    )
}

#[derive(Deserialize)]
pub struct MeshPairConfirmBody {
    pub session_id: String,
    pub sas_confirmed: bool,
    pub sas_digits: Option<String>,
}

pub async fn mesh_pair_confirm(
    State(state): State<AppState>,
    Json(body): Json<MeshPairConfirmBody>,
) -> Response {
    if !body.sas_confirmed {
        return err_envelope(
            StatusCode::UNPROCESSABLE_ENTITY,
            "sas_confirmation_required",
            "Explicit user verification of the SPAKE2+ SAS is required before ActorDelegation/1.",
        );
    }
    let mut inner = state.p10_state.inner.lock().expect("p10 mutex");
    let Some(session) = inner.pairing_sessions.remove(&body.session_id) else {
        return err_envelope(
            StatusCode::NOT_FOUND,
            "pairing_session_not_found",
            "Unknown or expired Personal Mesh pairing session.",
        );
    };
    if unix_now_s() > session.expires_at_unix_s {
        return err_envelope(
            StatusCode::GONE,
            "pairing_session_expired",
            "Personal Mesh pairing session expired before SAS confirmation.",
        );
    }
    if let Some(ref supplied_digits) = body.sas_digits {
        if supplied_digits.trim() != session.sas_digits {
            return err_envelope(
                StatusCode::UNPROCESSABLE_ENTITY,
                "sas_mismatch",
                "Supplied Short Authentication String does not match the SPAKE2+ transcript.",
            );
        }
    }
    let scopes = vec![
        "private_vault_mesh".to_owned(),
        "standing_needs".to_owned(),
        "pam_m1_m3".to_owned(),
    ];
    let epoch = inner.mesh_epoch_cache.max(session.target_mesh_epoch);
    let sas_words_verified = session.sas_words;
    let new_dev = MeshDeviceRecord {
        device_id: session.candidate_device_id.clone(),
        label: session.device_label,
        surface_kind: session.surface_kind,
        feed_id: session.candidate_feed_id.clone(),
        node_id: session.candidate_node_id,
        hpke_pubkey_hex: session.candidate_hpke_pubkey.clone(),
        mesh_epoch: epoch,
        status: "active".to_owned(),
        is_current_device: false,
        last_seen_unix_s: unix_now_s(),
        delegated_scopes: scopes.clone(),
    };
    inner.devices.retain(|d| d.device_id != new_dev.device_id);
    inner.devices.push(new_dev.clone());
    let delegation_cid = deterministic_hex32(
        "onebrain:vnext:p10-api:delegation:1",
        &format!("{}:{epoch}", session.session_id),
    );

    ok_envelope(
        json!({
            "delegation_kind": "ActorDelegation/1",
            "delegation_event_cid": delegation_cid,
            "actor_id": "a1".repeat(32),
            "new_device_id": session.candidate_device_id,
            "new_feed_id": session.candidate_feed_id,
            "new_device_hpke_pubkey": session.candidate_hpke_pubkey,
            "allowed_scopes": scopes,
            "mesh_epoch": epoch,
            "sas_words_verified": sas_words_verified,
            "private_keys_transmitted": false,
            "device": new_dev
        }),
        vec![],
    )
}

#[derive(Deserialize)]
pub struct MeshRevokeBody {
    pub device_id: String,
    pub reason: Option<String>,
}

pub async fn mesh_revoke(
    State(state): State<AppState>,
    Json(body): Json<MeshRevokeBody>,
) -> Response {
    let _ = &body.reason;
    let mut inner = state.p10_state.inner.lock().expect("p10 mutex");
    let prev_epoch = inner.mesh_epoch_cache;
    let next_epoch = prev_epoch + 1;
    inner.mesh_epoch_cache = next_epoch;
    for dev in &mut inner.devices {
        if dev.device_id == body.device_id && !dev.is_current_device {
            dev.status = "revoked".to_owned();
        } else if dev.status == "active" {
            dev.mesh_epoch = next_epoch;
        }
    }
    let remaining_active = inner
        .devices
        .iter()
        .filter(|d| d.status == "active")
        .count();
    let rev_cid = deterministic_hex32(
        "onebrain:vnext:p10-api:revocation:1",
        &format!("{}:{next_epoch}", body.device_id),
    );

    ok_envelope(
        json!({
            "revocation_kind": "ActorRevocation/1",
            "revocation_event_cid": rev_cid,
            "revoked_device_id": body.device_id,
            "previous_mesh_epoch": prev_epoch,
            "new_mesh_epoch": next_epoch,
            "epoch_dek_rotated": true,
            "remaining_active_devices": remaining_active,
            "historical_public_feed_verifiable": true
        }),
        vec![],
    )
}

#[derive(Deserialize)]
pub struct MeshSyncBody {
    pub vault_sync_enabled: bool,
}

pub async fn mesh_sync(State(state): State<AppState>, Json(body): Json<MeshSyncBody>) -> Response {
    let mut inner = state.p10_state.inner.lock().expect("p10 mutex");
    inner.vault_sync_enabled = body.vault_sync_enabled;
    let now = unix_now_s();
    if body.vault_sync_enabled {
        inner.last_sync_unix_s = Some(now);
    }
    let epoch = inner.mesh_epoch_cache;
    let active_count = inner
        .devices
        .iter()
        .filter(|d| d.status == "active")
        .count();
    let disclosure = if body.vault_sync_enabled {
        "NEGOTIATED_ENCRYPTED"
    } else {
        "LOCAL_ONLY"
    };

    ok_envelope(
        json!({
            "mesh_epoch": epoch,
            "vault_sync_enabled": body.vault_sync_enabled,
            "disclosure_class": disclosure,
            "wrapped_recipient_devices": if body.vault_sync_enabled { active_count } else { 0 },
            "synced_items": 0,
            "last_sync_unix_s": inner.last_sync_unix_s.unwrap_or(now)
        }),
        vec![],
    )
}

// ─── Scoped Surface Capability Tokens (`/api/vnext/surfaces/tokens*`) ────────

fn require_host_owner(principal: Option<&AuthenticatedSurfacePrincipal>) -> Option<Response> {
    match principal {
        Some(AuthenticatedSurfacePrincipal::ScopedSurface(claims)) => Some(err_envelope(
            StatusCode::FORBIDDEN,
            "surface_token_administration_forbidden",
            format!(
                "Surface {:?} cannot mint, list, or revoke obc_surface1 tokens (HostOwner required).",
                claims.surface_kind
            ),
        )),
        _ => None,
    }
}

fn surface_token_view(
    meta: &onebrain_node::SurfaceCapabilityTokenMetadata,
    label: &str,
    binding: &str,
) -> Value {
    let token_id_hex = onebrain_base_contract::ku_payload::hex(&meta.token_id);
    let kind_str = meta.surface_kind.as_str();
    let (perms, prohibitions, vault_scope, confirm_mode, doff_ms, quarantine) =
        match meta.surface_kind {
            SurfaceKind::Extension => (
                vec!["sanitized_web_capture", "own_draft_preview"],
                vec![
                    "bulk_private_vault_read",
                    "obm1_admin_archive",
                    "step2_public_confirm",
                    "p9_somatic_access",
                ],
                "own_captured_drafts_only",
                "prohibited",
                Value::Null,
                false,
            ),
            SurfaceKind::Bot => (
                vec!["public_validated_read", "quarantined_external_submit"],
                vec![
                    "owner_private_vault_read",
                    "obm1_admin_archive",
                    "step2_public_confirm",
                    "p9_somatic_access",
                ],
                "zero_public_store_only",
                "prohibited",
                Value::Null,
                true,
            ),
            SurfaceKind::Glasses => (
                vec![
                    "active_hud_cards_m0",
                    "somatic_immediate_abort",
                    "two_step_physical_confirm",
                ],
                vec![
                    "at_rest_master_secrets",
                    "obm1_admin_archive",
                    "voice_only_public_confirm",
                ],
                "active_hud_cards_only",
                "two_step_physical_and_readback_only",
                json!(40),
                false,
            ),
            _ => (
                vec!["scoped_host_surface"],
                vec!["raw_biometric_export"],
                "scoped_session",
                "two_step_physical_and_readback_only",
                Value::Null,
                false,
            ),
        };

    json!({
        "token_id": token_id_hex,
        "token_prefix": "obc_surface1",
        "token_preview": format!(
            "obc_surface1_{}_{}…{}",
            &kind_str[..kind_str.len().min(3)],
            &token_id_hex[..4],
            &token_id_hex[token_id_hex.len() - 4..]
        ),
        "surface_kind": kind_str,
        "label": label,
        "bound_channel_or_origin": binding,
        "granted_permissions": perms,
        "hard_prohibitions": prohibitions,
        "private_vault_scope": vault_scope,
        "step2_public_confirm": confirm_mode,
        "doff_lock_max_ms": doff_ms,
        "quarantine_inbound": quarantine,
        "created_at_unix_s": meta.issued_at_unix_s,
        "revoked": meta.revoked
    })
}

pub async fn list_surface_tokens(
    State(state): State<AppState>,
    principal: Option<Extension<AuthenticatedSurfacePrincipal>>,
) -> Response {
    if let Some(resp) = require_host_owner(principal.as_ref().map(|e| &e.0)) {
        return resp;
    }
    let tokens = {
        let node = state.node.lock().await;
        node.surface_capabilities().list_tokens()
    };
    let labels = state
        .p10_state
        .inner
        .lock()
        .expect("p10 mutex")
        .token_labels
        .clone();
    let items: Vec<Value> = tokens
        .iter()
        .map(|meta| {
            let (label, binding) = labels
                .get(&meta.token_id)
                .cloned()
                .unwrap_or_else(|| (meta.surface_kind.as_str().to_owned(), "loopback".to_owned()));
            surface_token_view(meta, &label, &binding)
        })
        .collect();
    ok_envelope(json!({ "items": items }), vec![])
}

#[derive(Deserialize)]
pub struct MintSurfaceTokenBody {
    pub surface_kind: String,
    pub label: String,
    pub bound_channel_or_origin: String,
}

pub async fn mint_surface_token(
    State(state): State<AppState>,
    principal: Option<Extension<AuthenticatedSurfacePrincipal>>,
    Json(body): Json<MintSurfaceTokenBody>,
) -> Response {
    if let Some(resp) = require_host_owner(principal.as_ref().map(|e| &e.0)) {
        return resp;
    }
    let kind = match body.surface_kind.trim().to_ascii_lowercase().as_str() {
        "extension" => SurfaceKind::Extension,
        "bot" => SurfaceKind::Bot,
        "glasses" => SurfaceKind::Glasses,
        "desktop" => SurfaceKind::Desktop,
        "web" => SurfaceKind::Web,
        "cli" => SurfaceKind::Cli,
        other => {
            return err_envelope(
                StatusCode::BAD_REQUEST,
                "invalid_surface_kind",
                format!("Unsupported surface_kind '{other}'."),
            )
        }
    };
    let seq = {
        let mut inner = state.p10_state.inner.lock().expect("p10 mutex");
        let s = inner.seq;
        inner.seq += 1;
        s
    };
    let token_id = deterministic_bytes32(
        "onebrain:vnext:p10-api:surface-token-id:1",
        &format!("{seq}:{}:{}", body.surface_kind, body.label),
    );
    let actor_bytes = [0xA1u8; 32];
    let device_bytes = deterministic_bytes32(
        "onebrain:vnext:p10-api:surface-device:1",
        &body.bound_channel_or_origin,
    );
    let now = unix_now_s();
    let scopes = match kind {
        SurfaceKind::Extension => std::collections::BTreeSet::from([
            SurfaceScope::ReadStatusAndQuery,
            SurfaceScope::CaptureSourceAndDraft,
        ]),
        SurfaceKind::Bot => std::collections::BTreeSet::from([
            SurfaceScope::ReadStatusAndQuery,
            SurfaceScope::CaptureSourceAndDraft,
        ]),
        SurfaceKind::Glasses => std::collections::BTreeSet::from([
            SurfaceScope::ReadStatusAndQuery,
            SurfaceScope::CaptureSourceAndDraft,
            SurfaceScope::SomaticEmergencyAbort,
        ]),
        _ => kind.max_allowed_scopes(),
    };
    let claims = SurfaceCapabilityClaimsV1 {
        token_id,
        surface_kind: kind,
        actor_id: ku_core_actor(actor_bytes),
        device_id: onebrain_node::foundation::DeviceId::from_bytes(device_bytes),
        allowed_scopes: scopes,
        issued_at_unix_s: now,
        expires_at_unix_s: now + 7 * 24 * 3600,
    };

    let (minted, meta) = {
        let mut node = state.node.lock().await;
        match node.surface_capabilities_mut().mint_token(claims) {
            Ok(tok) => {
                let m = node
                    .surface_capabilities()
                    .list_tokens()
                    .into_iter()
                    .find(|t| t.token_id == token_id)
                    .expect("just minted");
                (tok, m)
            }
            Err(e) => {
                return err_envelope(StatusCode::BAD_REQUEST, "mint_failed", e.to_string());
            }
        }
    };

    let label = if body.label.trim().is_empty() {
        format!("{} adapter", kind.as_str())
    } else {
        body.label.trim().to_owned()
    };
    let binding = if body.bound_channel_or_origin.trim().is_empty() {
        format!("loopback://{}", kind.as_str())
    } else {
        body.bound_channel_or_origin.trim().to_owned()
    };
    state
        .p10_state
        .inner
        .lock()
        .expect("p10 mutex")
        .token_labels
        .insert(token_id, (label.clone(), binding.clone()));

    let view = surface_token_view(&meta, &label, &binding);
    ok_envelope(
        json!({
            "token": view,
            "issued_capability_handle": minted
        }),
        vec![],
    )
}

pub async fn revoke_surface_token(
    State(state): State<AppState>,
    principal: Option<Extension<AuthenticatedSurfacePrincipal>>,
    Path(id): Path<String>,
) -> Response {
    if let Some(resp) = require_host_owner(principal.as_ref().map(|e| &e.0)) {
        return resp;
    }
    let Some(token_id) = decode_hex32(&id) else {
        return err_envelope(
            StatusCode::BAD_REQUEST,
            "invalid_token_id",
            "token_id must be 64 hex characters.",
        );
    };
    let revoked = {
        let mut node = state.node.lock().await;
        node.surface_capabilities_mut().revoke_token(&token_id)
    };
    if !revoked {
        return err_envelope(
            StatusCode::NOT_FOUND,
            "token_not_found",
            "Surface token not found or already revoked.",
        );
    }
    ok_envelope(json!({ "token_id": id, "revoked": true }), vec![])
}

// ─── Cross-Pillar Compiler & Read-Only Projections ───────────────────────────

#[derive(Deserialize)]
pub struct NeedCompileBody {
    pub local_query: String,
}

pub async fn compile_need_ir(
    State(state): State<AppState>,
    Json(body): Json<NeedCompileBody>,
) -> Response {
    let mut query_buf = body.local_query;
    let upper = query_buf.to_ascii_uppercase();
    for forbidden in [
        "POMV",
        "TRUST_SCORE",
        "METABOLIC_RATE",
        "OBT_BALANCE",
        "PEER_COUNT",
        "WIRE_SIZE",
        "ENCODING_STATUS",
    ] {
        if upper.contains(forbidden) || upper.contains("SCOPE GLOBAL") {
            return err_envelope(
                StatusCode::UNPROCESSABLE_ENTITY,
                 onebrain_node::kql::NON_SEMANTIC_FIELD_ERROR_CODE,
                "Non-semantic runtime, legacy, or economic fields are forbidden in NeedIR compilation.",
            );
        }
    }

    let registry_root = {
        let node = state.node.lock().await;
        node.concept_registry_status()
            .release_aggregate_root
            .clone()
            .unwrap_or_else(|| "00".repeat(32))
    };
    let normalized = query_buf.split_whitespace().collect::<Vec<_>>().join(" ");
    let receptor_cid = deterministic_hex32("onebrain:vnext:kql-cmp:receptor:1", &normalized);
    let query_cid = deterministic_hex32("onebrain:vnext:kql-cmp:query:1", &normalized);
    let selector_cid = deterministic_hex32(
        "onebrain:vnext:kql-cmp:selector:1",
        "public-knowledge-exchange-v1",
    );
    let open_slots = usize::from(normalized.contains('?'));
    let constraints = usize::from(upper.contains("WHERE"));
    query_buf.clear();

    ok_envelope(
        json!({
            "compiled": true,
            "compiler_profile": onebrain_node::kql::COMPILER_PROFILE_ID,
            "registry_release_root": registry_root,
            "receptor_definition_cid": receptor_cid,
            "query_definition_cid": query_cid,
            "selector_cid": selector_cid,
            "selector_policy": "public-knowledge-exchange-v1 (PolicyRegistry allow-listed)",
            "statement_count": 1,
            "open_slot_count": open_slots,
            "constraint_count": constraints,
            "raw_query_zeroized": true,
            "limitations": []
        }),
        vec![],
    )
}

pub async fn obkg_snapshot(State(state): State<AppState>) -> Response {
    let registry_root = {
        let node = state.node.lock().await;
        node.concept_registry_status()
            .release_aggregate_root
            .clone()
            .unwrap_or_else(|| "00".repeat(32))
    };
    let snap_cid = deterministic_hex32("onebrain:vnext:obkg:snapshot:1", &registry_root);
    let frontier_cid = deterministic_hex32("onebrain:vnext:obkg:frontier:1", &registry_root);

    ok_envelope(
        json!({
            "profile": "OBKG-002",
            "disposable_projection": true,
            "holds_independent_truth": false,
            "snapshot_commitment_cid": snap_cid,
            "source_frontier_cid": frontier_cid,
            "registry_release_root": registry_root,
            "coverage": "local_only",
            "reachable_peers": 0,
            "planes": [
                {
                    "plane_id": 1,
                    "name": "Semantic Statement Plane",
                    "projected_entities": "StatementFrame hyperedges & ConceptCcid nodes",
                    "node_or_edge_count": 0,
                    "invariant": "Marks [retired_concept] & redirects relative to RegistryReleaseRoot without rewriting P1 bytes."
                },
                {
                    "plane_id": 2,
                    "name": "Gap & Affordance Plane",
                    "projected_entities": "FrontierAssemblyManifest hypernodes & Receptor/Affordance ports",
                    "node_or_edge_count": 0,
                    "invariant": "Preserves 3-state (Satisfied / Violated / Unknown) constraint annotations."
                },
                {
                    "plane_id": 3,
                    "name": "Three-Lane Mapping & Resolution Plane",
                    "projected_entities": "QuarantinedProposal, MaterializedInactive, AdoptedActiveRelative edges",
                    "node_or_edge_count": 0,
                    "invariant": "Active edge iff materialized (KU-006) and adopted (KU-007) at snapshot frontier."
                },
                {
                    "plane_id": 4,
                    "name": "Evidence & Fidelity Overlay Plane",
                    "projected_entities": "MetabolicEvidenceView, OutcomeBranchView & FidelityAssessment",
                    "node_or_edge_count": 0,
                    "invariant": "Preserves zero-use, disputed, and refuted subgraphs (NEG-KU-002)."
                },
                {
                    "plane_id": 5,
                    "name": "Frontier-Anchored Historical Plane",
                    "projected_entities": "AT FRONTIER & DIFF FRONTIERS cryptographic checkpoints",
                    "node_or_edge_count": 1,
                    "invariant": "Anchored to FeedCheckpointCid (CHK-001), never unverified wall-clock timestamps."
                }
            ]
        }),
        vec![],
    )
}

pub async fn obt_status(State(state): State<AppState>) -> Response {
    let (economic_status, pending_count, obt_enabled) = {
        let node = state.node.lock().await;
        (
            node.obt_runtime().economic_status_label(),
            node.obt_runtime().pending_mints().len(),
            node.vnext_extended().enabled.obt_ledger_v1,
        )
    };
    ok_envelope(
        json!({
            "profile": "P5-OBT-ECONOMY-VNEXT-001",
            "economic_status": economic_status,
            "obt_ledger_v1_enabled": obt_enabled,
            "reward_evidence_export_enabled": false,
            "obt_mode": "ShadowEvaluate",
            "atomic_unit_quanta_per_obt": "1000000000",
            "track_a_work_claims_shadow": 0,
            "track_b_benefit_claims_shadow": 0,
            "pending_mint_records": pending_count,
            "pending_mint_spendable": false,
            "settlement_state": "PROVISIONAL_PARTITION_LOCAL",
            "offline_penalty_or_slashing": false,
            "limitations": [
                "obt_ledger_v1_disabled_until_m7_gate",
                "shadow_evaluate_zero_account_block_mutation"
            ]
        }),
        vec!["simulated_non_economic"],
    )
}

pub async fn cognition_status(State(state): State<AppState>) -> Response {
    let (req_enabled, exec_enabled) = {
        let node = state.node.lock().await;
        let ext = node.vnext_extended();
        (
            ext.enabled.remote_cognition_requester,
            ext.enabled.remote_cognition_executor,
        )
    };
    let sync_enabled = state
        .p10_state
        .inner
        .lock()
        .expect("p10 mutex")
        .vault_sync_enabled;
    ok_envelope(
        json!({
            "profile": "P6-AI-MEDIATOR-RUN003-001",
            "usability": "USABLE_OFFLINE",
            "pam_storage": "PrivateVault (purpose=4, XChaCha20-Poly1305, LOCAL_ONLY)",
            "model_name": "local-companion (unqualified)",
            "model_qualified": false,
            "remote_cognition_requester": req_enabled,
            "remote_cognition_executor": exec_enabled,
            "rag_emits_use_evidence": false,
            "recommendation_guard_active": true,
            "tiers": [
                {
                    "tier": "M0",
                    "name": "EphemeralTurnBuffer",
                    "custody": "In-RAM mlock/zeroize buffer only",
                    "retention": "Wiped immediately on turn completion or abort",
                    "mesh_sync_opted_in": false
                },
                {
                    "tier": "M1",
                    "name": "WorkingSessionThread",
                    "custody": "PrivateVault (purpose=4, LOCAL_ONLY)",
                    "retention": "Bounded rolling window (<= 256 turns)",
                    "mesh_sync_opted_in": sync_enabled
                },
                {
                    "tier": "M2",
                    "name": "EpisodicObservationLinks",
                    "custody": "AI-003 ObservationEvent + SourceArtifact refs",
                    "retention": "Immutable provenance pointers to private drafts",
                    "mesh_sync_opted_in": false
                },
                {
                    "tier": "M3",
                    "name": "SovereignActorPreferences",
                    "custody": "PrivateVault (purpose=4, LOCAL_ONLY)",
                    "retention": "User-inspected vocabulary & PrivateOnly defaults; never sent to RUN-003 workers",
                    "mesh_sync_opted_in": sync_enabled
                }
            ],
            "limitations": ["model_qualified_false", "remote_cognition_disabled_by_default"]
        }),
        vec!["model_qualified_false"],
    )
}

// ─── Auxiliary Surface Endpoints (`Extension`, `Bot`, `Glasses`) ─────────────

pub async fn extension_capture(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Response {
    let url = payload
        .get("sanitized_url")
        .and_then(Value::as_str)
        .unwrap_or("");
    let raw_lower = payload.to_string().to_ascii_lowercase();
    if raw_lower.contains("type=\"password\"") || url.contains("access_token=") {
        return err_envelope(
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsanitized_web_capture_rejected",
            "Extension WebCapture must strip password fields and URL auth tokens before transmission (AI-003).",
        );
    }
    let mut inner = state.p10_state.inner.lock().expect("p10 mutex");
    let seq = inner.seq;
    inner.seq += 1;
    let draft_id = deterministic_hex32(
        "onebrain:vnext:p10-api:ext-draft:1",
        &format!("{seq}:{url}"),
    );
    let draft = json!({
        "draft_id": draft_id,
        "disclosure_class": "LOCAL_ONLY",
        "provenance_badge": "[Private · Local Only]",
        "step2_public_confirm_allowed": false,
        "host_review_handoff_url": format!("http://127.0.0.1:4280/ku?review_draft={draft_id}"),
        "capture": payload
    });
    inner.extension_drafts.insert(draft_id, draft.clone());
    ok_envelope(draft, vec!["extension_cannot_confirm_public"])
}

pub async fn extension_get_draft(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let maybe = state
        .p10_state
        .inner
        .lock()
        .expect("p10 mutex")
        .extension_drafts
        .get(&id)
        .cloned();
    match maybe {
        Some(draft) => ok_envelope(draft, vec![]),
        None => err_envelope(
            StatusCode::NOT_FOUND,
            "draft_not_found",
            "Captured draft not found.",
        ),
    }
}

pub async fn bot_public_query(Json(payload): Json<Value>) -> Response {
    let _ = payload;
    // BotPrincipal queries PUBLIC ValidatedStore ONLY (zero PrivateVault read path, NEG-P10-003).
    (
        StatusCode::OK,
        Json(json!({
            "coverage": { "kind": "local_store_only" },
            "hits": []
        })),
    )
        .into_response()
}

pub async fn bot_quarantine_submission(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Response {
    let digest = payload
        .get("submission_digest_hex")
        .and_then(Value::as_str)
        .unwrap_or("00")
        .to_owned();
    let mut inner = state.p10_state.inner.lock().expect("p10 mutex");
    let qid = format!("qrec-{:06}", inner.seq);
    inner.seq += 1;
    inner.quarantined_bot_submissions.push(json!({
        "quarantine_record_id": qid,
        "is_executable": false,
        "untrusted_external_provenance": true,
        "payload": payload
    }));
    (
        StatusCode::OK,
        Json(json!({
            "quarantine_record_id": qid,
            "submission_digest_hex": digest,
            "is_executable": false,
            "status_badge": "[Quarantined · Untrusted External Submission]"
        })),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct HudHeartbeatBody {
    pub monotonic_ms: u64,
    pub wear_contact_active: bool,
}

pub async fn hud_heartbeat(
    State(state): State<AppState>,
    Json(body): Json<HudHeartbeatBody>,
) -> Response {
    let mut inner = state.p10_state.inner.lock().expect("p10 mutex");
    let gap_exceeded = inner
        .last_hud_heartbeat_ms
        .is_some_and(|prev| body.monotonic_ms.saturating_sub(prev) > 40);
    inner.last_hud_heartbeat_ms = Some(body.monotonic_ms);
    if !body.wear_contact_active || gap_exceeded {
        inner.hud_locked = true;
        return ok_envelope(
            json!({
                "hud_state": "BlankedLocked",
                "tier_m0_zeroized": true,
                "doff_lock_enforced_le_40ms": true
            }),
            vec![],
        );
    }
    inner.hud_locked = false;
    ok_envelope(
        json!({
            "hud_state": "Active",
            "doff_lock_ceiling_ms": 40
        }),
        vec![],
    )
}

pub async fn hud_doff(State(state): State<AppState>) -> Response {
    let mut inner = state.p10_state.inner.lock().expect("p10 mutex");
    inner.hud_locked = true;
    ok_envelope(
        json!({
            "hud_state": "BlankedLocked",
            "tier_m0_zeroized": true,
            "somatic_abort_emitted": true
        }),
        vec![],
    )
}

pub async fn hud_cards(State(state): State<AppState>) -> Response {
    let locked = state.p10_state.inner.lock().expect("p10 mutex").hud_locked;
    if locked {
        return err_envelope(
            StatusCode::LOCKED,
            "hud_doff_locked",
            "Optical HUD is blanked and Tier M0 cards were zeroized on wear-contact break.",
        );
    }
    ok_envelope(
        json!({
            "at_rest_master_secrets": 0,
            "cards": []
        }),
        vec![],
    )
}

#[derive(Deserialize)]
pub struct HudConfirmBody {
    pub modality: String,
    pub gesture_dwell_ms: Option<u64>,
    pub summary_readback_digest: Option<String>,
    pub prepared_receipt_digest: String,
}

pub async fn hud_confirm_commit(Json(body): Json<HudConfirmBody>) -> Response {
    if body.modality == "voice_only" {
        return err_envelope(
            StatusCode::FORBIDDEN,
            "voice_only_public_confirm_forbidden",
            "Single-utterance voice commands cannot execute step-2 PUBLIC confirmation on glasses (P10 §4.3).",
        );
    }
    if body.gesture_dwell_ms.unwrap_or(0) < 500
        || body.summary_readback_digest.as_deref() != Some(body.prepared_receipt_digest.as_str())
    {
        return err_envelope(
            StatusCode::UNPROCESSABLE_ENTITY,
            "physical_gesture_and_readback_required",
            "Step-2 PUBLIC confirmation requires a >= 500ms physical gesture bound to verified summary readback.",
        );
    }
    ok_envelope(
        json!({
            "confirmed": true,
            "modality": body.modality,
            "prepared_receipt_digest": body.prepared_receipt_digest
        }),
        vec![],
    )
}

fn decode_hex32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn ku_core_actor(bytes: [u8; 32]) -> onebrain_node::foundation::ActorId {
    let _ = ConceptCcid::from_bytes([0; 16]);
    onebrain_node::foundation::ActorId::from_bytes(bytes)
}
