//! External P5 signer boundary and durable anti-replay cursor.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey};
use ku_net::vnext_reachability_crypto::{ReachabilityCryptoError, ReachabilityIdentitySigner};
use ku_net::vnext_session::SessionIdentitySigner;

pub const CHILD_RECEIPT_DOMAIN_V2: &[u8] = b"onebrain/p5/child-receipt/v2";
pub const FAULT_TARGET_DOMAIN_V2: &[u8] = b"onebrain/p5/fault-target/v2";
pub const ADMIN_OPERATION_DOMAIN_V2: &[u8] = b"onebrain/p5/admin-operation-receipt/v2";
pub const SESSION_IDENTITY_DOMAIN_V2: &[u8] = b"onebrain/p5/session-identity/v2";
pub const MAX_SIGN_REQUEST_BYTES: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum P5SignerDomainV2 {
    ChildReceipt,
    FaultTarget,
    AdminOperation,
    SessionIdentity,
    ReachabilityIdentity,
}

impl P5SignerDomainV2 {
    fn cursor_label(self) -> &'static str {
        match self {
            Self::ChildReceipt => "child-receipt",
            Self::FaultTarget => "fault-target",
            Self::AdminOperation => "admin-operation",
            Self::SessionIdentity => "session-identity",
            Self::ReachabilityIdentity => "reachability-identity",
        }
    }

    pub fn bytes(self, reachability_domain: Option<&[u8]>) -> Result<Vec<u8>, P5SignerError> {
        Ok(match self {
            Self::ChildReceipt => CHILD_RECEIPT_DOMAIN_V2.to_vec(),
            Self::FaultTarget => FAULT_TARGET_DOMAIN_V2.to_vec(),
            Self::AdminOperation => ADMIN_OPERATION_DOMAIN_V2.to_vec(),
            Self::SessionIdentity => SESSION_IDENTITY_DOMAIN_V2.to_vec(),
            Self::ReachabilityIdentity => {
                let domain = reachability_domain.ok_or(P5SignerError::DomainRejected)?;
                if domain.is_empty() || domain.len() > 128 {
                    return Err(P5SignerError::DomainRejected);
                }
                let mut out = b"onebrain/p5/reachability-identity/v2\0".to_vec();
                out.extend_from_slice(domain);
                out
            }
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum P5SignerError {
    Stopped,
    Timeout,
    DomainRejected,
    RequestTooLarge,
    Replay,
    CursorBindingMismatch,
    CursorCorrupt,
    Io,
    SignatureRejected,
}

impl std::fmt::Display for P5SignerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "P5_SIGNER_V2: {self:?}")
    }
}
impl std::error::Error for P5SignerError {}

#[derive(Clone, Debug)]
pub struct DurableSequenceCursor {
    path: PathBuf,
    binding: [u8; 32],
}

impl DurableSequenceCursor {
    pub fn open(path: impl AsRef<Path>, binding: [u8; 32]) -> Result<Self, P5SignerError> {
        let cursor = Self {
            path: path.as_ref().to_path_buf(),
            binding,
        };
        if cursor.path.exists() {
            let (_, stored_binding) = cursor.read()?;
            if stored_binding != binding {
                return Err(P5SignerError::CursorBindingMismatch);
            }
        } else {
            cursor.persist(0)?;
        }
        Ok(cursor)
    }

    pub fn highest(&self) -> Result<u64, P5SignerError> {
        self.read().map(|(value, _)| value)
    }

    pub fn advance(&self, sequence: u64) -> Result<(), P5SignerError> {
        if sequence == 0 || sequence <= self.highest()? {
            return Err(P5SignerError::Replay);
        }
        self.persist(sequence)
    }

    fn read(&self) -> Result<(u64, [u8; 32]), P5SignerError> {
        let mut bytes = Vec::new();
        File::open(&self.path)
            .map_err(|_| P5SignerError::Io)?
            .read_to_end(&mut bytes)
            .map_err(|_| P5SignerError::Io)?;
        if bytes.len() != 48 || &bytes[..8] != b"OBP5CUR2" {
            return Err(P5SignerError::CursorCorrupt);
        }
        let mut seq = [0u8; 8];
        seq.copy_from_slice(&bytes[8..16]);
        let mut binding = [0u8; 32];
        binding.copy_from_slice(&bytes[16..48]);
        Ok((u64::from_le_bytes(seq), binding))
    }

    fn persist(&self, sequence: u64) -> Result<(), P5SignerError> {
        let parent = self.path.parent().ok_or(P5SignerError::Io)?;
        fs::create_dir_all(parent).map_err(|_| P5SignerError::Io)?;
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.path)
            .map_err(|_| P5SignerError::Io)?;
        file.write_all(b"OBP5CUR2")
            .and_then(|_| file.write_all(&sequence.to_le_bytes()))
            .and_then(|_| file.write_all(&self.binding))
            .and_then(|_| file.sync_all())
            .map_err(|_| P5SignerError::Io)?;
        sync_parent(parent)
    }
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> Result<(), P5SignerError> {
    File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|_| P5SignerError::Io)
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> Result<(), P5SignerError> {
    // The file itself is flushed above. Windows directory handles require
    // FILE_FLAG_BACKUP_SEMANTICS; deployment durability is exercised on Linux.
    Ok(())
}

pub trait P5SigningProvider: Send + Sync {
    fn public_key(&self) -> [u8; 32];
    fn sign(
        &self,
        domain: P5SignerDomainV2,
        sequence: u64,
        message: &[u8],
        reachability_domain: Option<&[u8]>,
    ) -> Result<[u8; 64], P5SignerError>;
}

pub struct InProcessP5SignerService {
    key: SigningKey,
    cursor: Option<DurableSequenceCursor>,
    domain_cursors: BTreeMap<P5SignerDomainV2, DurableSequenceCursor>,
    running: Mutex<bool>,
    allowed: Vec<P5SignerDomainV2>,
}

impl InProcessP5SignerService {
    /// Test/service-side constructor. The long-lived agent is only given a
    /// `dyn P5SigningProvider`, never this key-owning implementation.
    pub fn service_side(
        key: SigningKey,
        cursor: DurableSequenceCursor,
        allowed: Vec<P5SignerDomainV2>,
    ) -> Self {
        Self {
            key,
            cursor: Some(cursor),
            domain_cursors: BTreeMap::new(),
            running: Mutex::new(true),
            allowed,
        }
    }

    fn service_side_with_domain_cursors(
        key: SigningKey,
        domain_cursors: BTreeMap<P5SignerDomainV2, DurableSequenceCursor>,
        allowed: Vec<P5SignerDomainV2>,
    ) -> Self {
        Self {
            key,
            cursor: None,
            domain_cursors,
            running: Mutex::new(true),
            allowed,
        }
    }
    pub fn set_running(&self, running: bool) {
        *self.running.lock().expect("signer state") = running;
    }
}

impl P5SigningProvider for InProcessP5SignerService {
    fn public_key(&self) -> [u8; 32] {
        *self.key.verifying_key().as_bytes()
    }

    fn sign(
        &self,
        domain: P5SignerDomainV2,
        sequence: u64,
        message: &[u8],
        reachability_domain: Option<&[u8]>,
    ) -> Result<[u8; 64], P5SignerError> {
        if !*self.running.lock().map_err(|_| P5SignerError::Stopped)? {
            return Err(P5SignerError::Stopped);
        }
        if !self.allowed.contains(&domain) {
            return Err(P5SignerError::DomainRejected);
        }
        if message.len() > MAX_SIGN_REQUEST_BYTES {
            return Err(P5SignerError::RequestTooLarge);
        }
        let cursor = self
            .domain_cursors
            .get(&domain)
            .or(self.cursor.as_ref())
            .ok_or(P5SignerError::DomainRejected)?;
        cursor.advance(sequence)?;
        let domain = domain.bytes(reachability_domain)?;
        let mut preimage = Vec::with_capacity(domain.len() + message.len());
        preimage.extend_from_slice(&domain);
        preimage.extend_from_slice(message);
        Ok(self.key.sign(&preimage).to_bytes())
    }
}

#[derive(Clone)]
pub struct ExternalP5Signer {
    provider: Arc<dyn P5SigningProvider>,
    next_sequence: Arc<Mutex<u64>>,
    timeout: Duration,
}

impl ExternalP5Signer {
    pub fn new(
        provider: Arc<dyn P5SigningProvider>,
        initial_sequence: u64,
        timeout: Duration,
    ) -> Self {
        Self {
            provider,
            next_sequence: Arc::new(Mutex::new(initial_sequence)),
            timeout,
        }
    }
    pub fn public_key(&self) -> [u8; 32] {
        self.provider.public_key()
    }
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
    pub fn sign(
        &self,
        domain: P5SignerDomainV2,
        message: &[u8],
        reachability_domain: Option<&[u8]>,
    ) -> Result<[u8; 64], P5SignerError> {
        if self.timeout.is_zero() {
            return Err(P5SignerError::Timeout);
        }
        let mut guard = self
            .next_sequence
            .lock()
            .map_err(|_| P5SignerError::Stopped)?;
        *guard = guard.checked_add(1).ok_or(P5SignerError::Replay)?;
        self.provider
            .sign(domain, *guard, message, reachability_domain)
    }
}

/// Blocking Unix-socket client used by the Linux agent. The synchronous OBP
/// signer traits remain unchanged; hard socket deadlines prevent a stopped or
/// wedged signer service from blocking a Tokio worker indefinitely.
#[cfg(unix)]
pub struct UnixSocketP5SigningProvider {
    socket: PathBuf,
    expected_public_key: [u8; 32],
    timeout: Duration,
    cursor: Option<DurableSequenceCursor>,
}

#[cfg(unix)]
impl UnixSocketP5SigningProvider {
    pub fn new(
        socket: impl AsRef<Path>,
        expected_public_key: [u8; 32],
        timeout: Duration,
    ) -> Result<Self, P5SignerError> {
        if expected_public_key == [0; 32] || timeout.is_zero() {
            return Err(P5SignerError::SignatureRejected);
        }
        Ok(Self {
            socket: socket.as_ref().to_path_buf(),
            expected_public_key,
            timeout,
            cursor: None,
        })
    }

    pub fn with_cursor(mut self, cursor: DurableSequenceCursor) -> Self {
        self.cursor = Some(cursor);
        self
    }
}

#[cfg(unix)]
impl P5SigningProvider for UnixSocketP5SigningProvider {
    fn public_key(&self) -> [u8; 32] {
        self.expected_public_key
    }

    fn sign(
        &self,
        domain: P5SignerDomainV2,
        sequence: u64,
        message: &[u8],
        reachability_domain: Option<&[u8]>,
    ) -> Result<[u8; 64], P5SignerError> {
        if sequence == 0 || message.len() > MAX_SIGN_REQUEST_BYTES {
            return Err(P5SignerError::RequestTooLarge);
        }
        if let Some(cursor) = &self.cursor {
            cursor.advance(sequence)?;
        }
        let mut payload = Vec::with_capacity(message.len() + 130);
        if domain == P5SignerDomainV2::ReachabilityIdentity {
            let value = reachability_domain.ok_or(P5SignerError::DomainRejected)?;
            let length = u16::try_from(value.len()).map_err(|_| P5SignerError::DomainRejected)?;
            if value.is_empty() || value.len() > 128 {
                return Err(P5SignerError::DomainRejected);
            }
            payload.extend_from_slice(&length.to_be_bytes());
            payload.extend_from_slice(value);
        } else if reachability_domain.is_some() {
            return Err(P5SignerError::DomainRejected);
        }
        payload.extend_from_slice(message);
        if payload.len() > MAX_SIGN_REQUEST_BYTES {
            return Err(P5SignerError::RequestTooLarge);
        }
        let mut stream = UnixStream::connect(&self.socket).map_err(|_| P5SignerError::Stopped)?;
        stream
            .set_read_timeout(Some(self.timeout))
            .and_then(|_| stream.set_write_timeout(Some(self.timeout)))
            .map_err(|_| P5SignerError::Io)?;
        let tag = match domain {
            P5SignerDomainV2::ChildReceipt => 1,
            P5SignerDomainV2::FaultTarget => 2,
            P5SignerDomainV2::AdminOperation => 3,
            P5SignerDomainV2::SessionIdentity => 4,
            P5SignerDomainV2::ReachabilityIdentity => 5,
        };
        stream
            .write_all(&[tag])
            .and_then(|_| stream.write_all(&sequence.to_be_bytes()))
            .and_then(|_| stream.write_all(&(payload.len() as u32).to_be_bytes()))
            .and_then(|_| stream.write_all(&payload))
            .and_then(|_| stream.flush())
            .map_err(|error| {
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) {
                    P5SignerError::Timeout
                } else {
                    P5SignerError::Io
                }
            })?;
        let mut response = [0u8; 96];
        stream.read_exact(&mut response).map_err(|error| {
            if matches!(
                error.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) {
                P5SignerError::Timeout
            } else {
                P5SignerError::Io
            }
        })?;
        if response[..32] != self.expected_public_key {
            return Err(P5SignerError::SignatureRejected);
        }
        response[32..]
            .try_into()
            .map_err(|_| P5SignerError::SignatureRejected)
    }
}

impl SessionIdentitySigner for ExternalP5Signer {
    fn public_key(&self) -> [u8; 32] {
        self.public_key()
    }
    fn sign_session_message(&self, message: &[u8]) -> Result<[u8; 64], String> {
        self.sign(P5SignerDomainV2::SessionIdentity, message, None)
            .map_err(|e| e.to_string())
    }
}

impl ReachabilityIdentitySigner for ExternalP5Signer {
    fn public_key(&self) -> [u8; 32] {
        self.public_key()
    }
    fn sign_reachability_message(
        &self,
        domain: &'static [u8],
        message: &[u8],
    ) -> Result<[u8; 64], ReachabilityCryptoError> {
        self.sign(
            P5SignerDomainV2::ReachabilityIdentity,
            message,
            Some(domain),
        )
        .map_err(|_| ReachabilityCryptoError::SignerUnavailable)
    }
}

pub fn write_new_signing_key(path: &Path, bytes: [u8; 32]) -> Result<(), P5SignerError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| P5SignerError::Io)?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| P5SignerError::Io)?;
    if let Some(parent) = path.parent() {
        sync_parent(parent)?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P5SignerServiceKindV2 {
    Receipt,
    Identity,
}

/// Closed CLI shared by the two signer executables. The private key path is
/// accepted only by the dedicated signer process, never by the agent.
pub fn run_signer_service_cli(
    kind: P5SignerServiceKindV2,
) -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [mode, flag, output] if mode == "generate-key" && flag == "--output" => {
            let mut secret = [0u8; 32];
            getrandom::fill(&mut secret)?;
            write_new_signing_key(Path::new(output), secret)?;
            Ok(())
        }
        [mode, flag, key] if mode == "print-public" && flag == "--signing-key" => {
            let key = read_signing_key(Path::new(key))?;
            println!("{}", hex(key.verifying_key().as_bytes()));
            Ok(())
        }
        [mode, fd_flag, fd, key_flag, key, config_flag, config]
            if mode == "serve"
                && fd_flag == "--socket-fd"
                && fd == "3"
                && key_flag == "--signing-key"
                && config_flag == "--session-config"
                && config == "/run/onebrain/p5-v2/current-session.json" =>
        {
            if !Path::new(config).is_file() {
                return Err("canonical session config is unavailable".into());
            }
            let key_path = Path::new(key);
            let signing_key = read_signing_key(key_path)?;
            let allowed = match kind {
                P5SignerServiceKindV2::Receipt => vec![
                    P5SignerDomainV2::ChildReceipt,
                    P5SignerDomainV2::FaultTarget,
                    P5SignerDomainV2::AdminOperation,
                ],
                P5SignerServiceKindV2::Identity => vec![
                    P5SignerDomainV2::SessionIdentity,
                    P5SignerDomainV2::ReachabilityIdentity,
                ],
            };
            let binding = *blake3::hash(&fs::read(config)?).as_bytes();
            let mut domain_cursors = BTreeMap::new();
            for domain in &allowed {
                let extension = format!("{}.cursor-v2", domain.cursor_label());
                domain_cursors.insert(
                    *domain,
                    DurableSequenceCursor::open(key_path.with_extension(extension), binding)?,
                );
            }
            serve_signer_fd3(InProcessP5SignerService::service_side_with_domain_cursors(
                signing_key,
                domain_cursors,
                allowed,
            ))
        }
        _ => Err("invalid closed signer command".into()),
    }
}

fn read_signing_key(path: &Path) -> Result<SigningKey, P5SignerError> {
    let bytes = fs::read(path).map_err(|_| P5SignerError::Io)?;
    if bytes.len() != 32 {
        return Err(P5SignerError::Io);
    }
    let mut secret = [0u8; 32];
    secret.copy_from_slice(&bytes);
    Ok(SigningKey::from_bytes(&secret))
}

#[cfg(unix)]
fn serve_signer_fd3(service: InProcessP5SignerService) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::fd::FromRawFd;
    use std::os::unix::net::UnixListener;
    let listener = unsafe { UnixListener::from_raw_fd(3) };
    loop {
        let (mut stream, _) = listener.accept()?;
        let mut header = [0u8; 13];
        stream.read_exact(&mut header)?;
        let domain = match header[0] {
            1 => P5SignerDomainV2::ChildReceipt,
            2 => P5SignerDomainV2::FaultTarget,
            3 => P5SignerDomainV2::AdminOperation,
            4 => P5SignerDomainV2::SessionIdentity,
            5 => P5SignerDomainV2::ReachabilityIdentity,
            _ => return Err("unknown signer domain".into()),
        };
        let mut seq = [0u8; 8];
        seq.copy_from_slice(&header[1..9]);
        let mut len = [0u8; 4];
        len.copy_from_slice(&header[9..13]);
        let length = u32::from_be_bytes(len) as usize;
        if length > MAX_SIGN_REQUEST_BYTES {
            return Err("sign request exceeds bound".into());
        }
        let mut message = vec![0; length];
        stream.read_exact(&mut message)?;
        let (message, reachability_domain) = if domain == P5SignerDomainV2::ReachabilityIdentity {
            if message.len() < 3 {
                return Err("reachability signer domain is truncated".into());
            }
            let domain_length = u16::from_be_bytes([message[0], message[1]]) as usize;
            if domain_length == 0 || domain_length > 128 || message.len() <= 2 + domain_length {
                return Err("reachability signer domain is invalid".into());
            }
            (
                &message[2 + domain_length..],
                Some(&message[2..2 + domain_length]),
            )
        } else {
            (message.as_slice(), None)
        };
        let signature = service.sign(
            domain,
            u64::from_be_bytes(seq),
            message,
            reachability_domain,
        )?;
        stream.write_all(&service.public_key())?;
        stream.write_all(&signature)?;
        stream.flush()?;
    }
}

#[cfg(not(unix))]
fn serve_signer_fd3(_service: InProcessP5SignerService) -> Result<(), Box<dyn std::error::Error>> {
    Err("signer service requires Unix fd passing".into())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vnext_p5_multi_host_v2_cursor_rejects_replay_after_restart() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("cursor");
        DurableSequenceCursor::open(&path, [1; 32])
            .unwrap()
            .advance(7)
            .unwrap();
        let reopened = DurableSequenceCursor::open(&path, [1; 32]).unwrap();
        assert_eq!(reopened.highest().unwrap(), 7);
        assert_eq!(reopened.advance(7), Err(P5SignerError::Replay));
        assert!(matches!(
            DurableSequenceCursor::open(&path, [2; 32]),
            Err(P5SignerError::CursorBindingMismatch)
        ));
    }

    #[test]
    fn vnext_p5_multi_host_v2_signer_outage_fails_closed_and_recovers_with_sequence_floor() {
        let temp = tempfile::tempdir().unwrap();
        let cursor = DurableSequenceCursor::open(temp.path().join("cursor"), [3; 32]).unwrap();
        let service = Arc::new(InProcessP5SignerService::service_side(
            SigningKey::from_bytes(&[4; 32]),
            cursor,
            vec![P5SignerDomainV2::SessionIdentity],
        ));
        let client = ExternalP5Signer::new(service.clone(), 0, Duration::from_secs(1));
        assert!(client
            .sign(P5SignerDomainV2::SessionIdentity, b"first", None)
            .is_ok());
        service.set_running(false);
        assert_eq!(
            client.sign(P5SignerDomainV2::SessionIdentity, b"outage", None),
            Err(P5SignerError::Stopped)
        );
        service.set_running(true);
        assert!(client
            .sign(P5SignerDomainV2::SessionIdentity, b"recovered", None)
            .is_ok());
    }

    #[test]
    fn vnext_p5_multi_host_v2_signer_has_closed_domains_and_no_key_export() {
        let temp = tempfile::tempdir().unwrap();
        let service = Arc::new(InProcessP5SignerService::service_side(
            SigningKey::from_bytes(&[5; 32]),
            DurableSequenceCursor::open(temp.path().join("cursor"), [6; 32]).unwrap(),
            vec![P5SignerDomainV2::ChildReceipt],
        ));
        assert_eq!(
            service.sign(P5SignerDomainV2::AdminOperation, 1, b"x", None),
            Err(P5SignerError::DomainRejected)
        );
        assert_eq!(service.public_key().len(), 32);
    }

    #[test]
    fn vnext_p5_multi_host_v2_signer_replay_is_scoped_per_domain() {
        let temp = tempfile::tempdir().unwrap();
        let binding = [7; 32];
        let mut cursors = BTreeMap::new();
        cursors.insert(
            P5SignerDomainV2::AdminOperation,
            DurableSequenceCursor::open(temp.path().join("admin.cursor"), binding).unwrap(),
        );
        cursors.insert(
            P5SignerDomainV2::ChildReceipt,
            DurableSequenceCursor::open(temp.path().join("child.cursor"), binding).unwrap(),
        );
        let service = InProcessP5SignerService::service_side_with_domain_cursors(
            SigningKey::from_bytes(&[8; 32]),
            cursors,
            vec![
                P5SignerDomainV2::AdminOperation,
                P5SignerDomainV2::ChildReceipt,
            ],
        );

        assert!(service
            .sign(P5SignerDomainV2::AdminOperation, 1, b"admin", None)
            .is_ok());
        assert!(service
            .sign(P5SignerDomainV2::ChildReceipt, 1, b"child", None)
            .is_ok());
        assert_eq!(
            service.sign(P5SignerDomainV2::AdminOperation, 1, b"replay", None),
            Err(P5SignerError::Replay)
        );
        assert!(service
            .sign(P5SignerDomainV2::ChildReceipt, 2, b"next", None)
            .is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn unix_signer_client_binds_dynamic_reachability_domain_and_public_key() {
        use std::os::unix::net::UnixListener;
        use std::thread;

        let temp = tempfile::tempdir().unwrap();
        let socket = temp.path().join("signer.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut header = [0u8; 13];
            stream.read_exact(&mut header).unwrap();
            assert_eq!(header[0], 5);
            assert_eq!(u64::from_be_bytes(header[1..9].try_into().unwrap()), 9);
            let length = u32::from_be_bytes(header[9..13].try_into().unwrap()) as usize;
            let mut payload = vec![0; length];
            stream.read_exact(&mut payload).unwrap();
            let domain_length = u16::from_be_bytes(payload[..2].try_into().unwrap()) as usize;
            assert_eq!(
                &payload[2..2 + domain_length],
                b"onebrain/test/reachability/v1"
            );
            assert_eq!(&payload[2 + domain_length..], b"canonical-message");
            stream.write_all(&[0xA1; 32]).unwrap();
            stream.write_all(&[0xB2; 64]).unwrap();
        });
        let client =
            UnixSocketP5SigningProvider::new(&socket, [0xA1; 32], Duration::from_secs(1)).unwrap();
        assert_eq!(
            client
                .sign(
                    P5SignerDomainV2::ReachabilityIdentity,
                    9,
                    b"canonical-message",
                    Some(b"onebrain/test/reachability/v1"),
                )
                .unwrap(),
            [0xB2; 64]
        );
        server.join().unwrap();
    }
}
