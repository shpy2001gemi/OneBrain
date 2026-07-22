//! Action-scoped revocation freshness without a global liveness oracle.

use std::collections::{BTreeMap, BTreeSet};

use super::authority::FeedAuthorityDecision;
use super::capability_permit::{
    PermitAuthorityDecision, PermitExecutionScope, PermitValidationError, PermitValidator,
};
use super::content_id::{EventCid, PermitCid};
use super::feed::ValidatedFeedInception;
use super::identity::FeedId;
use super::key_state::KeyStateReducer;

pub const TERRESTRIAL_INTERACTIVE_PROFILE: &str = "TerrestrialInteractive/1";
pub const TASK_SPECIFIC_DTN_PROFILE: &str = "TaskSpecificDtn/1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum RevocationRisk {
    R0Observe = 0,
    R1LocalCognition = 1,
    R2NetworkExchange = 2,
    R3DelegatedExecution = 3,
    R4IrreversibleAuthority = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RevocationAction {
    ReadImmutableKnowledge,
    LocalReasoningOrProposal,
    RouteOrPublishExchange,
    NegotiatedDisclosure,
    RemoteCognition,
    ReversibleExternalEffect,
    DelegateAuthority,
    IrreversibleExternalEffect,
    SafetyCriticalEffect,
}

impl RevocationAction {
    pub const fn minimum_risk(self) -> RevocationRisk {
        match self {
            Self::ReadImmutableKnowledge => RevocationRisk::R0Observe,
            Self::LocalReasoningOrProposal => RevocationRisk::R1LocalCognition,
            Self::RouteOrPublishExchange => RevocationRisk::R2NetworkExchange,
            Self::NegotiatedDisclosure | Self::RemoteCognition | Self::ReversibleExternalEffect => {
                RevocationRisk::R3DelegatedExecution
            }
            Self::DelegateAuthority
            | Self::IrreversibleExternalEffect
            | Self::SafetyCriticalEffect => RevocationRisk::R4IrreversibleAuthority,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuthorityScope {
    Feed([u8; 32]),
    Permit([u8; 32]),
}

impl AuthorityScope {
    pub fn feed(feed: FeedId) -> Self {
        Self::Feed(*feed.as_bytes())
    }

    pub fn permit(permit: PermitCid) -> Self {
        Self::Permit(*permit.as_bytes())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservedAuthorityState {
    AuthorizedRelative { frontier: EventCid },
    RevokedRelative { frontier: EventCid },
    Expired,
    StaleOrUnresolved { frontier: Option<EventCid> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthorityFreshnessObservation {
    pub scope: AuthorityScope,
    pub state: ObservedAuthorityState,
    /// Local monotonic observation tick. It is never signed or replicated as
    /// a network-global timestamp.
    pub observed_at_local_tick: u64,
}

impl AuthorityFreshnessObservation {
    pub fn from_feed(
        key_state: &KeyStateReducer,
        feed: &ValidatedFeedInception,
        observed_at_local_tick: u64,
    ) -> Self {
        let state = match key_state.evaluate(feed) {
            FeedAuthorityDecision::AuthorizedRelative { frontier, .. } => {
                ObservedAuthorityState::AuthorizedRelative { frontier }
            }
            FeedAuthorityDecision::QuarantinedRevokedRelative { frontier, .. } => {
                ObservedAuthorityState::RevokedRelative { frontier }
            }
            FeedAuthorityDecision::StaleOrUnresolved { frontier, .. } => {
                ObservedAuthorityState::StaleOrUnresolved {
                    frontier: Some(frontier),
                }
            }
        };
        Self {
            scope: AuthorityScope::feed(feed.feed_id),
            state,
            observed_at_local_tick,
        }
    }

    pub fn from_permit(permits: &PermitValidator, permit: PermitCid, local_tick: u64) -> Self {
        let state = match permits.authority_at(permit, local_tick) {
            PermitAuthorityDecision::AuthorizedRelative {
                authority_frontier, ..
            } => ObservedAuthorityState::AuthorizedRelative {
                frontier: authority_frontier,
            },
            PermitAuthorityDecision::Expired => ObservedAuthorityState::Expired,
            PermitAuthorityDecision::NotYetActive | PermitAuthorityDecision::Unknown => {
                ObservedAuthorityState::StaleOrUnresolved { frontier: None }
            }
        };
        Self {
            scope: AuthorityScope::permit(permit),
            state,
            observed_at_local_tick: local_tick,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FreshnessWindows {
    pub r2_max_local_age: u64,
    pub r3_max_local_age: u64,
    pub r4_max_local_age: u64,
}

impl FreshnessWindows {
    pub fn validate(self) -> Result<Self, RevocationFreshnessError> {
        if self.r2_max_local_age == 0
            || self.r3_max_local_age == 0
            || self.r4_max_local_age == 0
            || self.r3_max_local_age > self.r2_max_local_age
            || self.r4_max_local_age > self.r3_max_local_age
        {
            Err(RevocationFreshnessError::InvalidProfile)
        } else {
            Ok(self)
        }
    }

    const fn for_risk(self, risk: RevocationRisk) -> Option<u64> {
        match risk {
            RevocationRisk::R0Observe | RevocationRisk::R1LocalCognition => None,
            RevocationRisk::R2NetworkExchange => Some(self.r2_max_local_age),
            RevocationRisk::R3DelegatedExecution => Some(self.r3_max_local_age),
            RevocationRisk::R4IrreversibleAuthority => Some(self.r4_max_local_age),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskSpecificDtnProfile {
    pub profile_id: [u8; 32],
    pub task_commitment: [u8; 32],
    pub action: RevocationAction,
    pub permit_id: PermitCid,
    pub permit_authority_frontier: EventCid,
    pub windows: FreshnessWindows,
    pub expires_at_local_tick: u64,
    pub profile_commitment: [u8; 32],
}

impl TaskSpecificDtnProfile {
    #[allow(clippy::too_many_arguments)]
    pub fn authorize(
        permits: &PermitValidator,
        profile_id: [u8; 32],
        task_commitment: [u8; 32],
        action: RevocationAction,
        permit_id: PermitCid,
        permit_scope: &PermitExecutionScope,
        windows: FreshnessWindows,
        expires_at_local_tick: u64,
        local_tick: u64,
    ) -> Result<Self, RevocationFreshnessError> {
        let windows = windows.validate()?;
        if profile_id == [0; 32]
            || task_commitment == [0; 32]
            || action.minimum_risk() < RevocationRisk::R2NetworkExchange
            || !permit_scope.input_commitments.contains(&task_commitment)
            || local_tick >= expires_at_local_tick
        {
            return Err(RevocationFreshnessError::InvalidProfile);
        }
        let permit = permits.authorize_scope(permit_id, local_tick, permit_scope)?;
        if expires_at_local_tick > permit.body.expires_at {
            return Err(RevocationFreshnessError::LifetimeExpansion);
        }
        let profile_commitment = dtn_profile_commitment(
            profile_id,
            task_commitment,
            action,
            permit_id,
            permit.authority_frontier,
            windows,
            expires_at_local_tick,
        );
        Ok(Self {
            profile_id,
            task_commitment,
            action,
            permit_id,
            permit_authority_frontier: permit.authority_frontier,
            windows,
            expires_at_local_tick,
            profile_commitment,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RevocationFreshnessProfile {
    TerrestrialInteractive,
    TaskSpecificDtn(TaskSpecificDtnProfile),
}

impl RevocationFreshnessProfile {
    pub const fn terrestrial_interactive() -> Self {
        Self::TerrestrialInteractive
    }

    pub const fn name(&self) -> &'static str {
        match self {
            Self::TerrestrialInteractive => TERRESTRIAL_INTERACTIVE_PROFILE,
            Self::TaskSpecificDtn(_) => TASK_SPECIFIC_DTN_PROFILE,
        }
    }

    fn windows(&self) -> FreshnessWindows {
        match self {
            // Caller ticks for this named profile are local monotonic seconds.
            Self::TerrestrialInteractive => FreshnessWindows {
                r2_max_local_age: 3_600,
                r3_max_local_age: 300,
                r4_max_local_age: 60,
            },
            Self::TaskSpecificDtn(profile) => profile.windows,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevocationCheck {
    pub action: RevocationAction,
    pub risk: RevocationRisk,
    pub task_commitment: Option<[u8; 32]>,
    pub required_scopes: Vec<AuthorityScope>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RevocationFreshnessDecision {
    ProceedWithoutFreshnessGate {
        risk: RevocationRisk,
    },
    AuthorizedRelativeFresh {
        profile: &'static str,
        frontiers: Vec<(AuthorityScope, EventCid)>,
        maximum_observed_age: u64,
    },
    RefreshRequired {
        scopes: Vec<AuthorityScope>,
    },
    DeniedRevokedRelative {
        scopes: Vec<AuthorityScope>,
    },
    DeniedExpired {
        scopes: Vec<AuthorityScope>,
    },
}

pub struct RevocationFreshnessEvaluator;

impl RevocationFreshnessEvaluator {
    pub fn evaluate(
        profile: &RevocationFreshnessProfile,
        check: &RevocationCheck,
        observations: &[AuthorityFreshnessObservation],
        local_tick: u64,
        permits: Option<&PermitValidator>,
    ) -> Result<RevocationFreshnessDecision, RevocationFreshnessError> {
        if check.risk < check.action.minimum_risk() {
            return Err(RevocationFreshnessError::RiskUnderstatement);
        }
        if matches!(
            check.risk,
            RevocationRisk::R0Observe | RevocationRisk::R1LocalCognition
        ) {
            return Ok(RevocationFreshnessDecision::ProceedWithoutFreshnessGate {
                risk: check.risk,
            });
        }
        if check.required_scopes.is_empty()
            || check
                .required_scopes
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                != check.required_scopes.len()
        {
            return Err(RevocationFreshnessError::ScopeMismatch);
        }
        if let RevocationFreshnessProfile::TaskSpecificDtn(dtn) = profile {
            if check.task_commitment != Some(dtn.task_commitment)
                || check.action != dtn.action
                || local_tick >= dtn.expires_at_local_tick
            {
                return Err(RevocationFreshnessError::DtnTaskScopeMismatch);
            }
            let permits = permits.ok_or(RevocationFreshnessError::DtnPermitUnavailable)?;
            if !matches!(
                permits.authority_at(dtn.permit_id, local_tick),
                PermitAuthorityDecision::AuthorizedRelative { .. }
            ) {
                return Err(RevocationFreshnessError::DtnPermitUnavailable);
            }
        }

        let by_scope = observations
            .iter()
            .map(|observation| (observation.scope, observation))
            .collect::<BTreeMap<_, _>>();
        if by_scope.len() != observations.len()
            || by_scope.keys().copied().collect::<BTreeSet<_>>()
                != check.required_scopes.iter().copied().collect()
        {
            return Err(RevocationFreshnessError::ScopeMismatch);
        }
        let max_age = profile
            .windows()
            .for_risk(check.risk)
            .expect("R2-R4 have a window");
        let mut revoked = Vec::new();
        let mut expired = Vec::new();
        let mut refresh = Vec::new();
        let mut frontiers = Vec::new();
        let mut maximum_observed_age = 0;
        for scope in &check.required_scopes {
            let observation = by_scope[scope];
            match observation.state {
                ObservedAuthorityState::RevokedRelative { .. } => revoked.push(*scope),
                ObservedAuthorityState::Expired => expired.push(*scope),
                ObservedAuthorityState::StaleOrUnresolved { .. } => refresh.push(*scope),
                ObservedAuthorityState::AuthorizedRelative { frontier } => {
                    let Some(age) = local_tick.checked_sub(observation.observed_at_local_tick)
                    else {
                        refresh.push(*scope);
                        continue;
                    };
                    if age > max_age {
                        refresh.push(*scope);
                    } else {
                        maximum_observed_age = maximum_observed_age.max(age);
                        frontiers.push((*scope, frontier));
                    }
                }
            }
        }
        if !revoked.is_empty() {
            Ok(RevocationFreshnessDecision::DeniedRevokedRelative { scopes: revoked })
        } else if !expired.is_empty() {
            Ok(RevocationFreshnessDecision::DeniedExpired { scopes: expired })
        } else if !refresh.is_empty() {
            Ok(RevocationFreshnessDecision::RefreshRequired { scopes: refresh })
        } else {
            Ok(RevocationFreshnessDecision::AuthorizedRelativeFresh {
                profile: profile.name(),
                frontiers,
                maximum_observed_age,
            })
        }
    }
}

fn dtn_profile_commitment(
    profile_id: [u8; 32],
    task_commitment: [u8; 32],
    action: RevocationAction,
    permit_id: PermitCid,
    frontier: EventCid,
    windows: FreshnessWindows,
    expires_at: u64,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:task-specific-dtn-revocation-profile:1\0");
    hasher.update(&profile_id);
    hasher.update(&task_commitment);
    hasher.update(&[action as u8]);
    hasher.update(permit_id.as_bytes());
    hasher.update(frontier.as_bytes());
    hasher.update(&windows.r2_max_local_age.to_be_bytes());
    hasher.update(&windows.r3_max_local_age.to_be_bytes());
    hasher.update(&windows.r4_max_local_age.to_be_bytes());
    hasher.update(&expires_at.to_be_bytes());
    *hasher.finalize().as_bytes()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RevocationFreshnessError {
    Permit(PermitValidationError),
    InvalidProfile,
    LifetimeExpansion,
    RiskUnderstatement,
    ScopeMismatch,
    DtnTaskScopeMismatch,
    DtnPermitUnavailable,
}

impl From<PermitValidationError> for RevocationFreshnessError {
    fn from(error: PermitValidationError) -> Self {
        Self::Permit(error)
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::*;
    use crate::foundation::{
        authenticate_delegation_permit, decode_feed_inception, ActorId, Budget, ConceptCcid,
        DelegationGrant, DelegationPermitBody, DeviceId, FeedInception, KeyStateApplyOutcome,
        NamespaceCommitment, ObjectCid, PermitApplyOutcome, RetentionRule, ScopedDelegation,
        SignedDelegationPermit, SignedFeedInception,
    };

    fn scope(byte: u8) -> AuthorityScope {
        AuthorityScope::Feed([byte; 32])
    }

    fn observation(
        scope: AuthorityScope,
        state: ObservedAuthorityState,
        tick: u64,
    ) -> AuthorityFreshnessObservation {
        AuthorityFreshnessObservation {
            scope,
            state,
            observed_at_local_tick: tick,
        }
    }

    fn authorized(frontier: u8) -> ObservedAuthorityState {
        ObservedAuthorityState::AuthorizedRelative {
            frontier: EventCid::from_bytes([frontier; 32]),
        }
    }

    fn dtn_permit() -> (PermitValidator, PermitCid, PermitExecutionScope) {
        let issuer = ActorId::from_bytes([1; 32]);
        let executor = ActorId::from_bytes([2; 32]);
        let key = SigningKey::from_bytes(&[3; 32]);
        let delegation_ref = EventCid::from_bytes([4; 32]);
        let mut inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"dtn-profile-test", [5; 32]).unwrap(),
            0,
            DeviceId::from_bytes([6; 32]),
        );
        inception.actor_delegation_ref = Some(delegation_ref.into_bytes());
        let signed: SignedFeedInception = inception.sign(&key).unwrap();
        let feed = decode_feed_inception(&signed.encode().unwrap()).unwrap();
        let mut key_state = KeyStateReducer::new(EventCid::from_bytes([7; 32]));
        assert_eq!(
            key_state.accept_root(ScopedDelegation {
                grant: DelegationGrant {
                    actor: issuer,
                    device: feed.signed.inception.owner_device,
                    delegation_ref,
                    namespace_commitment: None,
                    first_generation: 0,
                    last_generation: 0,
                    proof: EventCid::from_bytes([8; 32]),
                },
                parent_delegation_ref: None,
            }),
            KeyStateApplyOutcome::Accepted
        );
        let body = DelegationPermitBody {
            issuer,
            executor,
            capability_definition: ObjectCid::from_bytes([1; 32]),
            input_commitments: vec![[2; 32]],
            allowed_effect_classes: vec![ConceptCcid::from_bytes([3; 16])],
            purpose: ConceptCcid::from_bytes([4; 16]),
            budget: Budget::new(10, 10_000, 100, 4).unwrap(),
            retention: RetentionRule::DeleteAfterTask,
            onward_delegation: false,
            parent_permit: None,
            not_before: 10,
            expires_at: 200_000,
            nonce: [9; 32],
        };
        let bytes = SignedDelegationPermit::sign(body, &feed, &key)
            .unwrap()
            .encode()
            .unwrap();
        let authenticated = authenticate_delegation_permit(&bytes, &feed, &key_state).unwrap();
        let permit_id = authenticated.permit_id;
        let mut permits = PermitValidator::default();
        assert_eq!(
            permits.submit(authenticated, 20).unwrap(),
            PermitApplyOutcome::Accepted(permit_id)
        );
        let permit_scope = PermitExecutionScope {
            capability_definition: ObjectCid::from_bytes([1; 32]),
            input_commitments: vec![[2; 32]],
            requested_effect_classes: vec![ConceptCcid::from_bytes([3; 16])],
            purpose: ConceptCcid::from_bytes([4; 16]),
            budget: Budget::new(1, 1_000, 10, 2).unwrap(),
            retention: RetentionRule::DeleteAfterTask,
        };
        (permits, permit_id, permit_scope)
    }

    #[test]
    fn r0_and_r1_never_gate_local_read_or_reasoning() {
        for (action, risk) in [
            (
                RevocationAction::ReadImmutableKnowledge,
                RevocationRisk::R0Observe,
            ),
            (
                RevocationAction::LocalReasoningOrProposal,
                RevocationRisk::R1LocalCognition,
            ),
        ] {
            assert_eq!(
                RevocationFreshnessEvaluator::evaluate(
                    &RevocationFreshnessProfile::terrestrial_interactive(),
                    &RevocationCheck {
                        action,
                        risk,
                        task_commitment: None,
                        required_scopes: vec![],
                    },
                    &[],
                    u64::MAX,
                    None,
                )
                .unwrap(),
                RevocationFreshnessDecision::ProceedWithoutFreshnessGate { risk }
            );
        }
    }

    #[test]
    fn terrestrial_windows_are_risk_specific_and_scope_exact() {
        let profile = RevocationFreshnessProfile::terrestrial_interactive();
        let required = scope(1);
        for (risk, age, expected_fresh) in [
            (RevocationRisk::R2NetworkExchange, 3_600, true),
            (RevocationRisk::R2NetworkExchange, 3_601, false),
            (RevocationRisk::R3DelegatedExecution, 300, true),
            (RevocationRisk::R3DelegatedExecution, 301, false),
            (RevocationRisk::R4IrreversibleAuthority, 60, true),
            (RevocationRisk::R4IrreversibleAuthority, 61, false),
        ] {
            let action = match risk {
                RevocationRisk::R2NetworkExchange => RevocationAction::RouteOrPublishExchange,
                RevocationRisk::R3DelegatedExecution => RevocationAction::RemoteCognition,
                RevocationRisk::R4IrreversibleAuthority => RevocationAction::DelegateAuthority,
                _ => unreachable!(),
            };
            let decision = RevocationFreshnessEvaluator::evaluate(
                &profile,
                &RevocationCheck {
                    action,
                    risk,
                    task_commitment: None,
                    required_scopes: vec![required],
                },
                &[observation(required, authorized(9), 10_000 - age)],
                10_000,
                None,
            )
            .unwrap();
            assert_eq!(
                matches!(
                    decision,
                    RevocationFreshnessDecision::AuthorizedRelativeFresh { .. }
                ),
                expected_fresh
            );
        }
        assert_eq!(
            RevocationFreshnessEvaluator::evaluate(
                &profile,
                &RevocationCheck {
                    action: RevocationAction::RouteOrPublishExchange,
                    risk: RevocationRisk::R2NetworkExchange,
                    task_commitment: None,
                    required_scopes: vec![required],
                },
                &[observation(scope(2), authorized(9), 9_999)],
                10_000,
                None,
            ),
            Err(RevocationFreshnessError::ScopeMismatch)
        );
    }

    #[test]
    fn revoked_expired_and_unknown_remain_relative_not_global_liveness() {
        let profile = RevocationFreshnessProfile::terrestrial_interactive();
        let check = RevocationCheck {
            action: RevocationAction::RemoteCognition,
            risk: RevocationRisk::R3DelegatedExecution,
            task_commitment: None,
            required_scopes: vec![scope(1)],
        };
        assert!(matches!(
            RevocationFreshnessEvaluator::evaluate(
                &profile,
                &check,
                &[observation(
                    scope(1),
                    ObservedAuthorityState::RevokedRelative {
                        frontier: EventCid::from_bytes([4; 32])
                    },
                    100
                )],
                101,
                None
            )
            .unwrap(),
            RevocationFreshnessDecision::DeniedRevokedRelative { .. }
        ));
        assert!(matches!(
            RevocationFreshnessEvaluator::evaluate(
                &profile,
                &check,
                &[observation(
                    scope(1),
                    ObservedAuthorityState::StaleOrUnresolved { frontier: None },
                    100
                )],
                101,
                None
            )
            .unwrap(),
            RevocationFreshnessDecision::RefreshRequired { .. }
        ));
    }

    #[test]
    fn risk_understatement_cannot_turn_remote_or_irreversible_action_into_local() {
        assert_eq!(
            RevocationFreshnessEvaluator::evaluate(
                &RevocationFreshnessProfile::terrestrial_interactive(),
                &RevocationCheck {
                    action: RevocationAction::SafetyCriticalEffect,
                    risk: RevocationRisk::R1LocalCognition,
                    task_commitment: None,
                    required_scopes: vec![],
                },
                &[],
                0,
                None,
            ),
            Err(RevocationFreshnessError::RiskUnderstatement)
        );
    }

    #[test]
    fn dtn_profile_is_permit_and_task_specific_not_an_earth_global_override() {
        let windows = FreshnessWindows {
            r2_max_local_age: 86_400,
            r3_max_local_age: 43_200,
            r4_max_local_age: 3_600,
        };
        assert!(windows.validate().is_ok());
        assert_eq!(
            FreshnessWindows {
                r2_max_local_age: 60,
                r3_max_local_age: 600,
                r4_max_local_age: 30,
            }
            .validate(),
            Err(RevocationFreshnessError::InvalidProfile)
        );
        let (permits, permit_id, permit_scope) = dtn_permit();
        let dtn = TaskSpecificDtnProfile::authorize(
            &permits,
            [10; 32],
            [2; 32],
            RevocationAction::RemoteCognition,
            permit_id,
            &permit_scope,
            windows,
            100_000,
            20,
        )
        .unwrap();
        let profile = RevocationFreshnessProfile::TaskSpecificDtn(dtn);
        let permit_scope_id = AuthorityScope::permit(permit_id);
        let observation = observation(permit_scope_id, authorized(7), 100);
        let check = RevocationCheck {
            action: RevocationAction::RemoteCognition,
            risk: RevocationRisk::R3DelegatedExecution,
            task_commitment: Some([2; 32]),
            required_scopes: vec![permit_scope_id],
        };
        assert!(matches!(
            RevocationFreshnessEvaluator::evaluate(
                &profile,
                &check,
                &[observation],
                40_000,
                Some(&permits)
            )
            .unwrap(),
            RevocationFreshnessDecision::AuthorizedRelativeFresh {
                profile: TASK_SPECIFIC_DTN_PROFILE,
                ..
            }
        ));
        assert!(matches!(
            RevocationFreshnessEvaluator::evaluate(
                &RevocationFreshnessProfile::terrestrial_interactive(),
                &check,
                &[observation],
                40_000,
                Some(&permits)
            )
            .unwrap(),
            RevocationFreshnessDecision::RefreshRequired { .. }
        ));
        let mut wrong_task = check.clone();
        wrong_task.task_commitment = Some([99; 32]);
        assert_eq!(
            RevocationFreshnessEvaluator::evaluate(
                &profile,
                &wrong_task,
                &[observation],
                40_000,
                Some(&permits)
            ),
            Err(RevocationFreshnessError::DtnTaskScopeMismatch)
        );
    }
}
