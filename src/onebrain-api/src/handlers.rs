//! Route handler implementations.
//!
//! Each handler acquires `state.node.lock().await`, calls the
//! appropriate `OneBrainNode` method, and returns an `ApiResult<T>`.

use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use futures::SinkExt;
use serde::Serialize;
use serde_json::json;

use crate::error::{ApiError, ApiResult};
use crate::server::AppState;
use crate::types::*;

#[cfg(feature = "base-v1")]
const BASE_ARCHIVE_PROJECTION_MAX_BYTES: u64 =
    onebrain_node::archive_capabilities::DEFAULT_ARCHIVE_SPOOL_BYTES;

// â”€â”€â”€ Helpers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Wrap a value in `ApiSuccess` and `Json`.
fn ok<T: Serialize>(data: T) -> Json<ApiSuccess<T>> {
    Json(ApiSuccess::new(data))
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(feature = "base-v1")]
fn base_error(error: onebrain_node::BaseServiceError) -> axum::response::Response {
    let status = match error.code {
        onebrain_base_contract::BaseErrorCodeV1::InvalidRequest => StatusCode::BAD_REQUEST,
        onebrain_base_contract::BaseErrorCodeV1::NotFound => StatusCode::NOT_FOUND,
        onebrain_base_contract::BaseErrorCodeV1::Conflict
        | onebrain_base_contract::BaseErrorCodeV1::Expired
        | onebrain_base_contract::BaseErrorCodeV1::UnknownOutcome => StatusCode::CONFLICT,
        onebrain_base_contract::BaseErrorCodeV1::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        onebrain_base_contract::BaseErrorCodeV1::CapabilityDisabled
        | onebrain_base_contract::BaseErrorCodeV1::IncompatibleProfile => StatusCode::FORBIDDEN,
        onebrain_base_contract::BaseErrorCodeV1::ResourceExhausted => {
            StatusCode::INSUFFICIENT_STORAGE
        }
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    (
        status,
        Json(
            ApiErrorResponse::new(
                format!("base_v1_error_{}", error.code.discriminator()),
                error.reason,
            )
            .with_details(json!({
                "retryable": error.retryable,
                "reconcile_before_retry": error.reconcile_before_retry,
            })),
        ),
    )
        .into_response()
}

#[cfg(feature = "base-v1")]
fn base_unavailable() -> axum::response::Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ApiErrorResponse::new(
            "base_v1_error_7",
            "base_runtime_not_installed",
        )),
    )
        .into_response()
}

#[cfg(feature = "base-v1")]
fn base_status_value(status: &onebrain_node::BaseStatusV1) -> serde_json::Value {
    use onebrain_base_contract::{SourceCommitId, SourceCommitIdentity, ToolchainIdentity};

    let tuple = &status.version.compatibility;
    let source_commit = match tuple.base_commit {
        SourceCommitIdentity::Known(SourceCommitId::Sha1(value)) => {
            json!({ "kind": "sha1", "digest": encode_hex_slice(&value.0) })
        }
        SourceCommitIdentity::Known(SourceCommitId::Sha256(value)) => {
            json!({ "kind": "sha256", "digest": hex::encode(value.0) })
        }
        SourceCommitIdentity::Unknown => json!({ "kind": "unknown" }),
    };
    let toolchain = match tuple.toolchain {
        ToolchainIdentity::Known(value) => {
            json!({ "kind": "known", "digest": hex::encode(value.0) })
        }
        ToolchainIdentity::Unknown => json!({ "kind": "unknown" }),
    };
    json!({
        "profile_major": onebrain_base_contract::BASE_RUNTIME_PROFILE_MAJOR,
        "profile_minor": onebrain_base_contract::BASE_RUNTIME_PROFILE_MINOR,
        "process_generation": hex::encode(*status.process_generation.as_bytes()),
        "dataset_generation": hex::encode(status.dataset_generation.0),
        "lifecycle": status.lifecycle as u8,
        "candidate_semantic_digest": hex::encode(status.version.candidate_semantic_digest.0),
        "artifact_tuple_digest": hex::encode(status.version.artifact_tuple_digest.0),
        "qualification": status.version.qualification.discriminator(),
        "compatibility": {
            "base_version": {
                "major": tuple.base_version.major,
                "minor": tuple.base_version.minor,
                "patch": tuple.base_version.patch,
                "prerelease": tuple.base_version.prerelease.as_ref().map(|value| value.as_str()),
            },
            "base_commit": source_commit,
            "canonical_schema_digest": hex::encode(tuple.canonical_schema_digest.0),
            "domain_registry_digest": hex::encode(tuple.domain_registry_digest.0),
            "resource_registry_digest": hex::encode(tuple.resource_registry_digest.0),
            "storage_schema": tuple.storage_schema.0,
            "archive_profile": { "major": tuple.archive_profile.major, "minor": tuple.archive_profile.minor },
            "migration_profile": { "major": tuple.migration_profile.major, "minor": tuple.migration_profile.minor },
            "registry_profile": { "major": tuple.registry_profile.major, "minor": tuple.registry_profile.minor },
            "registry_profile_digest": hex::encode(tuple.registry_profile_digest.0),
            "wire_session": { "major": tuple.wire_session.major, "minor": tuple.wire_session.minor },
            "product_api": { "major": tuple.product_api.major, "minor": tuple.product_api.minor },
            "c_abi": { "major": tuple.c_abi.major, "minor": tuple.c_abi.minor },
            "feature_set_digest": hex::encode(tuple.feature_set_digest.0),
            "target_triple": tuple.target_triple.as_str(),
            "toolchain": toolchain,
        },
        "local_usable": status.local_usable,
        "network_compiled": status.network_compiled,
        "network_requested": false,
        "network_active": status.network_enabled,
        "limitations": status.limitations,
    })
}

/// Authenticated local capability projection. Reading it never activates a
/// distributed lane or opens a legacy backend.
#[cfg(feature = "base-v1")]
pub async fn get_base_capabilities(State(state): State<AppState>) -> axum::response::Response {
    let status = match state.base_services().await {
        Some(services) => match services.snapshot() {
            Ok(status) => Some(status),
            Err(error) => return base_error(error),
        },
        None => None,
    };
    ok(json!({
        "base_v1": true,
        "runtime_installed": status.is_some(),
        "legacy_read_compat_compiled": cfg!(feature = "legacy-read-compat"),
        "legacy_read_compat_enabled": state.legacy_read_compat_enabled,
        "network_compiled": cfg!(feature = "vnext-network-runtime"),
        "network_requested": false,
        "network_active": status.as_ref().is_some_and(|value| value.network_enabled),
    }))
    .into_response()
}

#[cfg(feature = "base-v1")]
pub async fn get_base_status(State(state): State<AppState>) -> axum::response::Response {
    let Some(services) = state.base_services().await else {
        return base_unavailable();
    };
    match services.snapshot() {
        Ok(status) => ok(base_status_value(&status)).into_response(),
        Err(error) => base_error(error),
    }
}

#[cfg(feature = "base-v1")]
pub async fn invoke_base_operation(
    State(state): State<AppState>,
    Json(mut body): Json<BaseOperationProjectionRequest>,
) -> axum::response::Response {
    use onebrain_base_contract::{
        ArchiveSecretHandleV1, ArchiveSinkHandleV1, ArchiveSourceHandleV1, BaseCommandV1,
        BaseConfirmRequestV1, BaseIdempotencyKey, BaseLocalCommandV1, BaseOperationId,
        BaseOperationKindV1, BaseOperationReservationId, BasePollEventsRequestV1,
        BasePrepareRequestV1, BaseQueryRequestV1, BaseRequestV1, BaseSubscriptionId,
        BaseSubscriptionRequestV1, CreateArchiveCommandV1, ResourceBudgetV1,
        RestoreArchiveCommandV1, TopicKindV1, TypedPayloadV1,
    };

    if is_base_management_operation(&body.operation) {
        return invoke_base_management_operation(&state, body).await;
    }

    let Some(services) = state.base_services().await else {
        return base_unavailable();
    };
    let closes_runtime = body.operation == "close";
    let request = match body.operation.as_str() {
        "status" => BaseRequestV1::Status,
        "reserve" => BaseRequestV1::ReserveOperation(match body.kind {
            Some(1) => BaseOperationKindV1::ExistingLocalCommand,
            Some(2) => BaseOperationKindV1::CreateArchive,
            Some(3) => BaseOperationKindV1::RestoreArchive,
            _ => return base_bad_request("unknown_operation_kind"),
        }),
        "query" => {
            let bytes = match decode_projection_payload(
                body.payload.take(),
                body.payload_hex.take(),
                1_048_576,
            ) {
                Ok(bytes) => bytes,
                Err(()) => return base_bad_request("invalid_query_payload"),
            };
            let payload = match TypedPayloadV1::try_from_bytes(bytes) {
                Ok(payload) => payload,
                Err(_) => return base_bad_request("invalid_query_payload"),
            };
            let budget = match ResourceBudgetV1::try_new(
                body.max_items.unwrap_or(256),
                body.max_bytes.unwrap_or(1_048_576),
                body.max_work_units.unwrap_or(1_000_000),
            ) {
                Ok(budget) => budget,
                Err(_) => return base_bad_request("invalid_resource_budget"),
            };
            BaseRequestV1::Query(BaseQueryRequestV1 {
                payload,
                continuation: None,
                budget,
            })
        }
        "prepare" => {
            let Some(reservation_id) = body.reservation_id.as_deref().and_then(parse_hex_32) else {
                return base_bad_request("invalid_reservation_id");
            };
            let budget = || {
                ResourceBudgetV1::try_new(
                    body.max_items.unwrap_or(1),
                    body.max_bytes.unwrap_or(1_048_576),
                    body.max_work_units.unwrap_or(1_000_000),
                )
            };
            let command = match body.kind {
                Some(1) => {
                    let bytes = match decode_projection_payload(
                        body.payload.take(),
                        body.payload_hex.take(),
                        1_048_576,
                    ) {
                        Ok(bytes) => bytes,
                        Err(()) => return base_bad_request("invalid_command_payload"),
                    };
                    let payload = match TypedPayloadV1::try_from_bytes(bytes) {
                        Ok(payload) => payload,
                        Err(_) => return base_bad_request("invalid_command_payload"),
                    };
                    BaseCommandV1::ExistingLocalCommand(BaseLocalCommandV1 { kind: 1, payload })
                }
                Some(2) => {
                    let Some(sink) = body.auxiliary_id.as_deref().and_then(parse_hex_32) else {
                        return base_bad_request("invalid_archive_sink_handle");
                    };
                    let Some(secret) = body.operation_id.as_deref().and_then(parse_hex_32) else {
                        return base_bad_request("invalid_archive_secret_handle");
                    };
                    BaseCommandV1::CreateArchive(CreateArchiveCommandV1 {
                        sink: ArchiveSinkHandleV1::from_opaque_bytes(sink),
                        secret: ArchiveSecretHandleV1::from_opaque_bytes(secret),
                        budget: match budget() {
                            Ok(value) => value,
                            Err(_) => return base_bad_request("invalid_resource_budget"),
                        },
                    })
                }
                Some(3) => {
                    let Some(source) = body.auxiliary_id.as_deref().and_then(parse_hex_32) else {
                        return base_bad_request("invalid_archive_source_handle");
                    };
                    let Some(secret) = body.operation_id.as_deref().and_then(parse_hex_32) else {
                        return base_bad_request("invalid_archive_secret_handle");
                    };
                    BaseCommandV1::RestoreArchive(RestoreArchiveCommandV1 {
                        source: ArchiveSourceHandleV1::from_opaque_bytes(source),
                        secret: ArchiveSecretHandleV1::from_opaque_bytes(secret),
                        budget: match budget() {
                            Ok(value) => value,
                            Err(_) => return base_bad_request("invalid_resource_budget"),
                        },
                    })
                }
                _ => return base_bad_request("unknown_command_kind"),
            };
            BaseRequestV1::Prepare(BasePrepareRequestV1 {
                reservation_id: BaseOperationReservationId(reservation_id),
                command,
            })
        }
        "confirm" => {
            let Some(operation_id) = body.operation_id.as_deref().and_then(parse_hex_32) else {
                return base_bad_request("invalid_operation_id");
            };
            let Some(idempotency_key) = body.idempotency_key.as_deref().and_then(parse_hex_32)
            else {
                return base_bad_request("invalid_idempotency_key");
            };
            BaseRequestV1::Confirm(BaseConfirmRequestV1 {
                operation_id: BaseOperationId(operation_id),
                idempotency_key: BaseIdempotencyKey(idempotency_key),
            })
        }
        "cancel" | "reconcile" => {
            let Some(value) = body.operation_id.as_deref().and_then(parse_hex_32) else {
                return base_bad_request("invalid_operation_id");
            };
            if body.operation == "cancel" {
                BaseRequestV1::Cancel(BaseOperationId(value))
            } else {
                BaseRequestV1::Reconcile(BaseOperationId(value))
            }
        }
        "drain" => BaseRequestV1::Drain,
        "close" => BaseRequestV1::Close,
        "subscribe" => BaseRequestV1::Subscribe(BaseSubscriptionRequestV1 {
            topic: match body.topic {
                Some(1) => TopicKindV1::RuntimeStatus,
                Some(2) => TopicKindV1::OperationReceipts,
                Some(3) => TopicKindV1::QueryResults,
                Some(4) => TopicKindV1::ArchiveProgress,
                Some(5) => TopicKindV1::Compatibility,
                _ => return base_bad_request("unknown_subscription_topic"),
            },
            cursor: body.cursor,
        }),
        "poll_events" => {
            let Some(subscription_id) = body.operation_id.as_deref().and_then(parse_hex_32) else {
                return base_bad_request("invalid_subscription_id");
            };
            BaseRequestV1::PollEvents(BasePollEventsRequestV1 {
                subscription_id: BaseSubscriptionId::from_opaque_bytes(subscription_id),
                after_cursor: body.cursor.unwrap_or(0),
                max_items: body.max_items.unwrap_or(256),
            })
        }
        "close_subscription" => {
            let Some(subscription_id) = body.operation_id.as_deref().and_then(parse_hex_32) else {
                return base_bad_request("invalid_subscription_id");
            };
            BaseRequestV1::CloseSubscription(BaseSubscriptionId::from_opaque_bytes(subscription_id))
        }
        _ => {
            let _ = body.payload;
            let _ = body.payload_hex;
            return base_bad_request("unsupported_base_operation");
        }
    };
    if closes_runtime {
        if let Err(error) = state.close_all_base_management().await {
            return base_error(error);
        }
    }
    match services.invoke(request).await {
        Ok(onebrain_node::BaseResponseV1::Status(status)) => {
            ok(base_status_value(&status)).into_response()
        }
        Ok(onebrain_node::BaseResponseV1::Reserved(id)) => {
            ok(json!({ "operation_id": hex::encode(id.0), "state": "reserved" })).into_response()
        }
        Ok(onebrain_node::BaseResponseV1::Query {
            payload,
            continuation,
        }) => ok(json!({
            "payload_hex": encode_hex_slice(payload.as_bytes()),
            "continuation": continuation.map(|value| encode_hex_slice(value.as_bytes())),
        }))
        .into_response(),
        Ok(onebrain_node::BaseResponseV1::Prepared(intent)) => ok(json!({
            "operation_id": hex::encode(intent.operation_id.0),
            "command_blake3": hex::encode(intent.command_blake3),
        }))
        .into_response(),
        Ok(onebrain_node::BaseResponseV1::Receipt(receipt)) => ok(json!({
            "operation_id": hex::encode(receipt.operation_id.0),
            "state": receipt.state as u8,
            "attempts": receipt.attempts,
            "result_hex": encode_hex_slice(&receipt.result),
            "result_blake3": receipt.result_blake3.map(hex::encode),
            "error": receipt.error.map(|value| value.discriminator()),
            "reconcile_required": receipt.reconcile_required,
        }))
        .into_response(),
        Ok(onebrain_node::BaseResponseV1::Reconciled(result)) => ok(json!({
            "operation_id": hex::encode(result.receipt.operation_id.0),
            "state": result.receipt.state as u8,
            "attempts": result.receipt.attempts,
            "result_hex": encode_hex_slice(&result.receipt.result),
            "result_blake3": result.receipt.result_blake3.map(hex::encode),
            "error": result.receipt.error.map(|value| value.discriminator()),
            "resumed_effect": result.resumed_effect,
            "reconcile_required": result.receipt.reconcile_required,
        }))
        .into_response(),
        Ok(onebrain_node::BaseResponseV1::Drain(receipt)) => {
            ok(json!({ "lifecycle": receipt.lifecycle as u8 })).into_response()
        }
        Ok(onebrain_node::BaseResponseV1::Close(receipt)) => {
            ok(json!({ "lifecycle": receipt.lifecycle as u8 })).into_response()
        }
        Ok(onebrain_node::BaseResponseV1::Subscription(id)) => ok(json!({
            "subscription_id": encode_hex_slice(id.as_bytes()),
        }))
        .into_response(),
        Ok(onebrain_node::BaseResponseV1::Events(batch)) => ok(json!({
            "subscription_id": encode_hex_slice(batch.subscription_id.as_bytes()),
            "next_cursor": batch.next_cursor,
            "earliest_available_cursor": batch.earliest_available_cursor,
            "resync_required": batch.resync_required,
            "events": batch.events.into_iter().map(|event| json!({
                "cursor": event.cursor,
                "topic": event.topic.discriminator(),
                "operation_id": event.operation_id.map(|value| hex::encode(value.0)),
                "payload": encode_hex_slice(&event.payload),
            })).collect::<Vec<_>>(),
        }))
        .into_response(),
        Ok(onebrain_node::BaseResponseV1::SubscriptionClosed) => {
            ok(json!({ "closed": true })).into_response()
        }
        Err(error) => base_error(error),
    }
}

#[cfg(feature = "base-v1")]
fn is_base_management_operation(operation: &str) -> bool {
    matches!(
        operation,
        "management_open"
            | "management_close"
            | "archive_source_begin"
            | "archive_source_push"
            | "archive_source_seal"
            | "archive_sink_begin"
            | "archive_sink_read"
            | "archive_sink_commit"
            | "archive_secret_register"
            | "archive_capability_abort"
            | "archive_capability_destroy"
            | "complete_signer_reprovision"
    )
}

#[cfg(feature = "base-v1")]
async fn invoke_base_management_operation(
    state: &AppState,
    mut body: BaseOperationProjectionRequest,
) -> axum::response::Response {
    use onebrain_base_contract::{
        ActorRootPublicIdV1, ArchiveCapabilityHandleV1, ArchiveChunkV1, ArchiveCredentialKindV1,
        ArchiveSinkBeginV1, ArchiveSinkHandleV1, ArchiveSinkReadV1, ArchiveSourceBeginV1,
        ArchiveSourceHandleV1, ArchiveSourcePushV1, BaseManagementRequestV1,
        BaseOperationReservationId, BoundedSecretIngressV1, CompleteSignerReprovisionV1,
        FeedAuthorPublicIdV1, NodeTransportPublicIdV1, SignerDomainV1, SignerProvisionHandleV1,
        SignerPublicIdV1,
    };
    use onebrain_node::{BaseManagementResponseV1, BaseManagementScope};

    if body.operation == "management_open" {
        let mut scopes = Vec::with_capacity(body.scopes.len());
        for scope in &body.scopes {
            let parsed = match scope.as_str() {
                "archive_source" => BaseManagementScope::ArchiveSource,
                "archive_sink" => BaseManagementScope::ArchiveSink,
                "archive_secret" => BaseManagementScope::ArchiveSecret,
                "signer_reprovision" => BaseManagementScope::SignerReprovision,
                _ => return base_bad_request("unknown_management_scope"),
            };
            if scopes.contains(&parsed) {
                return base_bad_request("duplicate_management_scope");
            }
            scopes.push(parsed);
        }
        if scopes.is_empty() {
            return base_bad_request("management_scope_is_empty");
        }
        return match state.open_base_management(scopes).await {
            Ok(id) => ok(json!({
                "management_handle": encode_hex_slice(&id),
                "expires_with_host_grant_seconds": 300,
            }))
            .into_response(),
            Err(error) => base_error(error),
        };
    }

    let Some(management_id) = body.management_handle.as_deref().and_then(parse_hex_32) else {
        return base_bad_request("invalid_management_handle");
    };
    if body.operation == "management_close" {
        return match state.close_base_management(management_id).await {
            Ok(receipt) => ok(json!({
                "management_handle": encode_hex_slice(&receipt.management_handle),
                "revoked_capabilities": receipt.revoked_capabilities,
                "closed": true,
            }))
            .into_response(),
            Err(error) => base_error(error),
        };
    }

    let management = match state.base_management(management_id).await {
        Ok(services) => services,
        Err(error) => return base_error(error),
    };
    let capability = || {
        body.capability_id
            .as_deref()
            .and_then(parse_hex_32)
            .map(ArchiveCapabilityHandleV1::from_opaque_bytes)
    };
    let request = match body.operation.as_str() {
        "archive_source_begin" => {
            let Some(reservation_id) = body.reservation_id.as_deref().and_then(parse_hex_32) else {
                return base_bad_request("invalid_reservation_id");
            };
            BaseManagementRequestV1::ArchiveSourceBegin(ArchiveSourceBeginV1 {
                reservation_id: BaseOperationReservationId(reservation_id),
                declared_total_bytes: body.declared_total_bytes.unwrap_or(0),
            })
        }
        "archive_source_push" => {
            let Some(handle) = body.capability_id.as_deref().and_then(parse_hex_32) else {
                return base_bad_request("invalid_archive_source_handle");
            };
            let Some(chunk_hex) = body.chunk_hex.take() else {
                return base_bad_request("missing_archive_chunk");
            };
            let Some(chunk) = decode_bounded_hex(&chunk_hex, 1_048_576) else {
                return base_bad_request("invalid_archive_chunk");
            };
            let chunk = match ArchiveChunkV1::try_from_bytes(chunk) {
                Ok(value) => value,
                Err(_) => return base_bad_request("invalid_archive_chunk"),
            };
            BaseManagementRequestV1::ArchiveSourcePush(ArchiveSourcePushV1 {
                handle: ArchiveSourceHandleV1::from_opaque_bytes(handle),
                offset: body.offset.unwrap_or(0),
                chunk,
            })
        }
        "archive_source_seal" => {
            let Some(handle) = capability() else {
                return base_bad_request("invalid_archive_source_handle");
            };
            BaseManagementRequestV1::ArchiveSourceSeal(handle)
        }
        "archive_sink_begin" => {
            let Some(reservation_id) = body.reservation_id.as_deref().and_then(parse_hex_32) else {
                return base_bad_request("invalid_reservation_id");
            };
            BaseManagementRequestV1::ArchiveSinkBegin(ArchiveSinkBeginV1 {
                reservation_id: BaseOperationReservationId(reservation_id),
                max_total_bytes: body.declared_total_bytes.unwrap_or(0),
            })
        }
        "archive_sink_read" => {
            let Some(handle) = body.capability_id.as_deref().and_then(parse_hex_32) else {
                return base_bad_request("invalid_archive_sink_handle");
            };
            BaseManagementRequestV1::ArchiveSinkRead(ArchiveSinkReadV1 {
                handle: ArchiveSinkHandleV1::from_opaque_bytes(handle),
                offset: body.offset.unwrap_or(0),
                max_bytes: body.max_items.unwrap_or(1_048_576),
            })
        }
        "archive_sink_commit" => {
            let Some(handle) = capability() else {
                return base_bad_request("invalid_archive_sink_handle");
            };
            BaseManagementRequestV1::ArchiveSinkCommit(handle)
        }
        "archive_secret_register" => {
            let kind = match body.credential_kind {
                Some(1) => ArchiveCredentialKindV1::Password,
                Some(2) => ArchiveCredentialKindV1::RecoveryKey,
                _ => return base_bad_request("invalid_archive_credential_kind"),
            };
            let bytes = match decode_projection_payload(
                body.payload.take(),
                body.payload_hex.take(),
                4_096,
            ) {
                Ok(bytes) => bytes,
                Err(()) => return base_bad_request("invalid_archive_secret"),
            };
            let secret = match BoundedSecretIngressV1::try_new(kind, bytes) {
                Ok(value) => value,
                Err(_) => return base_bad_request("invalid_archive_secret"),
            };
            BaseManagementRequestV1::ArchiveSecretRegister(secret)
        }
        "archive_capability_abort" => {
            let Some(handle) = capability() else {
                return base_bad_request("invalid_archive_capability_handle");
            };
            BaseManagementRequestV1::ArchiveCapabilityAbort(handle)
        }
        "archive_capability_destroy" => {
            let Some(handle) = capability() else {
                return base_bad_request("invalid_archive_capability_handle");
            };
            BaseManagementRequestV1::ArchiveCapabilityDestroy(handle)
        }
        "complete_signer_reprovision" => {
            let Some(expected) = body.auxiliary_id.as_deref().and_then(parse_hex_32) else {
                return base_bad_request("invalid_signer_public_id");
            };
            let Some(provision) = body.operation_id.as_deref().and_then(parse_hex_32) else {
                return base_bad_request("invalid_signer_provision_handle");
            };
            let (domain, expected_public_id) = match body.kind {
                Some(1) => (
                    SignerDomainV1::NodeTransport,
                    SignerPublicIdV1::NodeTransport(NodeTransportPublicIdV1(expected)),
                ),
                Some(2) => (
                    SignerDomainV1::ActorRoot,
                    SignerPublicIdV1::ActorRoot(ActorRootPublicIdV1(expected)),
                ),
                Some(3) => (
                    SignerDomainV1::FeedAuthor,
                    SignerPublicIdV1::FeedAuthor(FeedAuthorPublicIdV1(expected)),
                ),
                _ => return base_bad_request("invalid_signer_domain"),
            };
            BaseManagementRequestV1::CompleteSignerReprovision(CompleteSignerReprovisionV1 {
                domain,
                expected_public_id,
                provision_handle: SignerProvisionHandleV1::from_opaque_bytes(provision),
            })
        }
        _ => return base_bad_request("unsupported_base_management_operation"),
    };
    match management.invoke(request).await {
        Ok(BaseManagementResponseV1::ArchiveSource(handle)) => ok(json!({
            "archive_source_handle": encode_hex_slice(handle.as_bytes()),
        }))
        .into_response(),
        Ok(BaseManagementResponseV1::ArchiveSink(handle)) => ok(json!({
            "archive_sink_handle": encode_hex_slice(handle.as_bytes()),
        }))
        .into_response(),
        Ok(BaseManagementResponseV1::ArchiveSecret(handle)) => ok(json!({
            "archive_secret_handle": encode_hex_slice(handle.as_bytes()),
        }))
        .into_response(),
        Ok(BaseManagementResponseV1::ArchiveCapability(handle)) => ok(json!({
            "archive_capability_handle": encode_hex_slice(handle.as_bytes()),
        }))
        .into_response(),
        Ok(BaseManagementResponseV1::ArchiveChunk { offset, bytes, eof }) => ok(json!({
            "offset": offset,
            "chunk_hex": encode_hex_slice(&bytes),
            "eof": eof,
        }))
        .into_response(),
        Ok(BaseManagementResponseV1::CapabilityClosed) => {
            ok(json!({ "capability_closed": true })).into_response()
        }
        Ok(BaseManagementResponseV1::SignerReprovisioned) => {
            ok(json!({ "signer_reprovisioned": true })).into_response()
        }
        Ok(BaseManagementResponseV1::Close(_)) => {
            base_bad_request("management_close_must_use_registry_close")
        }
        Err(error) => base_error(error),
    }
}

#[cfg(feature = "base-v1")]
fn base_bad_request(reason: &'static str) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiErrorResponse::new("base_v1_error_1", reason)),
    )
        .into_response()
}

#[cfg(feature = "legacy-read-compat")]
pub async fn get_legacy_read_compat_status(
    State(state): State<AppState>,
) -> axum::response::Response {
    if !state.legacy_read_compat_enabled {
        return StatusCode::NOT_FOUND.into_response();
    }
    ok(json!({
        "mode": "bounded_read_only",
        "writes": "capability_disabled",
        "automatic_fallback": false,
    }))
    .into_response()
}

#[cfg(feature = "base-v1")]
fn parse_hex_32(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 || !value.is_ascii() {
        return None;
    }
    let mut output = [0; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(output)
}

#[cfg(feature = "base-v1")]
fn encode_hex_slice(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(feature = "base-v1")]
fn decode_bounded_hex(value: &str, maximum: usize) -> Option<Vec<u8>> {
    if !value.is_ascii() || value.len() & 1 == 1 || value.len() / 2 > maximum {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).ok())
        .collect()
}

#[cfg(feature = "base-v1")]
fn decode_projection_payload(
    text: Option<String>,
    hexadecimal: Option<String>,
    maximum: usize,
) -> Result<Vec<u8>, ()> {
    match (text, hexadecimal) {
        (Some(_), Some(_)) => Err(()),
        (Some(value), None) if value.len() <= maximum => Ok(value.into_bytes()),
        (Some(_), None) => Err(()),
        (None, Some(value)) => decode_bounded_hex(&value, maximum).ok_or(()),
        (None, None) => Ok(Vec::new()),
    }
}

// â”€â”€â”€ Identity â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn get_identity(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let info = node.get_identity_info().map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(info).unwrap()))
}

pub async fn recover_identity(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let info = node.recover_identity_legacy().map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(info).unwrap()))
}

// â”€â”€â”€ Knowledge â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn encode_knowledge(
    State(state): State<AppState>,
    Json(body): Json<EncodeRequest>,
) -> ApiResult<serde_json::Value> {
    let _ = body.preview;
    let text = body.text.clone();
    let node_ref = state.node.clone();
    let broadcast_tx = state.event_broadcast.clone();

    // Send initial progress via broadcast (no lock needed)
    let _ = broadcast_tx.send(
        serde_json::to_string(&WsEvent {
            event_type: "encode_progress".to_string(),
            timestamp: now_epoch(),
            data: json!({ "step": 0, "total_steps": 6, "message": "Starting encode pipeline..." }),
        })
        .unwrap_or_default(),
    );

    // Run encode in a spawned task with a 300s timeout.
    let encode_future = async move {
        let mut node = node_ref.lock().await;
        node.encode_and_store_with_progress(&text, Some(&broadcast_tx))
            .await
    };

    let result = tokio::time::timeout(std::time::Duration::from_secs(300), encode_future)
        .await
        .map_err(|_| {
            ApiError(onebrain_node::NodeError::Timeout(
                "Encode timed out after 300 seconds".to_string(),
            ))
        })?
        .map_err(ApiError::from)?;

    // EncodeStoreResult is NOT Serialize, so manually build JSON
    let cid_hex = hex::encode(result.cid);
    let data = json!({
        "cid_hex": cid_hex,
        "wire_size": result.wire_size,
        "instruction_count": result.instruction_count,
        "gene_type": result.gene_type,
        "confidence": result.confidence,
        "source_text": result.source_text,
        "peers_reached": result.peers_reached,
    });
    Ok(ok(data))
}

/// Helper: encode cid bytes to hex (inline, no external dep needed at compile).
mod hex {
    pub fn encode(bytes: [u8; 32]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

pub async fn list_kus(
    State(state): State<AppState>,
    Query(params): Query<KuListParams>,
) -> ApiResult<KuListResponse> {
    let node = state.node.lock().await;
    let type_filter = params.gene_type.as_deref();
    let (kus, total) = node
        .list_kus(params.page, params.limit, type_filter, &params.sort)
        .map_err(ApiError::from)?;
    Ok(ok(KuListResponse {
        kus,
        total,
        page: params.page,
    }))
}

pub async fn get_ku(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let detail = node.get_ku(&cid).map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(detail).unwrap()))
}

pub async fn delete_ku(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let deleted = node.delete_ku(&cid).map_err(ApiError::from)?;
    Ok(ok(json!({ "deleted": deleted, "cid_hex": cid })))
}

pub async fn search_knowledge(
    State(state): State<AppState>,
    Json(body): Json<SearchRequest>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let limit = body.limit.unwrap_or(10);
    let results = node
        .search_text(&body.query, limit)
        .map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(results).unwrap()))
}

pub async fn execute_kql(
    State(state): State<AppState>,
    Json(body): Json<KqlRequest>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let results = node.execute_kql(&body.query).map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(results).unwrap()))
}

#[derive(serde::Deserialize)]
pub struct SuggestQuery {
    pub q: String,
    pub limit: Option<usize>,
}

pub async fn search_suggest(
    State(state): State<AppState>,
    Query(params): Query<SuggestQuery>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let limit = params.limit.unwrap_or(5);
    let suggestions = node
        .search_suggest(&params.q, limit)
        .map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(suggestions).unwrap()))
}

// â”€â”€â”€ Chat â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn chat(
    State(state): State<AppState>,
    Json(body): Json<ChatRequest>,
) -> ApiResult<ChatResponse> {
    let mut node = state.node.lock().await;
    let text = node
        .process_input(&body.message)
        .await
        .map_err(ApiError::from)?;
    Ok(ok(ChatResponse {
        text,
        intent: None,
        suggestions: vec![],
        kus_encoded: 0,
        kus_retrieved: 0,
    }))
}

// â”€â”€â”€ Network â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn get_status(State(state): State<AppState>) -> ApiResult<StatusResponse> {
    let node = state.node.lock().await;
    let ku_count = node.ku_count().unwrap_or(0);
    let peer_count = node.peer_count();
    let uptime_s = state.start_time.elapsed().as_secs();
    let node_name = node.node_name().to_string();
    let concept_registry = node.concept_registry_status().clone();
    let vnext = node.vnext_status();

    // Get balance for tier + obt
    let (tier, obt_balance, obt_economic_status) = match node.get_balance() {
        Ok(w) => (w.tier, w.balance, w.economic_status),
        Err(_) => (
            "Unknown".to_string(),
            0,
            onebrain_node::types::WalletEconomicStatus::SimulatedNonEconomic,
        ),
    };

    Ok(ok(StatusResponse {
        ku_count,
        peer_count,
        uptime_s,
        node_name,
        tier,
        obt_balance,
        obt_economic_status,
        version: env!("CARGO_PKG_VERSION").to_string(),
        model: node.config().model.clone(),
        concept_registry,
        vnext,
    }))
}

/// Return the additive, read-only vNext workflow contract.
///
/// This endpoint describes boundaries and the next explicit action. It does not
/// discover candidates, materialize a Mapping, adopt a Mapping, or assert a
/// network-wide result.
pub async fn get_vnext_workflow() -> ApiResult<Vec<onebrain_node::WorkflowStageView>> {
    Ok(ok(onebrain_node::workflow_surface()))
}

/// Return one stage of the additive, read-only vNext workflow contract.
pub async fn get_vnext_workflow_stage(
    Path(stage): Path<String>,
) -> ApiResult<onebrain_node::WorkflowStageView> {
    let stage = onebrain_node::WorkflowStage::parse(&stage).ok_or_else(|| {
        ApiError(onebrain_node::NodeError::InvalidArgument(format!(
            "Unknown vNext workflow stage: {stage}. Expected assembly, receptor, discover, proposal, mapping, or resolution"
        )))
    })?;
    Ok(ok(onebrain_node::workflow_stage_view(stage)))
}

pub async fn get_peers(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let peers = node.peer_list_snapshot();
    // PeerInfo is NOT Serialize, so manually convert
    let list: Vec<serde_json::Value> = peers
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "addr": p.addr.to_string(),
                "ku_count": p.ku_count,
            })
        })
        .collect();
    let count = list.len();
    Ok(ok(json!({ "peers": list, "count": count })))
}

pub async fn connect_peer(
    State(state): State<AppState>,
    Json(body): Json<ConnectRequest>,
) -> ApiResult<serde_json::Value> {
    let addr: SocketAddr = body.address.parse().map_err(|_| {
        ApiError(onebrain_node::NodeError::InvalidArgument(format!(
            "Invalid socket address: {}",
            body.address
        )))
    })?;
    let node = state.node.lock().await;
    node.connect_to_seed(addr).await.map_err(ApiError::from)?;
    Ok(ok(json!({ "connected": true, "address": body.address })))
}

// â”€â”€â”€ Graph â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn get_graph(
    State(state): State<AppState>,
    Path(cid): Path<String>,
    Query(params): Query<GraphParams>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let neighbors = node
        .get_neighbors(&cid, params.depth)
        .map_err(ApiError::from)?;
    Ok(ok(json!({
        "cid_hex": cid,
        "depth": params.depth,
        "neighbors": serde_json::to_value(&neighbors).unwrap(),
    })))
}

pub async fn get_neighbors(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let neighbors = node.get_neighbors(&cid, 1).map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(&neighbors).unwrap()))
}

// â”€â”€â”€ Wallet â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn get_wallet(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let info = node.get_balance().map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(info).unwrap()))
}

pub async fn get_wallet_history(
    State(state): State<AppState>,
    Query(params): Query<HistoryParams>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let txns = node
        .get_wallet_history(params.limit)
        .map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(txns).unwrap()))
}

// â”€â”€â”€ Profile & Settings â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn get_profile(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let profile = node.get_profile().map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(profile).unwrap()))
}

pub async fn update_profile(
    State(state): State<AppState>,
    Json(body): Json<ProfileUpdateRequest>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    if let Some(name) = &body.display_name {
        node.update_profile("name", name).map_err(ApiError::from)?;
    }
    if let Some(lang) = &body.language {
        node.update_profile("language", lang)
            .map_err(ApiError::from)?;
    }
    if let Some(style) = &body.response_style {
        node.update_profile("style", style)
            .map_err(ApiError::from)?;
    }
    let profile = node.get_profile().map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(profile).unwrap()))
}

pub async fn get_settings(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let config = node.get_config_view();
    Ok(ok(serde_json::to_value(config).unwrap()))
}

pub async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<SettingsUpdateRequest>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    if let Some(name) = &body.name {
        node.update_config("name", name).map_err(ApiError::from)?;
    }
    if let Some(url) = &body.ollama_url {
        node.update_config("ollama_url", url)
            .map_err(ApiError::from)?;
    }
    if let Some(model) = &body.model {
        node.update_config("model", model).map_err(ApiError::from)?;
    }
    let config = node.get_config_view();
    Ok(ok(serde_json::to_value(config).unwrap()))
}

// â”€â”€â”€ AI â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn ai_status(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let info = node.test_ai_connection().await.map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(info).unwrap()))
}

pub async fn list_ai_models(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let models = node.list_ai_models().map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(models).unwrap()))
}

pub async fn switch_model(
    State(state): State<AppState>,
    Json(body): Json<SwitchModelRequest>,
) -> ApiResult<serde_json::Value> {
    let (old_model, ollama_url) = {
        let mut node = state.node.lock().await;
        let old = node.config().model.clone();
        let url = node.config().ollama_url.clone();
        node.switch_model(&body.model_name)
            .map_err(ApiError::from)?;
        (old, url)
    };
    // Node lock released here

    // Unload old model from Ollama to free RAM (server-side, reliable)
    let mut unloaded_models = Vec::new();
    if old_model != body.model_name {
        // First, get all currently loaded models from Ollama
        let client = reqwest::Client::new();
        if let Ok(resp) = client.get(format!("{}/api/ps", ollama_url)).send().await {
            if let Ok(ps_json) = resp.json::<serde_json::Value>().await {
                if let Some(models) = ps_json["models"].as_array() {
                    for m in models {
                        if let Some(name) = m["name"].as_str() {
                            // Unload each loaded model
                            let unload_body = json!({
                                "model": name,
                                "keep_alive": 0
                            });
                            if let Ok(res) = client
                                .post(format!("{}/api/generate", ollama_url))
                                .json(&unload_body)
                                .send()
                                .await
                            {
                                // Consume the streaming response body to ensure Ollama processes it
                                let _ = res.text().await;
                                unloaded_models.push(name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(ok(json!({
        "model": body.model_name,
        "switched": true,
        "old_model": old_model,
        "unloaded": unloaded_models,
    })))
}

// â”€â”€â”€ Blobs â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn list_blobs(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let blobs = node.list_blobs().map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(blobs).unwrap()))
}

pub async fn get_blob_meta(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let meta = node.get_blob_meta(&cid).map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(meta).unwrap()))
}

pub async fn delete_blob(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let deleted = node.delete_blob_file(&cid).map_err(ApiError::from)?;
    Ok(ok(json!({ "deleted": deleted, "blob_cid_hex": cid })))
}

pub async fn blob_stats(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let (count, total_size) = node.blob_stats().map_err(ApiError::from)?;
    Ok(ok(json!({
        "count": count,
        "total_size": total_size,
    })))
}

pub async fn blob_gc(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let (removed, freed) = node.blob_gc().map_err(ApiError::from)?;
    Ok(ok(json!({
        "removed": removed,
        "freed_bytes": freed,
    })))
}

// â”€â”€â”€ WebSocket â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Query params for WS auth.
#[derive(serde::Deserialize)]
pub struct WsAuthParams {
    pub token: Option<String>,
}

pub async fn ws_events(
    State(state): State<AppState>,
    Query(params): Query<WsAuthParams>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // Validate token from query param
    let token_valid = params
        .token
        .as_ref()
        .map(|t| t == &state.api_token)
        .unwrap_or(false);

    if !token_valid {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            Json(ApiErrorResponse::new(
                "AUTH_REQUIRED",
                "Invalid or missing WebSocket token",
            )),
        )
            .into_response();
    }

    ws.on_upgrade(move |socket| ws_handler(socket, state))
        .into_response()
}

async fn ws_handler(socket: WebSocket, state: AppState) {
    use futures::stream::StreamExt;

    let (mut sender, mut receiver) = socket.split();

    // Subscribe to broadcast channel for real-time events (encode progress, etc.)
    let mut broadcast_rx = state.event_broadcast.subscribe();

    // Spawn a task that receives broadcast events and forwards to WS client
    let send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                // Priority: broadcast events (no lock needed)
                result = broadcast_rx.recv() => {
                    match result {
                        Ok(json) => {
                            if sender.send(Message::Text(json.into())).await.is_err() {
                                return; // Client disconnected
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => return,
                    }
                }
            }
        }
    });

    // Read incoming messages (keep alive / close detection)
    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Close(_) => break,
            _ => {} // Ignore other messages
        }
    }

    send_task.abort();
}

/// Convert a `NodeEvent` (not Serialize) into a `WsEvent`.
pub fn node_event_to_ws_pub(event: &onebrain_node::NodeEvent) -> WsEvent {
    let ts = now_epoch();
    match event {
        onebrain_node::NodeEvent::PeerConnected(peer) => WsEvent {
            event_type: "peer_connected".to_string(),
            timestamp: ts,
            data: json!({
                "name": peer.name,
                "addr": peer.addr.to_string(),
                "ku_count": peer.ku_count,
            }),
        },
        onebrain_node::NodeEvent::KuReceived {
            cid_hex,
            source_text,
            from,
            ..
        } => WsEvent {
            event_type: "ku_received".to_string(),
            timestamp: ts,
            data: json!({
                "cid_hex": cid_hex,
                "source_text": source_text,
                "from": from,
            }),
        },
        onebrain_node::NodeEvent::VerifyResult {
            cid_hex,
            agreement_score,
            verified,
            from,
        } => WsEvent {
            event_type: "verify_result".to_string(),
            timestamp: ts,
            data: json!({
                "cid_hex": cid_hex,
                "agreement_score": agreement_score,
                "verified": verified,
                "from": from,
            }),
        },
        onebrain_node::NodeEvent::Notification(msg) => WsEvent {
            event_type: "notification".to_string(),
            timestamp: ts,
            data: json!({ "message": msg }),
        },
        onebrain_node::NodeEvent::EncodeProgress {
            step,
            total_steps,
            message,
        } => WsEvent {
            event_type: "encode_progress".to_string(),
            timestamp: ts,
            data: json!({
                "step": step,
                "total_steps": total_steps,
                "message": message,
            }),
        },
    }
}

// â”€â”€â”€ Phase 1: Knowledge Management â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn deprecate_ku(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let deprecated = node.deprecate_ku(&cid).map_err(ApiError::from)?;
    Ok(ok(json!({ "deprecated": deprecated, "cid_hex": cid })))
}

pub async fn save_draft(
    State(state): State<AppState>,
    Json(body): Json<DraftRequest>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let draft = node
        .save_draft(&body.text, body.title.as_deref())
        .map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(draft).unwrap()))
}

pub async fn list_drafts(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let drafts = node.list_drafts();
    Ok(ok(json!({ "drafts": drafts, "total": drafts.len() })))
}

pub async fn get_draft(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let draft = node.get_draft(&draft_id).map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(draft).unwrap()))
}

pub async fn update_draft(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
    Json(body): Json<DraftRequest>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let draft = node
        .update_draft(&draft_id, &body.text, body.title.as_deref())
        .map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(draft).unwrap()))
}

pub async fn delete_draft(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let deleted = node.delete_draft(&draft_id).map_err(ApiError::from)?;
    Ok(ok(json!({ "deleted": deleted, "draft_id": draft_id })))
}

pub async fn publish_draft(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let result = node
        .publish_draft(&draft_id)
        .await
        .map_err(ApiError::from)?;
    let cid_hex = hex::encode(result.cid);
    Ok(ok(json!({
        "cid_hex": cid_hex,
        "wire_size": result.wire_size,
        "instruction_count": result.instruction_count,
        "gene_type": result.gene_type,
        "confidence": result.confidence,
        "peers_reached": result.peers_reached,
    })))
}

// â”€â”€â”€ Phase 1: Tags â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn add_tag(
    State(state): State<AppState>,
    Path(cid): Path<String>,
    Json(body): Json<TagRequest>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    node.add_tag(&cid, &body.tag).map_err(ApiError::from)?;
    Ok(ok(
        json!({ "added": true, "cid_hex": cid, "tag": body.tag }),
    ))
}

pub async fn remove_tag(
    State(state): State<AppState>,
    Path((cid, tag)): Path<(String, String)>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    node.remove_tag(&cid, &tag).map_err(ApiError::from)?;
    Ok(ok(json!({ "removed": true, "cid_hex": cid, "tag": tag })))
}

pub async fn list_tags(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let tags = node.list_all_tags();
    Ok(ok(json!({ "tags": tags, "count": tags.len() })))
}

/// Get tags for a specific KU.
pub async fn get_ku_tags(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let tags = node.get_ku_tags(&cid);
    Ok(ok(json!({ "tags": tags, "cid_hex": cid })))
}

/// Stake OBT tokens.
pub async fn stake(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let amount = body["amount"].as_u64().unwrap_or(0);
    let mut node = state.node.lock().await;
    let info = node.stake(amount).map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(info).unwrap()))
}

/// Unstake OBT tokens.
pub async fn unstake(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let amount = body["amount"].as_u64().unwrap_or(0);
    let mut node = state.node.lock().await;
    let info = node.unstake(amount).map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(info).unwrap()))
}

// ─── Profile & Settings ────────────────────────────────────────────

pub async fn pin_ku(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let pinned = node.pin_ku(&cid).map_err(ApiError::from)?;
    Ok(ok(json!({ "pinned": pinned, "cid_hex": cid })))
}

pub async fn unpin_ku(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let unpinned = node.unpin_ku(&cid).map_err(ApiError::from)?;
    Ok(ok(json!({ "unpinned": unpinned, "cid_hex": cid })))
}

pub async fn list_pinned_kus(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let kus = node.pinned_kus();
    Ok(ok(serde_json::to_value(&kus).unwrap()))
}

// â”€â”€â”€ Phase 1: Social & Discovery â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn follow_node(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    node.follow_node(&node_id).map_err(ApiError::from)?;
    Ok(ok(json!({ "followed": true, "node_id": node_id })))
}

pub async fn unfollow_node(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    node.unfollow_node(&node_id).map_err(ApiError::from)?;
    Ok(ok(json!({ "unfollowed": true, "node_id": node_id })))
}

pub async fn list_following(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let following = node.following_list();
    Ok(ok(serde_json::to_value(&following).unwrap()))
}

pub async fn get_peer_profile(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    match node.get_peer_profile(&node_id) {
        Some(profile) => Ok(ok(serde_json::to_value(&profile).unwrap())),
        None => Err(ApiError(onebrain_node::NodeError::KuNotFound(format!(
            "Node not found: {}",
            node_id
        )))),
    }
}

// â”€â”€â”€ Phase 1: Multi-Device â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn list_devices(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let devices = node.list_devices();
    Ok(ok(serde_json::to_value(&devices).unwrap()))
}

pub async fn sync_status(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let status = node.sync_status();
    Ok(ok(serde_json::to_value(&status).unwrap()))
}

// â”€â”€â”€ Phase 1: Bulk Operations â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn bulk_delete_kus(
    State(state): State<AppState>,
    Json(body): Json<BulkDeleteRequest>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let result = node
        .bulk_delete(body.gene_type.as_deref(), body.before_timestamp)
        .map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(&result).unwrap()))
}

// â”€â”€â”€ Phase 1: Watch (Standing Queries) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn create_watch(
    State(state): State<AppState>,
    Json(body): Json<WatchRequest>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let watch_id = node.create_watch(&body.query).map_err(ApiError::from)?;
    Ok(ok(json!({ "watch_id": watch_id, "query": body.query })))
}

pub async fn list_watches(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let watches = node.list_watches();
    Ok(ok(serde_json::to_value(&watches).unwrap()))
}

pub async fn delete_watch(
    State(state): State<AppState>,
    Path(watch_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let deleted = node.delete_watch(&watch_id).map_err(ApiError::from)?;
    Ok(ok(json!({ "deleted": deleted, "watch_id": watch_id })))
}

// â”€â”€â”€ Phase 1: Blob Extensions â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn add_blob_ku_ref(
    State(state): State<AppState>,
    Path(cid): Path<String>,
    Json(body): Json<BlobRefRequest>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    node.blob_add_ku_ref(&cid, &body.ku_cid)
        .map_err(ApiError::from)?;
    Ok(ok(
        json!({ "linked": true, "blob_cid": cid, "ku_cid": body.ku_cid }),
    ))
}

pub async fn pin_blob(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let pinned = node.pin_blob(&cid).map_err(ApiError::from)?;
    Ok(ok(json!({ "pinned": pinned, "blob_cid": cid })))
}

pub async fn unpin_blob(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let unpinned = node.unpin_blob(&cid).map_err(ApiError::from)?;
    Ok(ok(json!({ "unpinned": unpinned, "blob_cid": cid })))
}

// â”€â”€â”€ Phase 1: Data Portability â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn export_kus(
    State(state): State<AppState>,
    Query(params): Query<ExportParams>,
) -> Result<axum::response::Response, ApiError> {
    let node = state.node.lock().await;
    let temp_dir = tempfile::tempdir().map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;
    let ext = match params.mode {
        DataPortabilityMode::CanonicalV1 => "obx",
        DataPortabilityMode::JsonViewV1 => "json",
        DataPortabilityMode::CsvViewV1 => "csv",
        DataPortabilityMode::TextDraftsV1 => {
            return Err(ApiError(onebrain_node::NodeError::InvalidArgument(
                "text-drafts-v1 is import-only".into(),
            )))
        }
    };
    let file_path = temp_dir.path().join(format!("export.{}", ext));
    let count = node
        .export_data(params.mode.as_str(), &file_path)
        .map_err(ApiError::from)?;
    drop(node);

    let data = tokio::fs::read(&file_path)
        .await
        .map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;

    let content_type = match params.mode {
        DataPortabilityMode::CanonicalV1 => "application/vnd.onebrain.obx-v1",
        DataPortabilityMode::JsonViewV1 => "application/json",
        DataPortabilityMode::CsvViewV1 => "text/csv",
        DataPortabilityMode::TextDraftsV1 => unreachable!(),
    };
    let filename = format!("onebrain_export_{}.{}", count, ext);

    Ok(axum::response::Response::builder()
        .status(200)
        .header("Content-Type", content_type)
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", filename),
        )
        .header("X-Export-Count", count.to_string())
        .body(axum::body::Body::from(data))
        .unwrap())
}

pub async fn import_kus(
    State(state): State<AppState>,
    Query(params): Query<ImportParams>,
    mut multipart: axum::extract::Multipart,
) -> ApiResult<serde_json::Value> {
    let temp_dir = tempfile::tempdir().map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;
    let mut file_path = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        ApiError(onebrain_node::NodeError::InvalidArgument(format!(
            "Multipart error: {}",
            e
        )))
    })? {
        if field.name() == Some("file") {
            let filename = field.file_name().unwrap_or("import.txt").to_string();
            let path = temp_dir.path().join(&filename);
            let data = field.bytes().await.map_err(|e| {
                ApiError(onebrain_node::NodeError::InvalidArgument(format!(
                    "Read error: {}",
                    e
                )))
            })?;
            tokio::fs::write(&path, &data)
                .await
                .map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;
            file_path = Some(path);
        }
    }

    let path = file_path.ok_or_else(|| {
        ApiError(onebrain_node::NodeError::InvalidArgument(
            "No file field in multipart".into(),
        ))
    })?;

    let mut node = state.node.lock().await;
    let result = match params.mode {
        DataPortabilityMode::CanonicalV1 => node
            .import_canonical_exchange(&path)
            .map_err(ApiError::from)?,
        DataPortabilityMode::TextDraftsV1 => node
            .import_text_drafts(&path)
            .await
            .map_err(ApiError::from)?,
        DataPortabilityMode::JsonViewV1 | DataPortabilityMode::CsvViewV1 => {
            return Err(ApiError(onebrain_node::NodeError::InvalidArgument(
                "JSON/CSV views are not importable".into(),
            )))
        }
    };
    Ok(ok(serde_json::to_value(&result).unwrap()))
}

#[cfg(feature = "base-v1")]
async fn create_base_archive_bytes(
    state: &AppState,
    password: Vec<u8>,
) -> Result<Vec<u8>, onebrain_node::BaseServiceError> {
    use onebrain_base_contract::{
        ArchiveCapabilityHandleV1, ArchiveCredentialKindV1, ArchiveSinkBeginV1,
        ArchiveSinkHandleV1, ArchiveSinkReadV1, BaseCommandV1, BaseConfirmRequestV1,
        BaseErrorCodeV1, BaseIdempotencyKey, BaseManagementRequestV1, BaseOperationKindV1,
        BasePrepareRequestV1, BaseRequestV1, BoundedSecretIngressV1, CreateArchiveCommandV1,
        ResourceBudgetV1,
    };
    use onebrain_node::{BaseManagementResponseV1, BaseManagementScope, BaseResponseV1};

    let services = state.base_services().await.ok_or_else(|| {
        onebrain_node::BaseServiceError::new(
            BaseErrorCodeV1::DependencyUnavailable,
            "base_runtime_not_installed",
        )
    })?;
    let management_id = state
        .open_base_management(vec![
            BaseManagementScope::ArchiveSink,
            BaseManagementScope::ArchiveSecret,
        ])
        .await?;
    let management = state.base_management(management_id).await?;
    let operation = async {
        let reservation = match services
            .invoke(BaseRequestV1::ReserveOperation(
                BaseOperationKindV1::CreateArchive,
            ))
            .await?
        {
            BaseResponseV1::Reserved(value) => value,
            _ => {
                return Err(base_projection_internal(
                    "unexpected_archive_reserve_response",
                ))
            }
        };
        let sink = match management
            .invoke(BaseManagementRequestV1::ArchiveSinkBegin(
                ArchiveSinkBeginV1 {
                    reservation_id: reservation,
                    max_total_bytes: BASE_ARCHIVE_PROJECTION_MAX_BYTES,
                },
            ))
            .await?
        {
            BaseManagementResponseV1::ArchiveSink(value) => value,
            _ => return Err(base_projection_internal("unexpected_archive_sink_response")),
        };
        let sink_id = *sink.as_bytes();
        let secret = match management
            .invoke(BaseManagementRequestV1::ArchiveSecretRegister(
                BoundedSecretIngressV1::try_new(ArchiveCredentialKindV1::Password, password)
                    .map_err(|_| {
                        onebrain_node::BaseServiceError::new(
                            BaseErrorCodeV1::InvalidRequest,
                            "invalid_archive_secret",
                        )
                    })?,
            ))
            .await?
        {
            BaseManagementResponseV1::ArchiveSecret(value) => value,
            _ => {
                return Err(base_projection_internal(
                    "unexpected_archive_secret_response",
                ))
            }
        };
        let prepared = match services
            .invoke(BaseRequestV1::Prepare(BasePrepareRequestV1 {
                reservation_id: reservation,
                command: BaseCommandV1::CreateArchive(CreateArchiveCommandV1 {
                    sink: ArchiveSinkHandleV1::from_opaque_bytes(sink_id),
                    secret,
                    budget: ResourceBudgetV1::try_new(1, 1_048_576, 1_000_000).map_err(|_| {
                        base_projection_internal("compiled_archive_budget_is_invalid")
                    })?,
                }),
            }))
            .await?
        {
            BaseResponseV1::Prepared(value) => value,
            _ => {
                return Err(base_projection_internal(
                    "unexpected_archive_prepare_response",
                ))
            }
        };
        let receipt = match services
            .invoke(BaseRequestV1::Confirm(BaseConfirmRequestV1 {
                operation_id: prepared.operation_id,
                idempotency_key: BaseIdempotencyKey(random_projection_id()?),
            }))
            .await?
        {
            BaseResponseV1::Receipt(value) => value,
            _ => {
                return Err(base_projection_internal(
                    "unexpected_archive_confirm_response",
                ))
            }
        };
        if let Some(error) = receipt.error {
            return Err(onebrain_node::BaseServiceError::new(
                error,
                "base_archive_create_failed",
            ));
        }
        if receipt.result.len() != 64 || receipt.result[..32] != sink_id {
            return Err(onebrain_node::BaseServiceError::new(
                BaseErrorCodeV1::CorruptState,
                "archive_create_receipt_binding_mismatch",
            ));
        }

        let mut archive = Vec::new();
        let mut offset = 0u64;
        loop {
            let (chunk_offset, bytes, eof) = match management
                .invoke(BaseManagementRequestV1::ArchiveSinkRead(
                    ArchiveSinkReadV1 {
                        handle: ArchiveSinkHandleV1::from_opaque_bytes(sink_id),
                        offset,
                        max_bytes: 1_048_576,
                    },
                ))
                .await?
            {
                BaseManagementResponseV1::ArchiveChunk { offset, bytes, eof } => {
                    (offset, bytes, eof)
                }
                _ => {
                    return Err(base_projection_internal(
                        "unexpected_archive_chunk_response",
                    ))
                }
            };
            if chunk_offset != offset || (bytes.is_empty() && !eof) {
                return Err(onebrain_node::BaseServiceError::new(
                    BaseErrorCodeV1::CorruptState,
                    "archive_sink_non_contiguous",
                ));
            }
            let next = archive.len().checked_add(bytes.len()).ok_or_else(|| {
                onebrain_node::BaseServiceError::new(
                    BaseErrorCodeV1::ResourceExhausted,
                    "archive_response_size_overflow",
                )
            })?;
            if next as u64 > BASE_ARCHIVE_PROJECTION_MAX_BYTES {
                return Err(onebrain_node::BaseServiceError::new(
                    BaseErrorCodeV1::ResourceExhausted,
                    "archive_response_exceeds_bound",
                ));
            }
            archive.extend_from_slice(&bytes);
            offset = next as u64;
            if eof {
                break;
            }
        }
        match management
            .invoke(BaseManagementRequestV1::ArchiveSinkCommit(
                ArchiveCapabilityHandleV1::from_opaque_bytes(sink_id),
            ))
            .await?
        {
            BaseManagementResponseV1::CapabilityClosed => Ok(archive),
            _ => Err(base_projection_internal(
                "unexpected_archive_commit_response",
            )),
        }
    }
    .await;
    let close = state.close_base_management(management_id).await;
    match (operation, close) {
        (Ok(archive), Ok(_)) => Ok(archive),
        (Err(error), _) | (Ok(_), Err(error)) => Err(error),
    }
}

#[cfg(feature = "base-v1")]
async fn restore_base_archive_bytes(
    state: &AppState,
    archive: Vec<u8>,
    password: Vec<u8>,
) -> Result<[u8; 32], onebrain_node::BaseServiceError> {
    use onebrain_base_contract::{
        ArchiveCapabilityHandleV1, ArchiveChunkV1, ArchiveCredentialKindV1, ArchiveSourceBeginV1,
        ArchiveSourceHandleV1, ArchiveSourcePushV1, BaseCommandV1, BaseConfirmRequestV1,
        BaseErrorCodeV1, BaseIdempotencyKey, BaseManagementRequestV1, BaseOperationKindV1,
        BasePrepareRequestV1, BaseRequestV1, BoundedSecretIngressV1, ResourceBudgetV1,
        RestoreArchiveCommandV1,
    };
    use onebrain_node::{BaseManagementResponseV1, BaseManagementScope, BaseResponseV1};

    let services = state.base_services().await.ok_or_else(|| {
        onebrain_node::BaseServiceError::new(
            BaseErrorCodeV1::DependencyUnavailable,
            "base_runtime_not_installed",
        )
    })?;
    let management_id = state
        .open_base_management(vec![
            BaseManagementScope::ArchiveSource,
            BaseManagementScope::ArchiveSecret,
        ])
        .await?;
    let management = state.base_management(management_id).await?;
    let operation = async {
        let reservation = match services
            .invoke(BaseRequestV1::ReserveOperation(
                BaseOperationKindV1::RestoreArchive,
            ))
            .await?
        {
            BaseResponseV1::Reserved(value) => value,
            _ => {
                return Err(base_projection_internal(
                    "unexpected_restore_reserve_response",
                ))
            }
        };
        let source = match management
            .invoke(BaseManagementRequestV1::ArchiveSourceBegin(
                ArchiveSourceBeginV1 {
                    reservation_id: reservation,
                    declared_total_bytes: archive.len() as u64,
                },
            ))
            .await?
        {
            BaseManagementResponseV1::ArchiveSource(value) => value,
            _ => {
                return Err(base_projection_internal(
                    "unexpected_archive_source_response",
                ))
            }
        };
        let source_id = *source.as_bytes();
        let mut offset = 0u64;
        for chunk in archive.chunks(1_048_576) {
            match management
                .invoke(BaseManagementRequestV1::ArchiveSourcePush(
                    ArchiveSourcePushV1 {
                        handle: ArchiveSourceHandleV1::from_opaque_bytes(source_id),
                        offset,
                        chunk: ArchiveChunkV1::try_from_bytes(chunk.to_vec()).map_err(|_| {
                            base_projection_internal("bounded_archive_chunk_is_invalid")
                        })?,
                    },
                ))
                .await?
            {
                BaseManagementResponseV1::ArchiveCapability(_) => {}
                _ => {
                    return Err(base_projection_internal(
                        "unexpected_archive_source_push_response",
                    ))
                }
            }
            offset = offset.checked_add(chunk.len() as u64).ok_or_else(|| {
                onebrain_node::BaseServiceError::new(
                    BaseErrorCodeV1::ResourceExhausted,
                    "archive_upload_offset_overflow",
                )
            })?;
        }
        match management
            .invoke(BaseManagementRequestV1::ArchiveSourceSeal(
                ArchiveCapabilityHandleV1::from_opaque_bytes(source_id),
            ))
            .await?
        {
            BaseManagementResponseV1::ArchiveSource(handle) if handle.as_bytes() == &source_id => {}
            _ => return Err(base_projection_internal("unexpected_archive_seal_response")),
        }
        let secret = match management
            .invoke(BaseManagementRequestV1::ArchiveSecretRegister(
                BoundedSecretIngressV1::try_new(ArchiveCredentialKindV1::Password, password)
                    .map_err(|_| {
                        onebrain_node::BaseServiceError::new(
                            BaseErrorCodeV1::InvalidRequest,
                            "invalid_archive_secret",
                        )
                    })?,
            ))
            .await?
        {
            BaseManagementResponseV1::ArchiveSecret(value) => value,
            _ => {
                return Err(base_projection_internal(
                    "unexpected_archive_secret_response",
                ))
            }
        };
        let prepared = match services
            .invoke(BaseRequestV1::Prepare(BasePrepareRequestV1 {
                reservation_id: reservation,
                command: BaseCommandV1::RestoreArchive(RestoreArchiveCommandV1 {
                    source: ArchiveSourceHandleV1::from_opaque_bytes(source_id),
                    secret,
                    budget: ResourceBudgetV1::try_new(1, 1_048_576, 1_000_000).map_err(|_| {
                        base_projection_internal("compiled_archive_budget_is_invalid")
                    })?,
                }),
            }))
            .await?
        {
            BaseResponseV1::Prepared(value) => value,
            _ => {
                return Err(base_projection_internal(
                    "unexpected_restore_prepare_response",
                ))
            }
        };
        let receipt = match services
            .invoke(BaseRequestV1::Confirm(BaseConfirmRequestV1 {
                operation_id: prepared.operation_id,
                idempotency_key: BaseIdempotencyKey(random_projection_id()?),
            }))
            .await?
        {
            BaseResponseV1::Receipt(value) => value,
            _ => {
                return Err(base_projection_internal(
                    "unexpected_restore_confirm_response",
                ))
            }
        };
        if let Some(error) = receipt.error {
            return Err(onebrain_node::BaseServiceError::new(
                error,
                "base_archive_restore_failed",
            ));
        }
        Ok(prepared.operation_id.0)
    }
    .await;
    let _ = state.close_base_management(management_id).await;
    let operation_id = operation?;
    let refreshed = state.base_services().await.ok_or_else(|| {
        onebrain_node::BaseServiceError::new(
            BaseErrorCodeV1::DependencyUnavailable,
            "base_runtime_missing_after_restore",
        )
    })?;
    match refreshed
        .invoke(BaseRequestV1::Reconcile(
            onebrain_base_contract::BaseOperationId(operation_id),
        ))
        .await?
    {
        BaseResponseV1::Reconciled(result)
            if result.receipt.error.is_none() && !result.receipt.reconcile_required =>
        {
            Ok(operation_id)
        }
        BaseResponseV1::Reconciled(_) => Err(onebrain_node::BaseServiceError::new(
            BaseErrorCodeV1::UnknownOutcome,
            "base_restore_reconcile_required",
        )),
        _ => Err(base_projection_internal(
            "unexpected_restore_reconcile_response",
        )),
    }
}

#[cfg(feature = "base-v1")]
fn base_projection_internal(reason: &'static str) -> onebrain_node::BaseServiceError {
    onebrain_node::BaseServiceError::new(
        onebrain_base_contract::BaseErrorCodeV1::InternalError,
        reason,
    )
}

#[cfg(feature = "base-v1")]
fn random_projection_id() -> Result<[u8; 32], onebrain_node::BaseServiceError> {
    use rand::RngCore;

    let mut id = [0; 32];
    rand::rngs::OsRng.fill_bytes(&mut id);
    if id == [0; 32] {
        return Err(base_projection_internal("projection_entropy_returned_zero"));
    }
    Ok(id)
}

#[cfg(feature = "base-v1")]
pub async fn create_backup(
    State(state): State<AppState>,
    Json(body): Json<BackupRequest>,
) -> axum::response::Response {
    match create_base_archive_bytes(&state, body.password.into_bytes()).await {
        Ok(data) => axum::response::Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/vnd.onebrain.obar-v2")
            .header(
                "Content-Disposition",
                format!(
                    "attachment; filename=\"onebrain_base_{}.obar\"",
                    now_epoch()
                ),
            )
            .header("X-Backup-Size", data.len().to_string())
            .body(axum::body::Body::from(data))
            .unwrap(),
        Err(error) => base_error(error),
    }
}

#[cfg(not(feature = "base-v1"))]
pub async fn create_backup(
    State(_state): State<AppState>,
    Json(_body): Json<BackupRequest>,
) -> axum::response::Response {
    StatusCode::NOT_FOUND.into_response()
}

#[cfg(feature = "base-v1")]
pub async fn restore_backup(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> axum::response::Response {
    let mut archive = None;
    let mut password = String::new();

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(_) => return base_bad_request("invalid_restore_multipart"),
        };
        match field.name() {
            Some("file") => {
                let data = match field.bytes().await {
                    Ok(value)
                        if !value.is_empty()
                            && value.len() as u64 <= BASE_ARCHIVE_PROJECTION_MAX_BYTES =>
                    {
                        value.to_vec()
                    }
                    _ => return base_bad_request("invalid_restore_archive"),
                };
                archive = Some(data);
            }
            Some("password") => {
                password = match field.text().await {
                    Ok(value) => value,
                    Err(_) => return base_bad_request("invalid_restore_password"),
                };
            }
            _ => {}
        }
    }

    let Some(archive) = archive else {
        return base_bad_request("missing_restore_archive");
    };
    match restore_base_archive_bytes(&state, archive, password.into_bytes()).await {
        Ok(operation_id) => ok(json!({
            "restored": true,
            "operation_id": hex::encode(operation_id),
            "reconciled": true,
        }))
        .into_response(),
        Err(error) => base_error(error),
    }
}

#[cfg(not(feature = "base-v1"))]
pub async fn restore_backup(
    State(_state): State<AppState>,
    _multipart: axum::extract::Multipart,
) -> axum::response::Response {
    StatusCode::NOT_FOUND.into_response()
}

// â”€â”€â”€ Phase 1: Blob Upload & Download â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub async fn upload_blob(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> ApiResult<serde_json::Value> {
    let temp_dir = tempfile::tempdir().map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;
    let mut file_path = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        ApiError(onebrain_node::NodeError::InvalidArgument(format!(
            "Multipart error: {}",
            e
        )))
    })? {
        if field.name() == Some("file") {
            let filename = field.file_name().unwrap_or("upload").to_string();
            let path = temp_dir.path().join(&filename);
            let data = field.bytes().await.map_err(|e| {
                ApiError(onebrain_node::NodeError::InvalidArgument(format!(
                    "Read error: {}",
                    e
                )))
            })?;
            tokio::fs::write(&path, &data)
                .await
                .map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;
            file_path = Some(path);
        }
    }

    let path = file_path.ok_or_else(|| {
        ApiError(onebrain_node::NodeError::InvalidArgument(
            "No file field in multipart".into(),
        ))
    })?;

    let node = state.node.lock().await;
    let meta = node.store_blob(&path).map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(&meta).unwrap()))
}

pub async fn download_blob(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    let node = state.node.lock().await;
    let meta = node.get_blob_meta(&cid).map_err(ApiError::from)?;
    let temp_dir = tempfile::tempdir().map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;
    let file_path = temp_dir.path().join(&meta.original_name);
    node.export_blob(&cid, &file_path).map_err(ApiError::from)?;
    drop(node);

    let data = tokio::fs::read(&file_path)
        .await
        .map_err(|e| ApiError(onebrain_node::NodeError::Io(e)))?;

    Ok(axum::response::Response::builder()
        .status(200)
        .header("Content-Type", &meta.mime_type)
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", meta.original_name),
        )
        .body(axum::body::Body::from(data))
        .unwrap())
}

// ═══════════════════════════════════════════════════════════════════════════
// Tier C — Search History
// ═══════════════════════════════════════════════════════════════════════════

pub async fn record_search(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let query = body["query"].as_str().unwrap_or("");
    let result_count = body["result_count"].as_u64().unwrap_or(0) as usize;
    let mut node = state.node.lock().await;
    let entry = node.record_search(query, result_count);
    Ok(ok(serde_json::to_value(&entry).unwrap()))
}

pub async fn list_search_history(
    State(state): State<AppState>,
    Query(params): Query<LimitQuery>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let limit = params.limit.unwrap_or(50);
    let history = node.list_search_history(limit);
    Ok(ok(json!({ "history": history })))
}

pub async fn clear_search_history(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    node.clear_search_history();
    Ok(ok(json!({ "cleared": true })))
}

// ═══════════════════════════════════════════════════════════════════════════
// Tier C — Notification Preferences
// ═══════════════════════════════════════════════════════════════════════════

pub async fn get_notification_prefs(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let prefs = node.get_notification_prefs();
    Ok(ok(serde_json::to_value(&prefs).unwrap()))
}

pub async fn set_notification_prefs(
    State(state): State<AppState>,
    Json(prefs): Json<onebrain_node::types::NotificationPrefs>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    node.set_notification_prefs(prefs.clone());
    Ok(ok(serde_json::to_value(&prefs).unwrap()))
}

// ═══════════════════════════════════════════════════════════════════════════
// Tier C — Saved Searches
// ═══════════════════════════════════════════════════════════════════════════

pub async fn save_search(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let name = body["name"].as_str().unwrap_or("");
    let query = body["query"].as_str().unwrap_or("");
    let is_kql = body["is_kql"].as_bool().unwrap_or(false);
    let mut node = state.node.lock().await;
    let saved = node
        .save_search(name, query, is_kql)
        .map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(&saved).unwrap()))
}

pub async fn list_saved_searches(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let searches = node.list_saved_searches();
    Ok(ok(json!({ "saved_searches": searches })))
}

pub async fn delete_saved_search(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let deleted = node.delete_saved_search(&id).map_err(ApiError::from)?;
    Ok(ok(json!({ "deleted": deleted })))
}

// ═══════════════════════════════════════════════════════════════════════════
// Tier C — Collections
// ═══════════════════════════════════════════════════════════════════════════

pub async fn create_collection(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let name = body["name"].as_str().unwrap_or("");
    let description = body["description"].as_str().unwrap_or("");
    let mut node = state.node.lock().await;
    let coll = node
        .create_collection(name, description)
        .map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(&coll).unwrap()))
}

pub async fn list_collections(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let collections = node.list_collections();
    Ok(ok(json!({ "collections": collections })))
}

pub async fn get_collection(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let coll = node.get_collection(&id).ok_or_else(|| {
        ApiError(onebrain_node::NodeError::InvalidArgument(format!(
            "Collection '{}' not found",
            id
        )))
    })?;
    Ok(ok(serde_json::to_value(&coll).unwrap()))
}

pub async fn add_to_collection(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let cid_hex = body["cid_hex"].as_str().unwrap_or("");
    let mut node = state.node.lock().await;
    node.add_to_collection(&id, cid_hex)
        .map_err(ApiError::from)?;
    Ok(ok(json!({ "added": true })))
}

pub async fn remove_from_collection(
    State(state): State<AppState>,
    Path((id, cid)): Path<(String, String)>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    node.remove_from_collection(&id, &cid)
        .map_err(ApiError::from)?;
    Ok(ok(json!({ "removed": true })))
}

pub async fn delete_collection(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut node = state.node.lock().await;
    let deleted = node.delete_collection(&id).map_err(ApiError::from)?;
    Ok(ok(json!({ "deleted": deleted })))
}

// ═══════════════════════════════════════════════════════════════════════════
// Tier C — KU Version Chain
// ═══════════════════════════════════════════════════════════════════════════

pub async fn get_version_chain(
    State(state): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let chain = node.get_ku_version_chain(&cid).map_err(ApiError::from)?;
    Ok(ok(json!({ "versions": chain })))
}

// ═══════════════════════════════════════════════════════════════════════════
// Tier C — Trending KUs
// ═══════════════════════════════════════════════════════════════════════════

pub async fn trending_kus(
    State(state): State<AppState>,
    Query(params): Query<LimitQuery>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let limit = params.limit.unwrap_or(10);
    let trending = node.trending_kus(limit).map_err(ApiError::from)?;
    Ok(ok(json!({ "trending": trending })))
}

// ═══════════════════════════════════════════════════════════════════════════
// Tier C — Recommendations
// ═══════════════════════════════════════════════════════════════════════════

pub async fn recommended_kus(
    State(state): State<AppState>,
    Query(params): Query<LimitQuery>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let limit = params.limit.unwrap_or(10);
    let recs = node.recommended_kus(limit).map_err(ApiError::from)?;
    Ok(ok(json!({ "recommendations": recs })))
}

// ═══════════════════════════════════════════════════════════════════════════
// Tier C — Analytics
// ═══════════════════════════════════════════════════════════════════════════

pub async fn get_analytics(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let analytics = node.get_analytics().map_err(ApiError::from)?;
    Ok(ok(serde_json::to_value(&analytics).unwrap()))
}

// ═══════════════════════════════════════════════════════════════════════════
// Tier C — Domain Taxonomy
// ═══════════════════════════════════════════════════════════════════════════

pub async fn list_domains(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let domains = node.list_domains().map_err(ApiError::from)?;
    Ok(ok(json!({ "domains": domains })))
}

pub async fn kus_by_domain(
    State(state): State<AppState>,
    Path(domain): Path<String>,
    Query(params): Query<PaginationQuery>,
) -> ApiResult<serde_json::Value> {
    let node = state.node.lock().await;
    let page = params.page.unwrap_or(1);
    let limit = params.limit.unwrap_or(20);
    let (kus, total) = node
        .kus_by_domain(&domain, page, limit)
        .map_err(ApiError::from)?;
    Ok(ok(json!({ "kus": kus, "total": total, "page": page })))
}
