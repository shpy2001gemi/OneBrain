//! Canonical Linux network snapshot hashing and monotonic epoch monitoring.

use std::sync::Mutex;

use crate::vnext_reachability_manager::{NetworkEpoch, ReachabilityError};

pub fn network_snapshot_digest<T: AsRef<[u8]>>(
    addresses: &[T],
    ipv4_routes: &[u8],
    ipv6_routes: &[u8],
) -> [u8; 32] {
    let mut canonical: Vec<Vec<u8>> = addresses
        .iter()
        .map(|address| address.as_ref().to_vec())
        .collect();
    canonical.sort();
    canonical.dedup();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain/network-epoch/v1\0");
    for value in canonical {
        hasher.update(&(value.len() as u64).to_be_bytes());
        hasher.update(&value);
    }
    hasher.update(&(ipv4_routes.len() as u64).to_be_bytes());
    hasher.update(ipv4_routes);
    hasher.update(&(ipv6_routes.len() as u64).to_be_bytes());
    hasher.update(ipv6_routes);
    *hasher.finalize().as_bytes()
}

pub trait NetworkSnapshotReader: Send + Sync {
    fn read_snapshot(&self) -> Result<NetworkSnapshotBytes, ReachabilityError>;
}

pub type NetworkSnapshotBytes = (Vec<Vec<u8>>, Vec<u8>, Vec<u8>);

pub struct LinuxProcNetworkSnapshotReader;

#[cfg(target_os = "linux")]
impl NetworkSnapshotReader for LinuxProcNetworkSnapshotReader {
    fn read_snapshot(&self) -> Result<NetworkSnapshotBytes, ReachabilityError> {
        let addresses = linux_addresses()?
            .into_iter()
            .map(|address| address.to_string().into_bytes())
            .collect();
        let v4 = std::fs::read("/proc/net/route").map_err(|_| ReachabilityError::Io)?;
        let v6 = std::fs::read("/proc/net/ipv6_route").map_err(|_| ReachabilityError::Io)?;
        Ok((addresses, v4, v6))
    }
}

#[cfg(not(target_os = "linux"))]
impl NetworkSnapshotReader for LinuxProcNetworkSnapshotReader {
    fn read_snapshot(&self) -> Result<NetworkSnapshotBytes, ReachabilityError> {
        Err(ReachabilityError::UnsupportedPlatform)
    }
}

pub struct LinuxNetworkEpochMonitor<R> {
    reader: R,
    state: Mutex<(NetworkEpoch, Option<[u8; 32]>)>,
}

impl<R: NetworkSnapshotReader> LinuxNetworkEpochMonitor<R> {
    pub fn new(reader: R, initial: NetworkEpoch) -> Self {
        Self {
            reader,
            state: Mutex::new((initial, None)),
        }
    }

    pub fn poll_once(&self) -> Result<(NetworkEpoch, bool), ReachabilityError> {
        let (addresses, v4, v6) = self.reader.read_snapshot()?;
        let digest = network_snapshot_digest(&addresses, &v4, &v6);
        let mut state = self.state.lock().map_err(|_| ReachabilityError::Io)?;
        let changed = state.1.is_some_and(|previous| previous != digest);
        if changed {
            state.0 = state.0.next()?;
        }
        state.1 = Some(digest);
        Ok((state.0, changed))
    }
}

#[cfg(target_os = "linux")]
fn linux_addresses() -> Result<Vec<std::net::IpAddr>, ReachabilityError> {
    use std::ptr;

    let mut head: *mut libc::ifaddrs = ptr::null_mut();
    // SAFETY: see getifaddrs(3); the returned list is freed below.
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return Err(ReachabilityError::Io);
    }
    let mut output = Vec::new();
    let mut cursor = head;
    while !cursor.is_null() {
        // SAFETY: cursor is an element of the live getifaddrs list.
        let item = unsafe { &*cursor };
        if !item.ifa_addr.is_null() && item.ifa_flags as i32 & libc::IFF_UP != 0 {
            // SAFETY: family selects the concrete sockaddr layout.
            match unsafe { (*item.ifa_addr).sa_family as i32 } {
                libc::AF_INET => {
                    let value = unsafe { &*(item.ifa_addr as *const libc::sockaddr_in) };
                    output.push(std::net::IpAddr::V4(std::net::Ipv4Addr::from(
                        u32::from_be(value.sin_addr.s_addr),
                    )));
                }
                libc::AF_INET6 => {
                    let value = unsafe { &*(item.ifa_addr as *const libc::sockaddr_in6) };
                    output.push(std::net::IpAddr::V6(std::net::Ipv6Addr::from(
                        value.sin6_addr.s6_addr,
                    )));
                }
                _ => {}
            }
        }
        cursor = item.ifa_next;
    }
    // SAFETY: head came from successful getifaddrs and is no longer used.
    unsafe { libc::freeifaddrs(head) };
    output.sort_unstable();
    output.dedup();
    Ok(output)
}
