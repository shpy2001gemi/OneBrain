//! Linux host-candidate gathering with a portable pure filtering core.

use std::net::IpAddr;
use std::sync::{Arc, RwLock};

use ku_net::vnext_relay_discovery::ReachabilityFuture;
use onebrain_protocol::{
    DirectCandidateKindV1, DirectCandidateV1, HostAddressV1, PrivateCandidateV1,
    PublicCandidateKindV1, PublicCandidateV1, ReachabilityEndpointV1, RelayCandidateV1,
};

use crate::vnext_reachability_manager::{
    CandidateGatherer, GatheredCandidates, NetworkEpoch, PrivateCandidateSet, ReachabilityError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InterfaceAddressObservation {
    address: IpAddr,
    up: bool,
    point_to_point: bool,
}

impl InterfaceAddressObservation {
    pub const fn new(address: IpAddr, up: bool, point_to_point: bool) -> Self {
        Self {
            address,
            up,
            point_to_point,
        }
    }
}

pub fn filter_host_addresses(observations: &[InterfaceAddressObservation]) -> Vec<IpAddr> {
    let mut output: Vec<_> = observations
        .iter()
        .filter(|observation| observation.up && !observation.point_to_point)
        .map(|observation| observation.address)
        .filter(|address| match address {
            IpAddr::V4(value) => {
                !value.is_unspecified()
                    && !value.is_loopback()
                    && !value.is_multicast()
                    && !value.is_broadcast()
            }
            IpAddr::V6(value) => {
                !value.is_unspecified()
                    && !value.is_loopback()
                    && !value.is_multicast()
                    && !value.is_unicast_link_local()
            }
        })
        .collect();
    output.sort_unstable();
    output.dedup();
    output.truncate(8);
    output
}

#[derive(Default)]
struct PublicInputs {
    observed: Vec<PublicCandidateV1>,
    provider: Vec<PublicCandidateV1>,
    relays: Vec<RelayCandidateV1>,
}

pub struct LinuxCandidateGatherer {
    listen_port: u16,
    now: Arc<dyn Fn() -> u64 + Send + Sync>,
    public: RwLock<PublicInputs>,
}

impl LinuxCandidateGatherer {
    pub fn new(
        listen_port: u16,
        now: Arc<dyn Fn() -> u64 + Send + Sync>,
    ) -> Result<Self, ReachabilityError> {
        if listen_port == 0 {
            return Err(ReachabilityError::InvalidCandidates);
        }
        Ok(Self {
            listen_port,
            now,
            public: RwLock::new(PublicInputs::default()),
        })
    }

    pub fn replace_public_inputs(
        &self,
        observed: Vec<PublicCandidateV1>,
        provider: Vec<PublicCandidateV1>,
        relays: Vec<RelayCandidateV1>,
    ) -> Result<(), ReachabilityError> {
        if observed
            .iter()
            .any(|value| value.kind != PublicCandidateKindV1::ServerReflexive)
            || provider
                .iter()
                .any(|value| value.kind != PublicCandidateKindV1::ProviderMapped)
            || observed.len().saturating_add(provider.len()) > 8
            || relays.len() > 6
        {
            return Err(ReachabilityError::InvalidCandidates);
        }
        let mut guard = self.public.write().map_err(|_| ReachabilityError::Io)?;
        *guard = PublicInputs {
            observed,
            provider,
            relays,
        };
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn platform_observations(&self) -> Result<Vec<InterfaceAddressObservation>, ReachabilityError> {
        use std::ptr;

        let mut head: *mut libc::ifaddrs = ptr::null_mut();
        // SAFETY: getifaddrs initializes `head` on success and freeifaddrs is
        // called exactly once after traversing the null-terminated list.
        if unsafe { libc::getifaddrs(&mut head) } != 0 {
            return Err(ReachabilityError::Io);
        }
        let mut output = Vec::new();
        let mut cursor = head;
        while !cursor.is_null() {
            // SAFETY: cursor belongs to the live getifaddrs list.
            let item = unsafe { &*cursor };
            if !item.ifa_addr.is_null() {
                let flags = item.ifa_flags as i32;
                let up = flags & libc::IFF_UP != 0;
                let point_to_point = flags & libc::IFF_POINTOPOINT != 0;
                // SAFETY: family selects the matching sockaddr layout.
                match unsafe { (*item.ifa_addr).sa_family as i32 } {
                    libc::AF_INET => {
                        let value = unsafe { &*(item.ifa_addr as *const libc::sockaddr_in) };
                        output.push(InterfaceAddressObservation::new(
                            IpAddr::V4(std::net::Ipv4Addr::from(u32::from_be(
                                value.sin_addr.s_addr,
                            ))),
                            up,
                            point_to_point,
                        ));
                    }
                    libc::AF_INET6 => {
                        let value = unsafe { &*(item.ifa_addr as *const libc::sockaddr_in6) };
                        output.push(InterfaceAddressObservation::new(
                            IpAddr::V6(std::net::Ipv6Addr::from(value.sin6_addr.s6_addr)),
                            up,
                            point_to_point,
                        ));
                    }
                    _ => {}
                }
            }
            cursor = item.ifa_next;
        }
        // SAFETY: head was produced by successful getifaddrs and is not used
        // after this call.
        unsafe { libc::freeifaddrs(head) };
        Ok(output)
    }

    #[cfg(not(target_os = "linux"))]
    fn platform_observations(&self) -> Result<Vec<InterfaceAddressObservation>, ReachabilityError> {
        Err(ReachabilityError::UnsupportedPlatform)
    }
}

impl CandidateGatherer for LinuxCandidateGatherer {
    fn gather(
        &self,
        epoch: NetworkEpoch,
    ) -> ReachabilityFuture<'_, Result<GatheredCandidates, ReachabilityError>> {
        Box::pin(async move {
            let addresses = filter_host_addresses(&self.platform_observations()?);
            let now = (self.now)();
            let mut private = Vec::with_capacity(addresses.len());
            let mut direct = Vec::with_capacity(addresses.len());
            for address in addresses {
                let host = match address {
                    IpAddr::V4(value) => HostAddressV1::Ipv4(value.octets()),
                    IpAddr::V6(value) => HostAddressV1::Ipv6(value.octets()),
                };
                let endpoint = ReachabilityEndpointV1 {
                    host,
                    port: self.listen_port,
                };
                let digest = blake3::hash(format!("{address}:{}", self.listen_port).as_bytes());
                let mut foundation = [0; 16];
                foundation.copy_from_slice(&digest.as_bytes()[..16]);
                private.push(PrivateCandidateV1 {
                    endpoint: endpoint.clone(),
                    priority: 100,
                    foundation,
                });
                direct.push(DirectCandidateV1 {
                    endpoint,
                    kind: DirectCandidateKindV1::Host,
                    priority: 100,
                    network_epoch: epoch.get(),
                    expires_at: now.saturating_add(300),
                });
            }
            let guard = self.public.read().map_err(|_| ReachabilityError::Io)?;
            let mut public = guard.observed.clone();
            public.extend(guard.provider.clone());
            let gathered = GatheredCandidates {
                private: PrivateCandidateSet::local(private, epoch)?,
                public,
                direct,
                relay: guard.relays.clone(),
                epoch,
                observed_at: now,
            };
            gathered.validate()?;
            Ok(gathered)
        })
    }
}
