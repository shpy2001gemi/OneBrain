//! Pillar 10 Surface Capability Tokens & Route Ceiling Guard (`P10` §4, `FW-10`).
//!
//! Enforces strict capability ceilings across all 6 client surface classes
//! (`Desktop`, `Web`, `Cli`, `Extension`, `Bot`, `Glasses`) using domain-separated
//! `"obc_surface1.<claims>.<mac>"` tokens while ensuring `obc_surface1` tokens
//! NEVER substitute for local `obm1.*` management credentials (`FW-10`).

use std::collections::{BTreeMap, BTreeSet};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ku_core::foundation::{
    decode_canonical, encode_canonical, ActorId, CanonicalValue, DeviceId, ResourceProfile,
};
use thiserror::Error;

pub const SURFACE_TOKEN_PREFIX: &str = "obc_surface1.";
pub const MANAGEMENT_TOKEN_PREFIX: &str = "obm1.";
const SURFACE_MAC_DOMAIN: &[u8] = b"onebrain:vnext:obc-surface1-mac:1\0";
const SURFACE_CLAIMS_MAJOR: u64 = 1;
pub const MAX_SURFACE_TOKEN_TTL_SECS: u64 = 30 * 24 * 3600;

/// Client surface classification (`P10` §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u64)]
pub enum SurfaceKind {
    Desktop = 1,
    Web = 2,
    Cli = 3,
    Extension = 4,
    Bot = 5,
    Glasses = 6,
}

impl SurfaceKind {
    pub const ALL: [Self; 6] = [
        Self::Desktop,
        Self::Web,
        Self::Cli,
        Self::Extension,
        Self::Bot,
        Self::Glasses,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Web => "web",
            Self::Cli => "cli",
            Self::Extension => "extension",
            Self::Bot => "bot",
            Self::Glasses => "glasses",
        }
    }

    pub const fn from_code(code: u64) -> Option<Self> {
        match code {
            1 => Some(Self::Desktop),
            2 => Some(Self::Web),
            3 => Some(Self::Cli),
            4 => Some(Self::Extension),
            5 => Some(Self::Bot),
            6 => Some(Self::Glasses),
            _ => None,
        }
    }

    /// Maximum scopes a surface class may ever hold (`P10` §4).
    pub fn max_allowed_scopes(self) -> BTreeSet<SurfaceScope> {
        match self {
            Self::Desktop | Self::Cli => BTreeSet::from([
                SurfaceScope::ReadStatusAndQuery,
                SurfaceScope::CaptureSourceAndDraft,
                SurfaceScope::StageProposalAndPreview,
                SurfaceScope::ConfirmExplicitUserGesture,
                SurfaceScope::WalletReadAndProvisionalView,
                SurfaceScope::PersonalMeshPairAndSync,
            ]),
            Self::Web => BTreeSet::from([
                SurfaceScope::ReadStatusAndQuery,
                SurfaceScope::CaptureSourceAndDraft,
                SurfaceScope::StageProposalAndPreview,
                SurfaceScope::ConfirmExplicitUserGesture,
                SurfaceScope::WalletReadAndProvisionalView,
            ]),
            Self::Extension => BTreeSet::from([
                SurfaceScope::ReadStatusAndQuery,
                SurfaceScope::CaptureSourceAndDraft,
            ]),
            Self::Bot => BTreeSet::from([
                SurfaceScope::ReadStatusAndQuery,
                SurfaceScope::CaptureSourceAndDraft,
                SurfaceScope::StageProposalAndPreview,
            ]),
            Self::Glasses => BTreeSet::from([
                SurfaceScope::ReadStatusAndQuery,
                SurfaceScope::CaptureSourceAndDraft,
                SurfaceScope::SomaticEmergencyAbort,
            ]),
        }
    }

    pub const fn may_hold_obm1_management(self) -> bool {
        matches!(self, Self::Desktop | Self::Cli)
    }
}

/// Fine-grained surface capability scopes bounded by `SurfaceKind::max_allowed_scopes`.
/// Notice that `AdministerNode` is intentionally absent from `obc_surface1`: node
/// administration always requires `obm1.*` (`FW-10`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u64)]
pub enum SurfaceScope {
    ReadStatusAndQuery = 1,
    CaptureSourceAndDraft = 2,
    StageProposalAndPreview = 3,
    ConfirmExplicitUserGesture = 4,
    WalletReadAndProvisionalView = 5,
    PersonalMeshPairAndSync = 6,
    SomaticEmergencyAbort = 7,
}

impl SurfaceScope {
    pub const fn from_code(code: u64) -> Option<Self> {
        match code {
            1 => Some(Self::ReadStatusAndQuery),
            2 => Some(Self::CaptureSourceAndDraft),
            3 => Some(Self::StageProposalAndPreview),
            4 => Some(Self::ConfirmExplicitUserGesture),
            5 => Some(Self::WalletReadAndProvisionalView),
            6 => Some(Self::PersonalMeshPairAndSync),
            7 => Some(Self::SomaticEmergencyAbort),
            _ => None,
        }
    }
}

/// Canonical claims carried inside `"obc_surface1.<claims>.<mac>"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceCapabilityClaimsV1 {
    pub token_id: [u8; 32],
    pub surface_kind: SurfaceKind,
    pub actor_id: ActorId,
    pub device_id: DeviceId,
    pub allowed_scopes: BTreeSet<SurfaceScope>,
    pub issued_at_unix_s: u64,
    pub expires_at_unix_s: u64,
}

impl SurfaceCapabilityClaimsV1 {
    pub fn validate(&self) -> Result<(), SurfaceCapabilityError> {
        if self.token_id == [0; 32]
            || self.actor_id.as_bytes() == &[0; 32]
            || self.device_id.as_bytes() == &[0; 32]
            || self.actor_id.as_bytes() == self.device_id.as_bytes()
        {
            return Err(SurfaceCapabilityError::InvalidPrincipal);
        }
        if self.allowed_scopes.is_empty() {
            return Err(SurfaceCapabilityError::EmptyScopes);
        }
        if self.expires_at_unix_s <= self.issued_at_unix_s
            || self.expires_at_unix_s - self.issued_at_unix_s > MAX_SURFACE_TOKEN_TTL_SECS
        {
            return Err(SurfaceCapabilityError::InvalidLifetime);
        }
        let ceiling = self.surface_kind.max_allowed_scopes();
        for scope in &self.allowed_scopes {
            if !ceiling.contains(scope) {
                return Err(SurfaceCapabilityError::CeilingExceeded {
                    surface: self.surface_kind,
                    forbidden_scope: *scope,
                });
            }
        }
        Ok(())
    }

    pub fn encode_canonical_bytes(&self) -> Result<Vec<u8>, SurfaceCapabilityError> {
        self.validate()?;
        let scopes = self
            .allowed_scopes
            .iter()
            .map(|s| CanonicalValue::Unsigned(*s as u64))
            .collect();
        let value = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(SURFACE_CLAIMS_MAJOR)),
            (1, CanonicalValue::Bytes(self.token_id.to_vec())),
            (2, CanonicalValue::Unsigned(self.surface_kind as u64)),
            (3, CanonicalValue::Bytes(self.actor_id.as_bytes().to_vec())),
            (4, CanonicalValue::Bytes(self.device_id.as_bytes().to_vec())),
            (5, CanonicalValue::Array(scopes)),
            (6, CanonicalValue::Unsigned(self.issued_at_unix_s)),
            (7, CanonicalValue::Unsigned(self.expires_at_unix_s)),
        ]);
        encode_canonical(&value, ResourceProfile::ControlV1)
            .map_err(|_| SurfaceCapabilityError::MalformedToken)
    }

    pub fn decode_canonical_bytes(bytes: &[u8]) -> Result<Self, SurfaceCapabilityError> {
        let value = decode_canonical(bytes, ResourceProfile::ControlV1)
            .map_err(|_| SurfaceCapabilityError::MalformedToken)?;
        let CanonicalValue::Map(fields) = value else {
            return Err(SurfaceCapabilityError::MalformedToken);
        };
        if fields.len() != 8
            || fields
                .iter()
                .enumerate()
                .any(|(idx, (k, _))| *k != idx as u64)
        {
            return Err(SurfaceCapabilityError::MalformedToken);
        }
        if expect_u64(&fields[0].1)? != SURFACE_CLAIMS_MAJOR {
            return Err(SurfaceCapabilityError::MalformedToken);
        }
        let token_id = expect_bytes32(&fields[1].1)?;
        let surface_kind = SurfaceKind::from_code(expect_u64(&fields[2].1)?)
            .ok_or(SurfaceCapabilityError::MalformedToken)?;
        let actor_id = ActorId::from_bytes(expect_bytes32(&fields[3].1)?);
        let device_id = DeviceId::from_bytes(expect_bytes32(&fields[4].1)?);
        let CanonicalValue::Array(scope_arr) = &fields[5].1 else {
            return Err(SurfaceCapabilityError::MalformedToken);
        };
        let mut allowed_scopes = BTreeSet::new();
        for item in scope_arr {
            let scope = SurfaceScope::from_code(expect_u64(item)?)
                .ok_or(SurfaceCapabilityError::MalformedToken)?;
            if !allowed_scopes.insert(scope) {
                return Err(SurfaceCapabilityError::MalformedToken);
            }
        }
        let issued_at_unix_s = expect_u64(&fields[6].1)?;
        let expires_at_unix_s = expect_u64(&fields[7].1)?;
        let claims = Self {
            token_id,
            surface_kind,
            actor_id,
            device_id,
            allowed_scopes,
            issued_at_unix_s,
            expires_at_unix_s,
        };
        claims.validate()?;
        if claims.encode_canonical_bytes()? != bytes {
            return Err(SurfaceCapabilityError::MalformedToken);
        }
        Ok(claims)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceCapabilityTokenMetadata {
    pub token_id: [u8; 32],
    pub surface_kind: SurfaceKind,
    pub actor_id: ActorId,
    pub device_id: DeviceId,
    pub allowed_scopes: BTreeSet<SurfaceScope>,
    pub issued_at_unix_s: u64,
    pub expires_at_unix_s: u64,
    pub revoked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceRouteDecision {
    Allowed {
        required_scope: SurfaceScope,
        requires_obm1_management: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum SurfaceCapabilityError {
    #[error("surface capability root secret must be non-zero")]
    ZeroSecret,
    #[error("surface capability principal IDs must be non-zero and distinct")]
    InvalidPrincipal,
    #[error("surface capability scopes cannot be empty")]
    EmptyScopes,
    #[error("surface capability token lifetime is invalid or exceeds 30 days")]
    InvalidLifetime,
    #[error("surface {surface:?} cannot hold scope {forbidden_scope:?} (P10 §4 ceiling)")]
    CeilingExceeded {
        surface: SurfaceKind,
        forbidden_scope: SurfaceScope,
    },
    #[error("obm1.* management token rejected on obc_surface1 boundary (FW-10)")]
    ManagementTokenOnSurfaceChannel,
    #[error("malformed obc_surface1 token")]
    MalformedToken,
    #[error("surface token MAC verification failed")]
    InvalidMac,
    #[error("surface token was revoked or never registered")]
    TokenRevokedOrUnknown,
    #[error("surface token is expired or not yet valid")]
    TokenExpired,
    #[error("HTTP route {method} {path} requires an authenticated obm1.* management header")]
    MissingObm1ManagementHeader { method: String, path: String },
    #[error("surface {surface:?} is forbidden from accessing HTTP route {method} {path}")]
    RouteForbiddenForSurface {
        surface: SurfaceKind,
        method: String,
        path: String,
    },
}

/// Host-local registry minting, verifying, listing, and revoking `obc_surface1` tokens.
pub struct SurfaceCapabilityRegistry {
    mac_key: [u8; 32],
    issued: BTreeMap<[u8; 32], SurfaceCapabilityTokenMetadata>,
}

impl SurfaceCapabilityRegistry {
    pub fn new(root_secret: [u8; 32]) -> Result<Self, SurfaceCapabilityError> {
        if root_secret == [0; 32] {
            return Err(SurfaceCapabilityError::ZeroSecret);
        }
        let mac_key = blake3::derive_key("onebrain:vnext:obc-surface1-key:1", &root_secret);
        Ok(Self {
            mac_key,
            issued: BTreeMap::new(),
        })
    }

    pub fn mint_token(
        &mut self,
        claims: SurfaceCapabilityClaimsV1,
    ) -> Result<String, SurfaceCapabilityError> {
        let claims_bytes = claims.encode_canonical_bytes()?;
        let mac = self.compute_mac(&claims_bytes);
        let claims_b64 = URL_SAFE_NO_PAD.encode(&claims_bytes);
        let mac_b64 = URL_SAFE_NO_PAD.encode(mac);
        self.issued.insert(
            claims.token_id,
            SurfaceCapabilityTokenMetadata {
                token_id: claims.token_id,
                surface_kind: claims.surface_kind,
                actor_id: claims.actor_id,
                device_id: claims.device_id,
                allowed_scopes: claims.allowed_scopes,
                issued_at_unix_s: claims.issued_at_unix_s,
                expires_at_unix_s: claims.expires_at_unix_s,
                revoked: false,
            },
        );
        Ok(format!("{SURFACE_TOKEN_PREFIX}{claims_b64}.{mac_b64}"))
    }

    pub fn list_tokens(&self) -> Vec<SurfaceCapabilityTokenMetadata> {
        self.issued.values().cloned().collect()
    }

    pub fn revoke_token(&mut self, token_id: &[u8; 32]) -> bool {
        if let Some(meta) = self.issued.get_mut(token_id) {
            if !meta.revoked {
                meta.revoked = true;
                return true;
            }
        }
        false
    }

    pub fn verify_token(
        &self,
        token: &str,
        now_unix_s: u64,
    ) -> Result<SurfaceCapabilityClaimsV1, SurfaceCapabilityError> {
        if token.starts_with(MANAGEMENT_TOKEN_PREFIX) {
            return Err(SurfaceCapabilityError::ManagementTokenOnSurfaceChannel);
        }
        let rest = token
            .strip_prefix(SURFACE_TOKEN_PREFIX)
            .ok_or(SurfaceCapabilityError::MalformedToken)?;
        let (claims_part, mac_part) = rest
            .split_once('.')
            .ok_or(SurfaceCapabilityError::MalformedToken)?;
        if claims_part.is_empty() || mac_part.is_empty() || mac_part.contains('.') {
            return Err(SurfaceCapabilityError::MalformedToken);
        }
        let claims_bytes = URL_SAFE_NO_PAD
            .decode(claims_part)
            .map_err(|_| SurfaceCapabilityError::MalformedToken)?;
        let mac_bytes: [u8; 32] = URL_SAFE_NO_PAD
            .decode(mac_part)
            .map_err(|_| SurfaceCapabilityError::MalformedToken)?
            .try_into()
            .map_err(|_| SurfaceCapabilityError::MalformedToken)?;
        let expected_mac = self.compute_mac(&claims_bytes);
        if !constant_time_eq_32(&expected_mac, &mac_bytes) {
            return Err(SurfaceCapabilityError::InvalidMac);
        }
        let claims = SurfaceCapabilityClaimsV1::decode_canonical_bytes(&claims_bytes)?;
        let meta = self
            .issued
            .get(&claims.token_id)
            .ok_or(SurfaceCapabilityError::TokenRevokedOrUnknown)?;
        if meta.revoked {
            return Err(SurfaceCapabilityError::TokenRevokedOrUnknown);
        }
        if now_unix_s < claims.issued_at_unix_s || now_unix_s >= claims.expires_at_unix_s {
            return Err(SurfaceCapabilityError::TokenExpired);
        }
        Ok(claims)
    }

    /// Enforce `P10 §4` HTTP route capability ceilings for `surface_kind` and `obm1.*` presence.
    pub fn authorize_http_route(
        surface_kind: SurfaceKind,
        method: &str,
        path: &str,
        has_obm1_header: bool,
    ) -> Result<SurfaceRouteDecision, SurfaceCapabilityError> {
        let method_upper = method.trim().to_ascii_uppercase();
        let clean_path = path.split('?').next().unwrap_or(path).trim();

        // 1. Raw biometric export and host surface-token administration routes are forbidden
        //    across ALL obc_surface1 tokens (FW-04, FW-10: mint/revoke requires HostOwner bearer).
        if clean_path.contains("somatic-raw")
            || clean_path.contains("biometric-raw")
            || clean_path.starts_with("/api/vnext/surfaces")
        {
            return Err(SurfaceCapabilityError::RouteForbiddenForSurface {
                surface: surface_kind,
                method: method_upper,
                path: clean_path.to_owned(),
            });
        }

        // 1b. Strict BotPrincipal DMZ isolation (`NEG-P10-003`, `T-BCI-04`):
        //     `Bot` tokens may access ONLY `/api/vnext/bot/query` and `/api/vnext/bot/submissions`.
        if surface_kind == SurfaceKind::Bot {
            return match (method_upper.as_str(), clean_path) {
                ("POST", "/api/vnext/bot/query") => Ok(SurfaceRouteDecision::Allowed {
                    required_scope: SurfaceScope::ReadStatusAndQuery,
                    requires_obm1_management: false,
                }),
                ("POST", "/api/vnext/bot/submissions") => Ok(SurfaceRouteDecision::Allowed {
                    required_scope: SurfaceScope::CaptureSourceAndDraft,
                    requires_obm1_management: false,
                }),
                _ => Err(SurfaceCapabilityError::RouteForbiddenForSurface {
                    surface: surface_kind,
                    method: method_upper,
                    path: clean_path.to_owned(),
                }),
            };
        }

        // 1c. Strict Extension isolation (`NEG-P10-002`, `T-BCI-04`):
        //     `Extension` cannot access HUD/somatic routes, bulk PrivateVault (`/api/kus`),
        //     backup/restore, or Base management.
        if surface_kind == SurfaceKind::Extension
            && (clean_path.starts_with("/api/vnext/hud")
                || clean_path.starts_with("/api/kus")
                || clean_path.starts_with("/api/export")
                || clean_path.starts_with("/api/backup")
                || clean_path.starts_with("/api/restore")
                || clean_path.starts_with("/api/base/v1"))
        {
            return Err(SurfaceCapabilityError::RouteForbiddenForSurface {
                surface: surface_kind,
                method: method_upper,
                path: clean_path.to_owned(),
            });
        }

        // 2. Node administration, archive secret, rollout, signer, OBT mode, and device revocation
        //    routes require BOTH an `obm1.*` management header AND a sovereign surface (`Desktop` | `Cli`).
        let is_management_route = clean_path.starts_with("/api/admin")
            || clean_path.starts_with("/api/v1/admin")
            || clean_path.starts_with("/api/rollout")
            || clean_path.starts_with("/api/archive")
            || clean_path.starts_with("/api/signer")
            || clean_path.starts_with("/api/mesh/revoke")
            || clean_path.starts_with("/api/vnext/mesh/revoke")
            || clean_path.starts_with("/api/obt/mode");
        if is_management_route {
            if !surface_kind.may_hold_obm1_management() {
                return Err(SurfaceCapabilityError::RouteForbiddenForSurface {
                    surface: surface_kind,
                    method: method_upper,
                    path: clean_path.to_owned(),
                });
            }
            if !has_obm1_header {
                return Err(SurfaceCapabilityError::MissingObm1ManagementHeader {
                    method: method_upper,
                    path: clean_path.to_owned(),
                });
            }
            return Ok(SurfaceRouteDecision::Allowed {
                required_scope: SurfaceScope::ConfirmExplicitUserGesture,
                requires_obm1_management: true,
            });
        }

        // 3. Classify remaining product routes into required `SurfaceScope`.
        let required_scope = if clean_path == "/api/pam/abort-somatic"
            || clean_path == "/api/v1/pam/abort-somatic"
            || clean_path == "/api/pam/somatic-abort"
            || clean_path == "/api/v1/pam/somatic-abort"
            || clean_path == "/api/vnext/hud/session/heartbeat"
            || clean_path == "/api/vnext/hud/session/doff"
        {
            // Allowed on Glasses, Desktop, Web, Cli for immediate M0 emergency wipe.
            if matches!(
                surface_kind,
                SurfaceKind::Glasses | SurfaceKind::Desktop | SurfaceKind::Web | SurfaceKind::Cli
            ) {
                return Ok(SurfaceRouteDecision::Allowed {
                    required_scope: SurfaceScope::SomaticEmergencyAbort,
                    requires_obm1_management: false,
                });
            }
            return Err(SurfaceCapabilityError::RouteForbiddenForSurface {
                surface: surface_kind,
                method: method_upper,
                path: clean_path.to_owned(),
            });
        } else if clean_path == "/api/vnext/hud/confirm/commit"
            && surface_kind == SurfaceKind::Glasses
        {
            // Glasses step-2 confirmation is gated inside the HUD handler by physical gesture + readback digest (`P10 §4.3`).
            SurfaceScope::ReadStatusAndQuery
        } else if clean_path.starts_with("/api/mesh")
            || clean_path.starts_with("/api/v1/mesh")
            || clean_path.starts_with("/api/vnext/mesh")
        {
            SurfaceScope::PersonalMeshPairAndSync
        } else if clean_path.contains("/confirm")
            || clean_path.contains("/publish")
            || clean_path.contains("/adopt")
            || clean_path.contains("/materialize")
            || clean_path.starts_with("/api/obt/transfer")
            || clean_path.starts_with("/api/wallet/send")
        {
            SurfaceScope::ConfirmExplicitUserGesture
        } else if clean_path.starts_with("/api/wallet")
            || clean_path.starts_with("/api/obt")
            || clean_path.starts_with("/api/vnext/obt")
        {
            if method_upper != "GET" {
                SurfaceScope::ConfirmExplicitUserGesture
            } else {
                SurfaceScope::WalletReadAndProvisionalView
            }
        } else if clean_path.contains("/prepare")
            || clean_path.contains("/proposals")
            || clean_path == "/api/vnext/kql/needs/compile"
        {
            SurfaceScope::StageProposalAndPreview
        } else if method_upper == "GET" || method_upper == "HEAD" {
            SurfaceScope::ReadStatusAndQuery
        } else if clean_path.starts_with("/api/capture")
            || clean_path.starts_with("/api/drafts")
            || clean_path.starts_with("/api/v1/capture")
            || clean_path.starts_with("/api/v1/drafts")
            || clean_path.starts_with("/api/vnext/extension/capture")
        {
            SurfaceScope::CaptureSourceAndDraft
        } else {
            // Default fail-closed to explicit confirmation scope for unknown mutating routes.
            SurfaceScope::ConfirmExplicitUserGesture
        };

        if !surface_kind.max_allowed_scopes().contains(&required_scope) {
            return Err(SurfaceCapabilityError::RouteForbiddenForSurface {
                surface: surface_kind,
                method: method_upper,
                path: clean_path.to_owned(),
            });
        }

        Ok(SurfaceRouteDecision::Allowed {
            required_scope,
            requires_obm1_management: false,
        })
    }

    pub fn authorize_token_for_route(
        &self,
        token: &str,
        method: &str,
        path: &str,
        has_obm1_header: bool,
        now_unix_s: u64,
    ) -> Result<(SurfaceCapabilityClaimsV1, SurfaceRouteDecision), SurfaceCapabilityError> {
        let claims = self.verify_token(token, now_unix_s)?;
        let decision =
            Self::authorize_http_route(claims.surface_kind, method, path, has_obm1_header)?;
        let SurfaceRouteDecision::Allowed { required_scope, .. } = decision;
        if required_scope != SurfaceScope::SomaticEmergencyAbort
            && !claims.allowed_scopes.contains(&required_scope)
        {
            return Err(SurfaceCapabilityError::RouteForbiddenForSurface {
                surface: claims.surface_kind,
                method: method.trim().to_ascii_uppercase(),
                path: path.to_owned(),
            });
        }
        Ok((claims, decision))
    }

    fn compute_mac(&self, claims_bytes: &[u8]) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new_keyed(&self.mac_key);
        hasher.update(SURFACE_MAC_DOMAIN);
        hasher.update(&(claims_bytes.len() as u64).to_be_bytes());
        hasher.update(claims_bytes);
        *hasher.finalize().as_bytes()
    }
}

fn constant_time_eq_32(left: &[u8; 32], right: &[u8; 32]) -> bool {
    left.iter()
        .zip(right.iter())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

fn expect_u64(value: &CanonicalValue) -> Result<u64, SurfaceCapabilityError> {
    match value {
        CanonicalValue::Unsigned(v) => Ok(*v),
        _ => Err(SurfaceCapabilityError::MalformedToken),
    }
}

fn expect_bytes32(value: &CanonicalValue) -> Result<[u8; 32], SurfaceCapabilityError> {
    let CanonicalValue::Bytes(bytes) = value else {
        return Err(SurfaceCapabilityError::MalformedToken);
    };
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| SurfaceCapabilityError::MalformedToken)
}
