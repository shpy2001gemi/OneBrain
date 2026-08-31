//! HTTPS bootstrap over Task 3 sealed dial tokens.
//!
//! Redirects, ambient proxies and ambient DNS are disabled. The original
//! configured host remains the URL/SNI/Host authority while reqwest connects
//! only to the address set revalidated by `ReachabilityDialValidator`.

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use ku_net::vnext_reachability_crypto::{
    ConfiguredBootstrapSource, PreparedBootstrapManifest, ReachabilityAdmission,
    ReachabilityAdmissionPreparer, ReachabilityDialValidator, ReachabilityLockFreeDialValidation,
    ReachabilityLockFreePreparation, ReachabilityRecordAdmission, RelayAdmissionError,
    ValidatedBootstrapManifest, ValidatedPublicDialEndpoint, ValidatedPublicDialTransportV1,
};
use onebrain_protocol::HostAddressV1;
use reqwest::{Client, StatusCode};

const DEFAULT_GLOBAL_BUDGET: Duration = Duration::from_secs(20);
const MAX_MANIFEST_BYTES: usize = 1_048_576;
static PROVIDER_INSTALL: OnceLock<Result<(), String>> = OnceLock::new();

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootstrapClientError {
    CryptoProvider,
    InvalidDialToken,
    InvalidUrl,
    RedirectRejected,
    Transport,
    ByteLimit,
    NoBootstrapReachable,
    Admission(RelayAdmissionError),
}

impl From<RelayAdmissionError> for BootstrapClientError {
    fn from(value: RelayAdmissionError) -> Self {
        Self::Admission(value)
    }
}

pub fn install_aws_lc_provider() -> Result<(), BootstrapClientError> {
    PROVIDER_INSTALL
        .get_or_init(|| {
            if rustls::crypto::CryptoProvider::get_default().is_some() {
                return Ok(());
            }
            rustls::crypto::aws_lc_rs::default_provider()
                .install_default()
                .map_err(|_| "unable to install the AWS-LC Rustls provider".to_owned())
        })
        .clone()
        .map_err(|_| BootstrapClientError::CryptoProvider)
}

#[derive(Clone)]
pub struct VNextBootstrapClient {
    admission_preparer: Arc<ReachabilityAdmissionPreparer>,
    dial_validator: Arc<ReachabilityDialValidator>,
    global_budget: Duration,
    max_manifest_bytes: usize,
}

impl VNextBootstrapClient {
    pub fn new(
        admission_preparer: Arc<ReachabilityAdmissionPreparer>,
        dial_validator: Arc<ReachabilityDialValidator>,
    ) -> Result<Self, BootstrapClientError> {
        install_aws_lc_provider()?;
        Ok(Self {
            admission_preparer,
            dial_validator,
            global_budget: DEFAULT_GLOBAL_BUDGET,
            max_manifest_bytes: MAX_MANIFEST_BYTES,
        })
    }

    pub async fn fetch_configured_manifest(
        &self,
        source: &ConfiguredBootstrapSource,
        now: u64,
    ) -> Result<PreparedBootstrapManifest, BootstrapClientError> {
        let deadline = Instant::now() + self.global_budget;
        let token = self
            .dial_validator
            .validate_configured_bootstrap_dial(source, deadline)
            .await?;
        let bytes = tokio::time::timeout(
            self.global_budget,
            fetch_sealed(&token, self.max_manifest_bytes),
        )
        .await
        .map_err(|_| BootstrapClientError::Transport)??;
        self.admission_preparer
            .prepare_bootstrap(&bytes, source, now, deadline)
            .await
            .map_err(BootstrapClientError::Admission)
    }

    pub async fn admit_local_manifest(
        &self,
        canonical_bytes: &[u8],
        source: &ConfiguredBootstrapSource,
        now: u64,
    ) -> Result<PreparedBootstrapManifest, BootstrapClientError> {
        if canonical_bytes.len() > self.max_manifest_bytes {
            return Err(BootstrapClientError::ByteLimit);
        }
        self.admission_preparer
            .prepare_bootstrap(
                canonical_bytes,
                source,
                now,
                Instant::now() + self.global_budget,
            )
            .await
            .map_err(BootstrapClientError::Admission)
    }

    pub async fn bootstrap_all(
        &self,
        sources: &[ConfiguredBootstrapSource],
        admission: &mut ReachabilityAdmission,
        now: u64,
    ) -> Result<Vec<ValidatedBootstrapManifest>, BootstrapClientError> {
        if sources.is_empty() {
            return Err(BootstrapClientError::NoBootstrapReachable);
        }
        let global_deadline = tokio::time::Instant::now() + self.global_budget;
        let mut tasks = tokio::task::JoinSet::new();
        for source in sources.iter().cloned() {
            let client = self.clone();
            tasks.spawn(async move {
                let result = client.fetch_configured_manifest(&source, now).await;
                (source, result)
            });
        }
        let mut prepared = Vec::new();
        loop {
            let next = tokio::time::timeout_at(global_deadline, tasks.join_next()).await;
            match next {
                Ok(Some(Ok((source, Ok(value))))) => prepared.push((value, source)),
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(_) => {
                    tasks.abort_all();
                    break;
                }
            }
        }
        if prepared.is_empty() {
            return Err(BootstrapClientError::NoBootstrapReachable);
        }
        prepared.sort_by_key(|(_, source)| *source.authority_digest());
        let mut admitted = Vec::with_capacity(prepared.len());
        for (value, source) in prepared {
            admitted.push(admission.register_prepared_bootstrap(value, &source, now)?);
        }
        Ok(admitted)
    }
}

async fn fetch_sealed(
    token: &ValidatedPublicDialEndpoint,
    max_bytes: usize,
) -> Result<Vec<u8>, BootstrapClientError> {
    if token.transport() != ValidatedPublicDialTransportV1::BootstrapHttps
        || token.dial_addresses().is_empty()
    {
        return Err(BootstrapClientError::InvalidDialToken);
    }
    let (authority, resolver_name) = host_authority(token.signed_host())?;
    let path = token.signed_path().unwrap_or("/");
    let url = format!("https://{authority}:{}{path}", token.port());
    let mut builder = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(5))
        .timeout(DEFAULT_GLOBAL_BUDGET);
    if let Some(name) = resolver_name {
        builder = builder.resolve_to_addrs(name, token.dial_addresses());
    }
    let client = builder
        .build()
        .map_err(|_| BootstrapClientError::Transport)?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| BootstrapClientError::Transport)?;
    if response.status().is_redirection() {
        return Err(BootstrapClientError::RedirectRejected);
    }
    if response.status() != StatusCode::OK {
        return Err(BootstrapClientError::Transport);
    }
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(BootstrapClientError::ByteLimit);
    }
    let mut output = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| BootstrapClientError::Transport)?
    {
        if output.len().saturating_add(chunk.len()) > max_bytes {
            return Err(BootstrapClientError::ByteLimit);
        }
        output.extend_from_slice(&chunk);
    }
    Ok(output)
}

fn host_authority(host: &HostAddressV1) -> Result<(String, Option<&str>), BootstrapClientError> {
    match host {
        HostAddressV1::Dns(name) => Ok((name.clone(), Some(name.as_str()))),
        HostAddressV1::Ipv4(bytes) => Ok((std::net::Ipv4Addr::from(*bytes).to_string(), None)),
        HostAddressV1::Ipv6(bytes) => Ok((format!("[{}]", std::net::Ipv6Addr::from(*bytes)), None)),
    }
}

impl std::fmt::Display for BootstrapClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OBP_BOOTSTRAP: {self:?}")
    }
}

impl std::error::Error for BootstrapClientError {}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use ku_net::vnext_session::principal_node_id;
    use onebrain_protocol::{
        encode_reachability_object, reachability_signing_bytes, BootstrapManifestV1,
        DiscoveryEndpointV1, DiscoveryTransportV1, ProtocolVersionV1, ReachabilityObjectV1,
        ReachabilitySignatureRoleV1,
    };

    #[test]
    fn aws_lc_provider_install_is_idempotent() {
        install_aws_lc_provider().unwrap();
        install_aws_lc_provider().unwrap();
    }

    #[test]
    fn empty_source_set_is_a_typed_limitation_without_blocking_startup() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let resolver = Arc::new(EmptyResolver);
        let preparer = Arc::new(ReachabilityAdmissionPreparer::new(resolver.clone(), 1).unwrap());
        let dial = Arc::new(ReachabilityDialValidator::new(resolver, 1).unwrap());
        let client = VNextBootstrapClient::new(preparer, dial).unwrap();
        let replay =
            Arc::new(ku_net::vnext_reachability_crypto::InMemoryReachabilityReplayStore::default());
        let mut admission = ReachabilityAdmission::new(replay);
        assert_eq!(
            runtime.block_on(client.bootstrap_all(&[], &mut admission, 1)),
            Err(BootstrapClientError::NoBootstrapReachable)
        );
    }

    #[test]
    fn stale_local_manifest_is_rejected_before_it_can_seed_refresh() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let key = SigningKey::from_bytes(&[51; 32]);
        let public_key = key
            .verifying_key()
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let directory = tempfile::tempdir().unwrap();
        let source_path = directory.path().join("bootstrap-source.conf");
        std::fs::write(
            &source_path,
            format!(
                "format=onebrain/bootstrap-source/1\npublic_key={public_key}\ntransport=https\nhost=ipv4:1.1.1.1\nport=443\npath=/bootstrap\n"
            ),
        )
        .unwrap();
        let source = ConfiguredBootstrapSource::load_from_trusted_local_file(&source_path).unwrap();
        let now = 10_000;
        let mut manifest = BootstrapManifestV1 {
            format: 1,
            discovery_source_id: *principal_node_id(key.verifying_key().as_bytes()).as_bytes(),
            discovery_endpoints: vec![DiscoveryEndpointV1 {
                transport: DiscoveryTransportV1::Https,
                host: HostAddressV1::Ipv4([1, 1, 1, 1]),
                port: 443,
                path: "/bootstrap".into(),
            }],
            protocol_versions: vec![ProtocolVersionV1 { major: 1, minor: 0 }],
            sequence: 1,
            issued_at: now - 600,
            expires_at: now - 1,
            source_signature: [0; 64],
        };
        manifest.source_signature = key
            .sign(
                &reachability_signing_bytes(
                    &ReachabilityObjectV1::BootstrapManifest(manifest.clone()),
                    ReachabilitySignatureRoleV1::BootstrapSource,
                )
                .unwrap(),
            )
            .to_bytes();
        let bytes =
            encode_reachability_object(&ReachabilityObjectV1::BootstrapManifest(manifest)).unwrap();
        let resolver = Arc::new(StaticResolver);
        let preparer = Arc::new(ReachabilityAdmissionPreparer::new(resolver.clone(), 1).unwrap());
        let dial = Arc::new(ReachabilityDialValidator::new(resolver, 1).unwrap());
        let client = VNextBootstrapClient::new(preparer, dial).unwrap();
        assert_eq!(
            runtime.block_on(client.admit_local_manifest(&bytes, &source, now)),
            Err(BootstrapClientError::Admission(
                RelayAdmissionError::Expired
            ))
        );
    }

    struct EmptyResolver;

    struct StaticResolver;

    impl ku_net::vnext_reachability_crypto::PublicEndpointResolver for StaticResolver {
        fn resolve(
            &self,
            _host: &HostAddressV1,
            _deadline: Instant,
        ) -> Result<Vec<std::net::IpAddr>, RelayAdmissionError> {
            Ok(vec!["1.1.1.1".parse().unwrap()])
        }
    }

    impl ku_net::vnext_reachability_crypto::PublicEndpointResolver for EmptyResolver {
        fn resolve(
            &self,
            _host: &HostAddressV1,
            _deadline: Instant,
        ) -> Result<Vec<std::net::IpAddr>, RelayAdmissionError> {
            Err(RelayAdmissionError::DnsResolutionFailed)
        }
    }
}
