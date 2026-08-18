//! Bounded transport ports for fetching and publishing canonical reachability records.

use std::sync::Arc;
use std::time::Instant;

use ku_core::foundation::NodeId;

use crate::vnext_relay_discovery::{
    ReachabilityFuture, RelayDiscoveryLimitation, RelayDiscoveryPolicy, SourceBudget,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReachabilityRecordQueryV1 {
    RelayDescriptor { relay: NodeId },
    PeerAdvertisement { target: NodeId },
}

pub trait ReachabilityRecordSource: Send + Sync {
    fn fetch<'a>(
        &'a self,
        query: &'a ReachabilityRecordQueryV1,
        budget: SourceBudget,
    ) -> ReachabilityFuture<'a, Result<Vec<Vec<u8>>, RelayDiscoveryLimitation>>;
}

pub trait ReachabilityRecordSink: Send + Sync {
    fn publish<'a>(
        &'a self,
        canonical_signed_record: &'a [u8],
        budget: SourceBudget,
    ) -> ReachabilityFuture<'a, Result<(), RelayDiscoveryLimitation>>;
}

pub struct ReachabilityAdvertisementResolver {
    sources: Vec<Arc<dyn ReachabilityRecordSource>>,
    policy: RelayDiscoveryPolicy,
}

impl ReachabilityAdvertisementResolver {
    pub fn new(
        sources: Vec<Arc<dyn ReachabilityRecordSource>>,
        policy: RelayDiscoveryPolicy,
    ) -> Self {
        Self { sources, policy }
    }

    pub fn fetch_records<'a>(
        &'a self,
        query: &'a ReachabilityRecordQueryV1,
        deadline: Instant,
    ) -> ReachabilityFuture<'a, Result<Vec<Vec<u8>>, RelayDiscoveryLimitation>> {
        Box::pin(async move {
            if self.sources.is_empty() || Instant::now() > deadline {
                return Err(RelayDiscoveryLimitation::NoBootstrapReachable);
            }
            let mut output = Vec::new();
            let mut total_bytes = 0_usize;
            let mut any_success = false;
            for source in &self.sources {
                if Instant::now() > deadline {
                    break;
                }
                let budget = SourceBudget {
                    max_records: self.policy.max_records_per_source,
                    max_bytes: self.policy.max_bytes_per_source,
                    max_signature_checks: self.policy.max_signature_checks,
                    deadline,
                };
                let Ok(records) = source.fetch(query, budget).await else {
                    continue;
                };
                any_success = true;
                if records.len() > self.policy.max_records_per_source
                    || output.len().saturating_add(records.len()) > self.policy.max_total_records
                {
                    return Err(RelayDiscoveryLimitation::RecordLimit);
                }
                for record in records {
                    total_bytes = total_bytes
                        .checked_add(record.len())
                        .ok_or(RelayDiscoveryLimitation::ByteLimit)?;
                    if total_bytes > self.policy.max_bytes_per_source {
                        return Err(RelayDiscoveryLimitation::ByteLimit);
                    }
                    output.push(record);
                }
            }
            if any_success {
                Ok(output)
            } else {
                Err(RelayDiscoveryLimitation::NoBootstrapReachable)
            }
        })
    }
}
