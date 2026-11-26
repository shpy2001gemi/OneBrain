//! Pillar 10 CLI subcommands (`onebrain mesh`, `onebrain surface`, `onebrain obt`)
//! routing over the authenticated local node API (`P10-SURFACES-SYNC-001`).

use std::path::PathBuf;

use clap::{Args, Subcommand};
use serde_json::{json, Value};

use crate::resolve_client_api_token;

#[derive(Args, Debug)]
pub(crate) struct LocalApiTarget {
    #[arg(long, default_value = "http://127.0.0.1:4280")]
    pub api_url: String,
    #[arg(long, conflicts_with = "api_token_file")]
    pub api_token: Option<String>,
    #[arg(long, value_name = "PATH", conflicts_with = "api_token")]
    pub api_token_file: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub(crate) struct MeshArgs {
    #[command(flatten)]
    pub target: LocalApiTarget,
    #[command(subcommand)]
    pub command: MeshCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum MeshCommand {
    /// Inspect Personal Mesh epoch, 4-principal separation, and enrolled devices.
    Status,
    /// Start an out-of-band SPAKE2+/QR Personal Mesh device pairing ceremony.
    PairStart {
        #[arg(long)]
        label: String,
        #[arg(long, default_value = "mobile")]
        surface_kind: String,
    },
    /// Confirm SPAKE2+ Short Authentication String to issue ActorDelegation/1.
    PairConfirm {
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        sas_digits: String,
    },
    /// Revoke a lost or compromised device via ActorRevocation/1 (rotates mesh_epoch + 1).
    Revoke {
        #[arg(long)]
        device_id: String,
        #[arg(long, default_value = "lost")]
        reason: String,
    },
}

#[derive(Args, Debug)]
pub(crate) struct SurfaceArgs {
    #[command(flatten)]
    pub target: LocalApiTarget,
    #[command(subcommand)]
    pub command: SurfaceCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum SurfaceCommand {
    /// List issued scoped `obc_surface1` capability tokens.
    List,
    /// Mint a least-privilege `obc_surface1` capability token for an adapter surface.
    Mint {
        #[arg(long)]
        surface_kind: String,
        #[arg(long)]
        label: String,
        #[arg(long, default_value = "loopback")]
        binding: String,
    },
    /// Revoke an `obc_surface1` token by its 64-hex token_id.
    Revoke {
        #[arg(long)]
        token_id: String,
    },
}

#[derive(Args, Debug)]
pub(crate) struct ObtArgs {
    #[command(flatten)]
    pub target: LocalApiTarget,
    #[command(subcommand)]
    pub command: ObtCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum ObtCommand {
    /// Inspect firewalled vNext OBT shadow/settlement status (`[Simulated · Non-Economic Compatibility View]`).
    Status,
}

async fn send_local_json(
    target: &LocalApiTarget,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let token = resolve_client_api_token(
        target.api_token.as_deref(),
        target.api_token_file.as_deref(),
    )?;
    let url = format!("{}{}", target.api_url.trim_end_matches('/'), path);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("failed to build local HTTP client: {e}"))?;
    let mut req = client.request(method, &url).bearer_auth(token);
    if let Some(payload) = body {
        req = req.json(&payload);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("request to {url} failed: {e}"))?;
    let status = resp.status();
    let parsed: Value = resp
        .json()
        .await
        .map_err(|e| format!("invalid JSON from {url} ({status}): {e}"))?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {parsed}"));
    }
    Ok(parsed)
}

pub(crate) async fn execute_mesh(args: MeshArgs) -> Result<(), String> {
    let value = match args.command {
        MeshCommand::Status => {
            send_local_json(
                &args.target,
                reqwest::Method::GET,
                "/api/vnext/mesh/status",
                None,
            )
            .await?
        }
        MeshCommand::PairStart {
            label,
            surface_kind,
        } => {
            send_local_json(
                &args.target,
                reqwest::Method::POST,
                "/api/vnext/mesh/pair/start",
                Some(json!({ "device_label": label, "surface_kind": surface_kind })),
            )
            .await?
        }
        MeshCommand::PairConfirm {
            session_id,
            sas_digits,
        } => {
            send_local_json(
                &args.target,
                reqwest::Method::POST,
                "/api/vnext/mesh/pair/confirm",
                Some(json!({
                    "session_id": session_id,
                    "sas_confirmed": true,
                    "sas_digits": sas_digits
                })),
            )
            .await?
        }
        MeshCommand::Revoke { device_id, reason } => {
            send_local_json(
                &args.target,
                reqwest::Method::POST,
                "/api/vnext/mesh/revoke",
                Some(json!({ "device_id": device_id, "reason": reason })),
            )
            .await?
        }
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?
    );
    Ok(())
}

pub(crate) async fn execute_surface(args: SurfaceArgs) -> Result<(), String> {
    let value = match args.command {
        SurfaceCommand::List => {
            send_local_json(
                &args.target,
                reqwest::Method::GET,
                "/api/vnext/surfaces/tokens",
                None,
            )
            .await?
        }
        SurfaceCommand::Mint {
            surface_kind,
            label,
            binding,
        } => {
            send_local_json(
                &args.target,
                reqwest::Method::POST,
                "/api/vnext/surfaces/tokens",
                Some(json!({
                    "surface_kind": surface_kind,
                    "label": label,
                    "bound_channel_or_origin": binding
                })),
            )
            .await?
        }
        SurfaceCommand::Revoke { token_id } => {
            send_local_json(
                &args.target,
                reqwest::Method::DELETE,
                &format!("/api/vnext/surfaces/tokens/{token_id}"),
                None,
            )
            .await?
        }
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?
    );
    Ok(())
}

pub(crate) async fn execute_obt(args: ObtArgs) -> Result<(), String> {
    let value = match args.command {
        ObtCommand::Status => {
            send_local_json(
                &args.target,
                reqwest::Method::GET,
                "/api/vnext/obt/status",
                None,
            )
            .await?
        }
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?
    );
    Ok(())
}
