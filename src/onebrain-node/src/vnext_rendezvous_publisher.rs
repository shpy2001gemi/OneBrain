//! Bounded publication of already-signed reachability advertisements.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ku_net::vnext_reachability_resolver::ReachabilityRecordSink;
use ku_net::vnext_relay_discovery::{ReachabilityFuture, SourceBudget};
use onebrain_protocol::{
    encode_reachability_object, ReachabilityAdvertisementV1, ReachabilityObjectV1,
};

use crate::vnext_reachability_manager::{AdvertisementPublisher, ReachabilityError};

pub struct RendezvousAdvertisementPublisher {
    sinks: Vec<Arc<dyn ReachabilityRecordSink>>,
}

impl RendezvousAdvertisementPublisher {
    pub fn new(sinks: Vec<Arc<dyn ReachabilityRecordSink>>) -> Result<Self, ReachabilityError> {
        if sinks.is_empty() || sinks.len() > 8 {
            return Err(ReachabilityError::InvalidPolicy);
        }
        Ok(Self { sinks })
    }
}

impl AdvertisementPublisher for RendezvousAdvertisementPublisher {
    fn publish<'a>(
        &'a self,
        advertisement: &'a ReachabilityAdvertisementV1,
    ) -> ReachabilityFuture<'a, Result<(), ReachabilityError>> {
        Box::pin(async move {
            let bytes = encode_reachability_object(&ReachabilityObjectV1::Advertisement(
                advertisement.clone(),
            ))
            .map_err(|_| ReachabilityError::CorruptState)?;
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stored = false;
            for sink in &self.sinks {
                let budget = SourceBudget {
                    max_records: 1,
                    max_bytes: 32_768,
                    max_signature_checks: 1,
                    deadline,
                };
                if sink.publish(&bytes, budget).await.is_ok() {
                    stored = true;
                }
            }
            if stored {
                Ok(())
            } else {
                Err(ReachabilityError::Discovery(
                    ku_net::vnext_relay_discovery::RelayDiscoveryLimitation::NoBootstrapReachable,
                ))
            }
        })
    }
}
