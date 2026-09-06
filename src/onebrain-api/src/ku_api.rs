//! Thin, private REST projection of the node-owned KU service.
//! No source intake, extraction, Registry selection, storage or WS authority lives here.
use axum::body::to_bytes;
use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use onebrain_base_contract::ku::*;
use onebrain_base_contract::ku_payload::{decode_hex, hex, MAX_KU_PAYLOAD_BYTES};
use onebrain_base_contract::{BaseErrorCodeV1, ResourceBudgetV1};
use onebrain_node::{ku_product::KuServices, BaseServiceError};
use serde::{Deserialize, Serialize};

use crate::server::AppState;
use crate::vnext_api::{
    CoverageV1, LifecycleV1, VNextMetaV1, VNextSuccessV1, VNEXT_PRODUCT_PROFILE,
};

const ENVELOPE_ALLOWANCE: u64 = 16_384;
const DEFAULT_BYTES: u64 = MAX_KU_PAYLOAD_BYTES as u64;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Session {
    process_generation: String,
    dataset_generation: String,
}

impl Session {
    fn check(&self, current: &Self) -> Result<(), BaseServiceError> {
        decode_hex::<32>(&self.process_generation).map_err(|_| invalid())?;
        decode_hex::<32>(&self.dataset_generation).map_err(|_| invalid())?;
        if self != current {
            return Err(BaseServiceError::new(
                BaseErrorCodeV1::Conflict,
                "refresh_session_then_reconcile",
            ));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Budget {
    max_items: u32,
    max_bytes: u64,
    max_work_units: u64,
}

impl Budget {
    fn service_budget(&self, request_bytes: usize) -> Result<ResourceBudgetV1, BaseServiceError> {
        if !(1..=256).contains(&self.max_items)
            || !(32_768..=DEFAULT_BYTES).contains(&self.max_bytes)
            || !(1..=1_000_000).contains(&self.max_work_units)
            || request_bytes as u64 > self.max_bytes
        {
            return Err(invalid());
        }
        ResourceBudgetV1::try_new(
            self.max_items,
            self.max_bytes - ENVELOPE_ALLOWANCE,
            self.max_work_units,
        )
        .map_err(|_| invalid())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReservationRequest {
    session: Session,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OperationRequest {
    session: Session,
    budget: Budget,
    request: Operation,
}

// Only the transport tag is new. Every payload is the generated, validated DTO.
#[derive(Deserialize)]
#[serde(
    tag = "operation",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Operation {
    Prepare(KuPrepareV1),
    Preview(KuOperationRefV1),
    Save(KuSaveV1),
    Get(KuGetV1),
    List(KuListV1),
    Search(KuSearchV1),
    Revise(KuReviseV1),
    Export(KuExportV1),
    Status(KuStatusRequestV1),
    Cancel(KuOperationRefV1),
    Reconcile(KuOperationRefV1),
}

impl From<Operation> for KuRequestV1 {
    fn from(value: Operation) -> Self {
        match value {
            Operation::Prepare(v) => Self::Prepare(v),
            Operation::Preview(v) => Self::Preview(v),
            Operation::Save(v) => Self::Save(v),
            Operation::Get(v) => Self::Get(v),
            Operation::List(v) => Self::List(v),
            Operation::Search(v) => Self::Search(v),
            Operation::Revise(v) => Self::Revise(v),
            Operation::Export(v) => Self::Export(v),
            Operation::Status(v) => Self::Status(v),
            Operation::Cancel(v) => Self::Cancel(v),
            Operation::Reconcile(v) => Self::Reconcile(v),
        }
    }
}

fn invalid() -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::InvalidRequest, "invalid_ku_request")
}

fn meta() -> VNextMetaV1 {
    VNextMetaV1 {
        lifecycle: LifecycleV1::Active,
        coverage: CoverageV1::LocalOnly,
        limitations: vec![
            "local_only".into(),
            "real_model_unqualified".into(),
            "host_admitted_sources_required".into(),
        ],
        continuation: None,
    }
}

fn private_response(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}

// Error reasons are bounded static host codes, never parser errors/private input text.
fn failure(error: BaseServiceError) -> Response {
    use BaseErrorCodeV1 as B;
    let (status, outer, code) = match error.code {
        B::InvalidRequest => (
            StatusCode::BAD_REQUEST,
            "invalid_request",
            BaseError::InvalidRequest,
        ),
        B::NotFound => (StatusCode::NOT_FOUND, "not_found", BaseError::NotFound),
        B::Conflict => (StatusCode::CONFLICT, "conflict", BaseError::Conflict),
        B::Expired => (StatusCode::GONE, "expired", BaseError::Expired),
        B::RateLimited => (
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            BaseError::RateLimited,
        ),
        B::CapabilityDisabled => (
            StatusCode::SERVICE_UNAVAILABLE,
            "capability_disabled",
            BaseError::CapabilityDisabled,
        ),
        B::DependencyUnavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "dependency_unavailable",
            BaseError::DependencyUnavailable,
        ),
        B::IncompatibleProfile => (
            StatusCode::SERVICE_UNAVAILABLE,
            "capability_disabled",
            BaseError::IncompatibleProfile,
        ),
        B::ResourceExhausted => (
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            BaseError::ResourceExhausted,
        ),
        B::CorruptState => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            BaseError::CorruptState,
        ),
        B::ReprovisionRequired => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            BaseError::ReprovisionRequired,
        ),
        B::UnknownOutcome => (StatusCode::CONFLICT, "conflict", BaseError::UnknownOutcome),
        B::InternalError => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            BaseError::InternalError,
        ),
    };
    let mut metadata = meta();
    metadata.lifecycle = if error.code == B::CapabilityDisabled {
        LifecycleV1::Disabled
    } else {
        LifecycleV1::Degraded
    };
    let detail = KuFailureV1 {
        code,
        retryable: error.retryable,
        reconcile_before_retry: error.reconcile_before_retry,
        limitations: vec![error.reason.into()],
    };
    private_response(
        (
            status,
            Json(serde_json::json!({
                "ok": false, "profile": VNEXT_PRODUCT_PROFILE,
                "error": {"code":outer,"message":error.reason,
                    "retryable": matches!(outer, "rate_limited" | "dependency_unavailable"),
                    "limitations": metadata.limitations,
                    "discriminator":error.code.discriminator(),"failure":detail},
                "meta":metadata,
            })),
        )
            .into_response(),
    )
}

pub(crate) fn authentication_error(status: StatusCode) -> Response {
    let mut response = failure(invalid());
    *response.status_mut() = status;
    response
}

async fn service(state: &AppState) -> Result<(KuServices, Session), BaseServiceError> {
    let node = state.node.lock().await;
    let base = node.base_services().ok_or_else(|| {
        BaseServiceError::new(
            BaseErrorCodeV1::DependencyUnavailable,
            "base_runtime_not_installed",
        )
    })?;
    let snapshot = base.snapshot()?;
    let ku = node.ku_services([0; 32], state.api_token.as_bytes())?;
    Ok((
        ku,
        Session {
            process_generation: hex(snapshot.process_generation.as_bytes()),
            dataset_generation: hex(&snapshot.dataset_generation.0),
        },
    ))
    // Aggregate mutex is released before invoking the returned handle.
}

async fn body<T: serde::de::DeserializeOwned>(
    request: Request,
) -> Result<(T, usize), BaseServiceError> {
    if request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_none_or(|s| s.split(';').next().map(str::trim) != Some("application/json"))
    {
        return Err(invalid());
    }
    let bytes = to_bytes(request.into_body(), MAX_KU_PAYLOAD_BYTES)
        .await
        .map_err(|_| invalid())?;
    let value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    Ok((value, bytes.len()))
}

#[derive(Serialize)]
struct Data<T: Serialize> {
    session: Session,
    payload: T,
    model_qualified: bool,
}

struct BoundedWriter {
    bytes: Vec<u8>,
    limit: usize,
}
impl std::io::Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::ErrorKind::OutOfMemory.into());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn success<T: Serialize>(
    session: Session,
    payload: T,
    metadata: VNextMetaV1,
    limit: u64,
) -> Response {
    let envelope = VNextSuccessV1 {
        ok: true,
        profile: VNEXT_PRODUCT_PROFILE,
        data: Data {
            session,
            payload,
            model_qualified: false,
        },
        meta: metadata,
    };
    let mut writer = BoundedWriter {
        bytes: Vec::new(),
        limit: limit as usize,
    };
    if serde_json::to_writer(&mut writer, &envelope).is_err() {
        return failure(BaseServiceError::new(
            BaseErrorCodeV1::ResourceExhausted,
            "response_limit_reconcile_before_retry",
        ));
    }
    private_response(([(header::CONTENT_TYPE, "application/json")], writer.bytes).into_response())
}

fn project(session: Session, response: KuResponseV1, limit: u64) -> Response {
    let mut metadata = meta();
    // Copy the service's coverage/state; never infer complete from HTTP success.
    match &response {
        KuResponseV1::Prepare(p) | KuResponseV1::Preview(p) | KuResponseV1::Revise(p) => {
            metadata.limitations.extend(p.limitations.clone());
            if p.validity != Validity::Ready {
                metadata.coverage = CoverageV1::Partial;
            }
        }
        KuResponseV1::Get(p) => {
            metadata.coverage = coverage(p.coverage);
            metadata.limitations.extend(p.limitations.clone());
        }
        KuResponseV1::List(p) | KuResponseV1::Search(p) => {
            metadata.coverage = coverage(p.coverage);
            metadata.continuation = p.continuation.clone();
            metadata.limitations.extend(p.limitations.clone());
        }
        KuResponseV1::Status(p) => {
            metadata.lifecycle = match p.lifecycle {
                Lifecycle::Active => LifecycleV1::Active,
                Lifecycle::Disabled => LifecycleV1::Disabled,
                Lifecycle::Requested => LifecycleV1::Requested,
                Lifecycle::Degraded => LifecycleV1::Degraded,
            };
            metadata.coverage = coverage(p.coverage);
            metadata.limitations.extend(p.limitations.clone());
        }
        KuResponseV1::Save(p) | KuResponseV1::Cancel(p) | KuResponseV1::Reconcile(p) => {
            metadata.limitations.extend(p.limitations.clone());
            if p.state == BaseState::UnknownOutcome {
                metadata.limitations.push("reconcile_before_retry".into());
            }
        }
        KuResponseV1::Export(p) => {
            metadata.limitations.extend(p.limitations.clone());
        }
    }
    let bytes = match response.payload_bytes() {
        Ok(bytes) => bytes,
        Err(_) => {
            return failure(BaseServiceError::new(
                BaseErrorCodeV1::InternalError,
                "invalid_service_projection_reconcile_before_retry",
            ))
        }
    };
    // Validated generated JSON is embedded, not recompiled or reinterpreted.
    match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(payload) => success(session, payload, metadata, limit),
        Err(_) => failure(BaseServiceError::new(
            BaseErrorCodeV1::InternalError,
            "invalid_service_projection_reconcile_before_retry",
        )),
    }
}

fn coverage(value: Coverage) -> CoverageV1 {
    match value {
        Coverage::LocalOnly => CoverageV1::LocalOnly,
        Coverage::Partial => CoverageV1::Partial,
    }
}

pub async fn status(State(state): State<AppState>) -> Response {
    let (ku, session) = match service(&state).await {
        Ok(v) => v,
        Err(e) => return failure(e),
    };
    let budget =
        ResourceBudgetV1::try_new(256, DEFAULT_BYTES - ENVELOPE_ALLOWANCE, 1_000_000).unwrap();
    match ku
        .invoke(
            KuRequestV1::Status(KuStatusRequestV1 { operation_id: None }),
            budget,
        )
        .await
    {
        Ok(value) => project(session, value, DEFAULT_BYTES),
        Err(e) => failure(e),
    }
}

pub async fn reserve(State(state): State<AppState>, request: Request) -> Response {
    let (body, _) = match body::<ReservationRequest>(request).await {
        Ok(v) => v,
        Err(e) => return failure(e),
    };
    let (ku, session) = match service(&state).await {
        Ok(v) => v,
        Err(e) => return failure(e),
    };
    if let Err(e) = body.session.check(&session) {
        return failure(e);
    }
    match ku.reserve().await {
        Ok(operation_id) => success(
            session,
            KuOperationRefV1 { operation_id },
            meta(),
            DEFAULT_BYTES,
        ),
        Err(e) => failure(e),
    }
}

pub async fn invoke(State(state): State<AppState>, request: Request) -> Response {
    let (body, size) = match body::<OperationRequest>(request).await {
        Ok(v) => v,
        Err(e) => return failure(e),
    };
    let budget = match body.budget.service_budget(size) {
        Ok(v) => v,
        Err(e) => return failure(e),
    };
    let request = KuRequestV1::from(body.request);
    if request.validate().is_err() {
        return failure(invalid());
    }
    let ai = match &request {
        KuRequestV1::Prepare(p) => p.input_mode == InputMode::LocalAi,
        KuRequestV1::Revise(p) => p.preparation.input_mode == InputMode::LocalAi,
        _ => false,
    };
    if ai {
        return failure(BaseServiceError::new(
            BaseErrorCodeV1::CapabilityDisabled,
            "real_model_unqualified",
        ));
    }
    let (ku, session) = match service(&state).await {
        Ok(v) => v,
        Err(e) => return failure(e),
    };
    if let Err(e) = body.session.check(&session) {
        return failure(e);
    }
    match ku.invoke(request, budget).await {
        Ok(value) => project(session, value, body.budget.max_bytes),
        Err(e) => failure(e),
    }
}

#[cfg(test)]
mod tests;
