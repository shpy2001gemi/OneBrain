//! Thin authenticated transport for the D-029 node-owned OBP service.
use crate::server::AppState;
use axum::{
    extract::{Request, State, WebSocketUpgrade},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

fn private(mut r: Response) -> Response {
    r.headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    r
}
pub(crate) fn auth_error(status: StatusCode) -> Response {
    private((status,Json(json!({"ok":false,"error":{"code":"AUTH_REQUIRED","message":"OBP authentication required"}}))).into_response())
}
fn unavailable() -> Response {
    success(
        json!({"available":false,"compiled":cfg!(feature="vnext-outbound-first")}),
        None,
    )
}
#[cfg(not(feature = "vnext-outbound-first"))]
async fn valid_status_request(req: Request) -> bool {
    req.uri().query().is_none()
        && matches!(axum::body::to_bytes(req.into_body(),1).await,Ok(b) if b.is_empty())
}
fn success(data: Value, status: Option<&Value>) -> Response {
    let value = json!({"ok":true,"profile":crate::vnext_api::VNEXT_PRODUCT_PROFILE,"data":data,"meta":{
        "lifecycle":status.and_then(|s|s["lifecycle"].as_str()).unwrap_or("disabled"),
        "coverage":status.and_then(|s|s["coverage"].as_str()).unwrap_or("local_only"),
        "limitations":status.map(|s|s["limitations"].clone()).unwrap_or(json!(["obp_service_unavailable"])),"continuation":null}});
    private(Json(value).into_response())
}

#[cfg(feature = "vnext-outbound-first")]
pub use enabled::Binding;

#[cfg(feature = "vnext-outbound-first")]
mod enabled {
    use super::*;
    use onebrain_node::{
        vnext_product_runtime::obp::{self, contract, Control, Error, Session},
        VNextProductServices,
    };
    #[derive(Clone)]
    pub struct Binding {
        pub(crate) principal: [u8; 32],
        pub(crate) control: Option<Control>,
    }
    impl Binding {
        pub fn new(principal: [u8; 32], control: Option<Control>) -> Self {
            Self { principal, control }
        }
    }
    pub(crate) async fn service(
        state: &AppState,
    ) -> Result<(Binding, VNextProductServices), Error> {
        let binding = state
            .obp_binding
            .clone()
            .ok_or_else(|| Error::new("feature_disabled"))?;
        let services = state
            .vnext_product_services()
            .await
            .ok_or_else(|| Error::new("feature_disabled"))?;
        Ok((binding, services))
    }
    pub(crate) fn failure(error: Error) -> Response {
        if error.reason == "forbidden" {
            return auth_error(StatusCode::FORBIDDEN);
        }
        let (code, http, retryable) = match error.reason {
            "invalid_payload" => ("invalid_request", 400, false),
            "local_not_found" => ("not_found", 404, false),
            "generation_conflict" | "session_conflict" | "idempotency_conflict" => {
                ("conflict", 409, false)
            }
            "snapshot_expired" | "input_expired" => ("expired", 410, false),
            "admission_limit" => ("rate_limited", 429, true),
            "feature_disabled" => ("capability_disabled", 503, false),
            "dependency_unavailable" | "outcome_unknown" => ("dependency_unavailable", 503, true),
            _ => ("internal_error", 500, false),
        };
        private((StatusCode::from_u16(http).unwrap(),Json(json!({"ok":false,"profile":crate::vnext_api::VNEXT_PRODUCT_PROFILE,
            "error":{"code":code,"message":error.reason,"retryable":retryable,"limitations":[error.reason],"obp":{"reason":error.reason,"outcome":if error.unknown(){"unknown"}else{"not_admitted"},"reconcile_before_retry":error.unknown()}},
            "meta":{"lifecycle":"degraded","coverage":"local_only","limitations":[error.reason],"continuation":null}}))).into_response())
    }
    async fn body(req: Request) -> Result<(axum::http::HeaderMap, Value), Error> {
        if req.uri().query().is_some() {
            return Err(Error::invalid());
        }
        let content = req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .ok_or_else(Error::invalid)?;
        if !matches!(
            content.to_ascii_lowercase().replace(' ', "").as_str(),
            "application/json" | "application/json;charset=utf-8"
        ) {
            return Err(Error::invalid());
        }
        let (parts, body) = req.into_parts();
        let bytes = axum::body::to_bytes(body, contract::MAX_BYTES)
            .await
            .map_err(|_| Error::invalid())?;
        Ok((parts.headers, contract::parse(&bytes)?))
    }
    fn session(v: &Value) -> Result<Session, Error> {
        contract::fields(v, &["process_generation", "dataset_generation"], &[])?;
        contract::id(&v["process_generation"])?;
        contract::id(&v["dataset_generation"])?;
        serde_json::from_value(v.clone()).map_err(|_| Error::invalid())
    }
    pub(super) async fn status(state: AppState, req: Request) -> Response {
        if req.uri().query().is_some() {
            return failure(Error::invalid());
        }
        let (parts, body) = req.into_parts();
        if !matches!(axum::body::to_bytes(body,1).await,Ok(b) if b.is_empty()) {
            return failure(Error::invalid());
        }
        let Ok((binding, services)) = service(&state).await else {
            return unavailable();
        };
        match (services.obp_session(), services.obp_status().await) {
            (Ok(session), Ok(status)) => {
                state
                    .vnext_ws
                    .publish_obp(&parts.headers, binding.principal, &session, &status);
                success(
                    json!({"available":true,"session":session,"status":status}),
                    Some(&status),
                )
            }
            (Err(e), _) | (_, Err(e)) => failure(e),
        }
    }
    pub(super) async fn operation(state: AppState, req: Request, kind: &str) -> Response {
        let (binding, services) = match service(&state).await {
            Ok(v) => v,
            Err(e) => return failure(e),
        };
        let (headers, body) = match body(req).await {
            Ok(v) => v,
            Err(e) => return failure(e),
        };
        let result = async {
            let required = match kind {
                "reconcile" => vec!["session", "idempotency_key"],
                "tickets" => vec!["session", "subscriptions"],
                _ => vec!["session", "operation", "payload"],
            };
            contract::fields(&body, &required, &[])?;
            let session = session(&body["session"])?;
            if services.obp_session()? != session {
                return Err(Error::new("session_conflict"));
            }
            let value = match kind {
                "tickets" => {
                    if body["subscriptions"] != json!(["network"]) {
                        return Err(Error::invalid());
                    }
                    state
                        .vnext_ws
                        .issue_obp(binding.principal, session.clone())?
                }
                "reconcile" => services.obp_reconcile(
                    binding.principal,
                    &session,
                    contract::id(&body["idempotency_key"])?,
                )?,
                _ => {
                    let r = obp::Request::new(
                        body["operation"].as_str().ok_or_else(Error::invalid)?,
                        body["payload"].clone(),
                        kind == "commands",
                    )?;
                    if kind == "commands" {
                        let control = binding.control.as_ref().ok_or_else(Error::forbidden)?;
                        let token = headers
                            .get("X-OneBrain-OBP-Management")
                            .and_then(|v| v.to_str().ok());
                        services
                            .obp_command(binding.principal, &session, control, token, r)
                            .await?
                    } else {
                        services.obp_query(binding.principal, &session, r).await?
                    }
                }
            };
            let status = services.obp_status().await?;
            state
                .vnext_ws
                .publish_obp(&headers, binding.principal, &session, &status);
            if serde_json::to_vec(&value)
                .map_err(|_| Error::new("response_overflow"))?
                .len()
                > contract::MAX_BYTES - 16_384
            {
                return Err(Error::new("response_overflow"));
            }
            Ok((value, status))
        }
        .await;
        match result {
            Ok((value, status)) => success(value, Some(&status)),
            Err(error) => failure(error),
        }
    }
}

pub async fn status(State(state): State<AppState>, req: Request) -> Response {
    #[cfg(feature = "vnext-outbound-first")]
    {
        enabled::status(state, req).await
    }
    #[cfg(not(feature = "vnext-outbound-first"))]
    {
        let _ = state;
        if !valid_status_request(req).await {
            return private((StatusCode::BAD_REQUEST,Json(json!({"ok":false,"profile":crate::vnext_api::VNEXT_PRODUCT_PROFILE,"error":{"code":"invalid_request","message":"invalid_payload","retryable":false,"limitations":["invalid_payload"],"obp":{"reason":"invalid_payload","outcome":"not_admitted","reconcile_before_retry":false}},"meta":{"lifecycle":"disabled","coverage":"local_only","limitations":["invalid_payload"],"continuation":null}}))).into_response());
        }
        unavailable()
    }
}
macro_rules! operation {($name:ident,$kind:literal)=>{
    pub async fn $name(State(state):State<AppState>,req:Request)->Response {
        #[cfg(feature="vnext-outbound-first")] {enabled::operation(state,req,$kind).await}
        #[cfg(not(feature="vnext-outbound-first"))] {let _=(state,req);private((StatusCode::SERVICE_UNAVAILABLE,Json(json!({"ok":false,"profile":crate::vnext_api::VNEXT_PRODUCT_PROFILE,"error":{"code":"capability_disabled","message":"feature_disabled","retryable":false,"limitations":["feature_disabled"],"obp":{"reason":"feature_disabled","outcome":"not_admitted","reconcile_before_retry":false}},"meta":{"lifecycle":"disabled","coverage":"local_only","limitations":["feature_disabled"],"continuation":null}}))).into_response())}
    }
}}
operation!(query, "query");
operation!(commands, "commands");
operation!(reconcile, "reconcile");
operation!(tickets, "tickets");
pub async fn upgrade(
    State(state): State<AppState>,
    req: axum::extract::RawQuery,
    ws: Result<WebSocketUpgrade, axum::extract::ws::rejection::WebSocketUpgradeRejection>,
) -> Response {
    let Ok(ws) = ws else {
        return auth_error(StatusCode::UNAUTHORIZED);
    };
    #[cfg(feature = "vnext-outbound-first")]
    {
        let Some(ticket) = req
            .0
            .as_deref()
            .and_then(|q| q.strip_prefix("ticket="))
            .filter(|t| {
                t.len() == 48
                    && t.starts_with("obw1.")
                    && t[5..]
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            })
        else {
            return auth_error(StatusCode::UNAUTHORIZED);
        };
        crate::vnext_ws::obp::upgrade(
            state,
            std::collections::BTreeMap::from([("ticket".into(), ticket.into())]),
            ws,
        )
        .await
    }
    #[cfg(not(feature = "vnext-outbound-first"))]
    {
        let _ = (state, req, ws);
        auth_error(StatusCode::UNAUTHORIZED)
    }
}

#[cfg(test)]
mod tests;
