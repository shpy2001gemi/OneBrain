//! Bounded datagram adaptation for an opaque authenticated relay carrier.

use std::collections::VecDeque;
use std::fmt;
use std::io::{self, IoSliceMut};
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use quinn::udp::{EcnCodepoint, RecvMeta, Transmit};
use quinn::{AsyncUdpSocket, UdpPoller};
use tokio::sync::mpsc;

use ku_core::foundation::NodeId;
use onebrain_protocol::RelayTransportV1;

use crate::vnext_reachability_crypto::{
    ValidatedPublicDialEndpoint, ValidatedPublicDialTransportV1, ValidatedRelayDescriptor,
};

pub const RELAY_SOCKET_FRAME_LIMIT: usize = 64;
pub const RELAY_SOCKET_BYTE_LIMIT: usize = 1024 * 1024;
pub const RELAY_GLOBAL_FRAME_LIMIT: usize = 1_024;
pub const RELAY_GLOBAL_BYTE_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedPublicRelayDialEndpoint {
    descriptor: ValidatedRelayDescriptor,
    endpoint_index: usize,
    endpoint: ValidatedPublicDialEndpoint,
    transport: RelayTransportV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedAlternateRelayDialEndpoint {
    descriptor: ValidatedRelayDescriptor,
    public_endpoint_index: usize,
    alternate_socket: SocketAddr,
    transport: RelayTransportV1,
    spki_observation_digest: [u8; 32],
    expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidatedRelayDialRoute {
    Public(ValidatedPublicRelayDialEndpoint),
    Alternate(ValidatedAlternateRelayDialEndpoint),
}

impl ValidatedRelayDialRoute {
    pub fn public(
        descriptor: &ValidatedRelayDescriptor,
        endpoint: ValidatedPublicDialEndpoint,
    ) -> Result<Self, RelayRouteError> {
        if endpoint.source_digest() != *descriptor.digest() {
            return Err(RelayRouteError::DescriptorMismatch);
        }
        let index = endpoint.endpoint_index();
        let signed = descriptor
            .canonical()
            .endpoints
            .get(index)
            .ok_or(RelayRouteError::EndpointMismatch)?;
        let transport = match endpoint.transport() {
            ValidatedPublicDialTransportV1::RelayQuicUdp => RelayTransportV1::QuicUdp,
            ValidatedPublicDialTransportV1::RelayTlsTcp443 => RelayTransportV1::TlsTcp443,
            _ => return Err(RelayRouteError::TransportMismatch),
        };
        if signed.transport != transport
            || signed.port != endpoint.port()
            || &signed.host != endpoint.signed_host()
        {
            return Err(RelayRouteError::EndpointMismatch);
        }
        Ok(Self::Public(ValidatedPublicRelayDialEndpoint {
            descriptor: descriptor.clone(),
            endpoint_index: index,
            endpoint,
            transport,
        }))
    }

    pub fn relay_node_id(&self) -> NodeId {
        self.descriptor().canonical().relay_node_id
    }

    pub fn relay_public_key(&self) -> [u8; 32] {
        self.descriptor().canonical().relay_public_key
    }

    pub fn descriptor_digest(&self) -> [u8; 32] {
        *self.descriptor().digest()
    }

    pub fn transport(&self) -> RelayTransportV1 {
        match self {
            Self::Public(value) => value.transport,
            Self::Alternate(value) => value.transport,
        }
    }

    pub fn dial_addresses(&self) -> Vec<SocketAddr> {
        match self {
            Self::Public(value) => value.endpoint.dial_addresses().to_vec(),
            Self::Alternate(value) => vec![value.alternate_socket],
        }
    }

    pub fn expires_at(&self) -> u64 {
        match self {
            Self::Public(value) => value.endpoint.expires_at(),
            Self::Alternate(value) => value.expires_at,
        }
    }

    pub fn endpoint_index(&self) -> usize {
        match self {
            Self::Public(value) => value.endpoint_index,
            Self::Alternate(value) => value.public_endpoint_index,
        }
    }

    pub fn alternate_spki_observation_digest(&self) -> Option<[u8; 32]> {
        match self {
            Self::Public(_) => None,
            Self::Alternate(value) => Some(value.spki_observation_digest),
        }
    }

    fn descriptor(&self) -> &ValidatedRelayDescriptor {
        match self {
            Self::Public(value) => &value.descriptor,
            Self::Alternate(value) => &value.descriptor,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedRelayDialSet {
    primary: ValidatedRelayDialRoute,
    tcp_fallback: Option<ValidatedRelayDialRoute>,
}

impl ValidatedRelayDialSet {
    pub fn from_admitted_descriptor(
        primary: ValidatedRelayDialRoute,
        tcp_fallback: Option<ValidatedRelayDialRoute>,
    ) -> Result<Self, RelayRouteError> {
        if primary.transport() != RelayTransportV1::QuicUdp {
            return Err(RelayRouteError::TransportMismatch);
        }
        if let Some(fallback) = &tcp_fallback {
            if fallback.transport() != RelayTransportV1::TlsTcp443
                || fallback.descriptor_digest() != primary.descriptor_digest()
                || fallback.relay_node_id() != primary.relay_node_id()
                || fallback.relay_public_key() != primary.relay_public_key()
            {
                return Err(RelayRouteError::DescriptorMismatch);
            }
        }
        Ok(Self {
            primary,
            tcp_fallback,
        })
    }

    pub fn primary(&self) -> &ValidatedRelayDialRoute {
        &self.primary
    }

    pub fn tcp_fallback(&self) -> Option<&ValidatedRelayDialRoute> {
        self.tcp_fallback.as_ref()
    }

    pub fn select_for_udp_datagram_limit(
        &self,
        max_datagram_size: Option<usize>,
        encoded_header_len: usize,
    ) -> Result<&ValidatedRelayDialRoute, RelayRouteError> {
        let usable = max_datagram_size.and_then(|size| size.checked_sub(encoded_header_len));
        if usable.is_some_and(|value| value > 0) {
            Ok(&self.primary)
        } else {
            self.tcp_fallback
                .as_ref()
                .ok_or(RelayRouteError::NoUsableTransport)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayRouteError {
    DescriptorMismatch,
    EndpointMismatch,
    TransportMismatch,
    NoUsableTransport,
}

#[derive(Clone, Debug)]
pub struct RelaySocketGlobalBudget {
    frames: Arc<AtomicUsize>,
    bytes: Arc<AtomicUsize>,
    frame_limit: usize,
    byte_limit: usize,
}

impl RelaySocketGlobalBudget {
    pub fn standard() -> Self {
        Self::with_limits(RELAY_GLOBAL_FRAME_LIMIT, RELAY_GLOBAL_BYTE_LIMIT)
    }

    pub fn with_limits(frame_limit: usize, byte_limit: usize) -> Self {
        Self {
            frames: Arc::new(AtomicUsize::new(0)),
            bytes: Arc::new(AtomicUsize::new(0)),
            frame_limit,
            byte_limit,
        }
    }

    fn reserve(&self, bytes: usize) -> io::Result<GlobalLease> {
        reserve_atomic(&self.frames, 1, self.frame_limit)?;
        if let Err(error) = reserve_atomic(&self.bytes, bytes, self.byte_limit) {
            self.frames.fetch_sub(1, Ordering::AcqRel);
            return Err(error);
        }
        Ok(GlobalLease {
            budget: self.clone(),
            bytes,
        })
    }
}

#[derive(Debug)]
struct GlobalLease {
    budget: RelaySocketGlobalBudget,
    bytes: usize,
}

impl Drop for GlobalLease {
    fn drop(&mut self) {
        self.budget.frames.fetch_sub(1, Ordering::AcqRel);
        self.budget.bytes.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

fn reserve_atomic(value: &AtomicUsize, amount: usize, limit: usize) -> io::Result<()> {
    let mut current = value.load(Ordering::Acquire);
    loop {
        let next = current.checked_add(amount).ok_or_else(would_block)?;
        if next > limit {
            return Err(would_block());
        }
        match value.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return Ok(()),
            Err(observed) => current = observed,
        }
    }
}

fn would_block() -> io::Error {
    io::Error::from(io::ErrorKind::WouldBlock)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedRelayTransmit {
    pub destination: SocketAddr,
    pub ecn: Option<EcnCodepoint>,
    pub contents: Vec<u8>,
    pub segment_size: Option<usize>,
    pub src_ip: Option<IpAddr>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayInboundDatagram {
    pub source: SocketAddr,
    pub destination_ip: Option<IpAddr>,
    pub ecn: Option<EcnCodepoint>,
    pub contents: Vec<u8>,
}

#[derive(Debug)]
struct QueuedTransmit {
    value: OwnedRelayTransmit,
    _lease: GlobalLease,
}

#[derive(Debug)]
struct QueuedInbound {
    value: RelayInboundDatagram,
    _lease: GlobalLease,
}

#[derive(Debug)]
struct Shared {
    send_tx: mpsc::Sender<QueuedTransmit>,
    send_bytes: AtomicUsize,
    recv_queue: Mutex<VecDeque<QueuedInbound>>,
    recv_bytes: AtomicUsize,
    write_waiters: Mutex<Vec<Waker>>,
    recv_waker: Mutex<Option<Waker>>,
    terminal: Mutex<Option<io::ErrorKind>>,
    closed: AtomicBool,
    frame_limit: usize,
    byte_limit: usize,
    global: RelaySocketGlobalBudget,
}

pub struct RelayDatagramSocket {
    local_addr: SocketAddr,
    shared: Arc<Shared>,
}

impl fmt::Debug for RelayDatagramSocket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayDatagramSocket")
            .field("local_addr", &self.local_addr)
            .finish_non_exhaustive()
    }
}

pub struct RelaySocketDriver {
    shared: Arc<Shared>,
    send_rx: mpsc::Receiver<QueuedTransmit>,
}

impl RelayDatagramSocket {
    pub fn pair(
        local_addr: SocketAddr,
        global: RelaySocketGlobalBudget,
    ) -> (Arc<Self>, RelaySocketDriver) {
        Self::pair_with_limits(
            local_addr,
            RELAY_SOCKET_FRAME_LIMIT,
            RELAY_SOCKET_BYTE_LIMIT,
            global,
        )
    }

    pub fn pair_with_limits(
        local_addr: SocketAddr,
        frame_limit: usize,
        byte_limit: usize,
        global: RelaySocketGlobalBudget,
    ) -> (Arc<Self>, RelaySocketDriver) {
        let (send_tx, send_rx) = mpsc::channel(frame_limit);
        let shared = Arc::new(Shared {
            send_tx,
            send_bytes: AtomicUsize::new(0),
            recv_queue: Mutex::new(VecDeque::new()),
            recv_bytes: AtomicUsize::new(0),
            write_waiters: Mutex::new(Vec::new()),
            recv_waker: Mutex::new(None),
            terminal: Mutex::new(None),
            closed: AtomicBool::new(false),
            frame_limit,
            byte_limit,
            global,
        });
        (
            Arc::new(Self {
                local_addr,
                shared: shared.clone(),
            }),
            RelaySocketDriver { shared, send_rx },
        )
    }

    fn terminal_error(&self) -> Option<io::Error> {
        self.shared
            .terminal
            .lock()
            .ok()
            .and_then(|guard| *guard)
            .map(io::Error::from)
    }
}

impl AsyncUdpSocket for RelayDatagramSocket {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        Box::pin(RelayWritePoller {
            shared: self.shared.clone(),
        })
    }

    fn try_send(&self, transmit: &Transmit<'_>) -> io::Result<()> {
        if let Some(error) = self.terminal_error() {
            return Err(error);
        }
        if transmit.contents.is_empty() || transmit.contents.len() > self.shared.byte_limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "relay frame limit",
            ));
        }
        reserve_atomic(
            &self.shared.send_bytes,
            transmit.contents.len(),
            self.shared.byte_limit,
        )?;
        let lease = match self.shared.global.reserve(transmit.contents.len()) {
            Ok(lease) => lease,
            Err(error) => {
                self.shared
                    .send_bytes
                    .fetch_sub(transmit.contents.len(), Ordering::AcqRel);
                return Err(error);
            }
        };
        let queued = QueuedTransmit {
            value: OwnedRelayTransmit {
                destination: transmit.destination,
                ecn: transmit.ecn,
                contents: transmit.contents.to_vec(),
                segment_size: transmit.segment_size,
                src_ip: transmit.src_ip,
            },
            _lease: lease,
        };
        match self.shared.send_tx.try_send(queued) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.shared
                    .send_bytes
                    .fetch_sub(transmit.contents.len(), Ordering::AcqRel);
                match error {
                    mpsc::error::TrySendError::Full(_) => Err(would_block()),
                    mpsc::error::TrySendError::Closed(_) => {
                        Err(io::Error::from(io::ErrorKind::BrokenPipe))
                    }
                }
            }
        }
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        if let Some(error) = self.terminal_error() {
            return Poll::Ready(Err(error));
        }
        let capacity = bufs.len().min(meta.len());
        let mut queue = match self.shared.recv_queue.lock() {
            Ok(queue) => queue,
            Err(_) => return Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe))),
        };
        let mut count = 0;
        while count < capacity {
            let Some(queued) = queue.pop_front() else {
                break;
            };
            if queued.value.contents.len() > bufs[count].len() {
                self.shared
                    .recv_bytes
                    .fetch_sub(queued.value.contents.len(), Ordering::AcqRel);
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "receive buffer too small",
                )));
            }
            let len = queued.value.contents.len();
            bufs[count][..len].copy_from_slice(&queued.value.contents);
            meta[count] = RecvMeta {
                addr: queued.value.source,
                len,
                stride: len,
                ecn: queued.value.ecn,
                dst_ip: queued.value.destination_ip,
            };
            self.shared.recv_bytes.fetch_sub(len, Ordering::AcqRel);
            count += 1;
        }
        drop(queue);
        if count > 0 {
            Poll::Ready(Ok(count))
        } else {
            if let Ok(mut waker) = self.shared.recv_waker.lock() {
                *waker = Some(cx.waker().clone());
            }
            Poll::Pending
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local_addr)
    }

    fn may_fragment(&self) -> bool {
        false
    }
}

#[derive(Debug)]
struct RelayWritePoller {
    shared: Arc<Shared>,
}

impl UdpPoller for RelayWritePoller {
    fn poll_writable(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let terminal = self.shared.terminal.lock().ok().and_then(|guard| *guard);
        if let Some(kind) = terminal {
            return Poll::Ready(Err(io::Error::from(kind)));
        }
        if self.shared.send_tx.capacity() > 0
            && self.shared.send_bytes.load(Ordering::Acquire) < self.shared.byte_limit
        {
            return Poll::Ready(Ok(()));
        }
        if let Ok(mut waiters) = self.shared.write_waiters.lock() {
            waiters.push(cx.waker().clone());
        }
        Poll::Pending
    }
}

impl RelaySocketDriver {
    pub async fn recv_outbound(&mut self) -> Option<OwnedRelayTransmit> {
        let queued = self.send_rx.recv().await?;
        self.shared
            .send_bytes
            .fetch_sub(queued.value.contents.len(), Ordering::AcqRel);
        wake_all(&self.shared.write_waiters);
        Some(queued.value)
    }

    pub fn push_inbound(&self, value: RelayInboundDatagram) -> io::Result<()> {
        if value.contents.is_empty() || value.contents.len() > self.shared.byte_limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "relay frame limit",
            ));
        }
        let mut queue = self
            .shared
            .recv_queue
            .lock()
            .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
        if queue.len() >= self.shared.frame_limit {
            return Err(would_block());
        }
        reserve_atomic(
            &self.shared.recv_bytes,
            value.contents.len(),
            self.shared.byte_limit,
        )?;
        let lease = match self.shared.global.reserve(value.contents.len()) {
            Ok(lease) => lease,
            Err(error) => {
                self.shared
                    .recv_bytes
                    .fetch_sub(value.contents.len(), Ordering::AcqRel);
                return Err(error);
            }
        };
        queue.push_back(QueuedInbound {
            value,
            _lease: lease,
        });
        drop(queue);
        if let Ok(mut waker) = self.shared.recv_waker.lock() {
            if let Some(waker) = waker.take() {
                waker.wake();
            }
        }
        Ok(())
    }

    pub fn fail(&self, kind: io::ErrorKind) {
        if let Ok(mut terminal) = self.shared.terminal.lock() {
            *terminal = Some(kind);
        }
        self.shared.closed.store(true, Ordering::Release);
        wake_all(&self.shared.write_waiters);
        if let Ok(mut waker) = self.shared.recv_waker.lock() {
            if let Some(waker) = waker.take() {
                waker.wake();
            }
        }
    }
}

impl Drop for RelaySocketDriver {
    fn drop(&mut self) {
        self.fail(io::ErrorKind::BrokenPipe);
    }
}

fn wake_all(waiters: &Mutex<Vec<Waker>>) {
    if let Ok(mut waiters) = waiters.lock() {
        for waker in waiters.drain(..) {
            waker.wake();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn vnext_relay_tunnel_copies_bounds_drains_and_recovers() {
        let global = RelaySocketGlobalBudget::with_limits(2, 16);
        let (socket, mut driver) =
            RelayDatagramSocket::pair_with_limits("127.0.0.1:40000".parse().unwrap(), 1, 8, global);
        let destination = "127.0.0.1:40001".parse().unwrap();
        socket
            .try_send(&Transmit {
                destination,
                ecn: None,
                contents: b"opaque",
                segment_size: None,
                src_ip: None,
            })
            .unwrap();
        assert_eq!(
            socket
                .try_send(&Transmit {
                    destination,
                    ecn: None,
                    contents: b"again",
                    segment_size: None,
                    src_ip: None,
                })
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(driver.recv_outbound().await.unwrap().contents, b"opaque");
        socket
            .try_send(&Transmit {
                destination,
                ecn: None,
                contents: b"again",
                segment_size: None,
                src_ip: None,
            })
            .unwrap();
    }

    #[test]
    fn vnext_relay_tunnel_worker_failure_is_terminal() {
        let (socket, driver) = RelayDatagramSocket::pair(
            "127.0.0.1:41000".parse().unwrap(),
            RelaySocketGlobalBudget::standard(),
        );
        driver.fail(io::ErrorKind::ConnectionReset);
        let error = socket
            .try_send(&Transmit {
                destination: "127.0.0.1:41001".parse().unwrap(),
                ecn: None,
                contents: b"x",
                segment_size: None,
                src_ip: None,
            })
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::ConnectionReset);
    }
}
