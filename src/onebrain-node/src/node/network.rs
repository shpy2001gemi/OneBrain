//! Peer networking for [`OneBrainNode`]: vNext signer/dependency injection,
//! network start/shutdown (opt-in legacy TCP listener or vNext OBP-RP runtime),
//! seed connect, legacy KU broadcast/verification, vNext runtime controls and
//! peer snapshots, plus the bounded legacy background listener.

use super::*;

impl OneBrainNode {
    /// Inject a caller-owned signer before starting the network. A generation
    /// restore may also supply this handle through typed identity recovery;
    /// the Base dataset path never creates a compatibility key file.
    #[cfg(feature = "vnext-network-runtime")]
    pub fn set_vnext_identity_signer(&mut self, signer: Arc<dyn SessionIdentitySigner>) {
        self.vnext_identity_signer = Some(signer);
    }

    /// Inject caller-owned encrypted Vault and allow-listed policy handles
    /// before starting the integrated vNext product runtime.
    #[cfg(feature = "vnext-network-runtime")]
    pub fn set_vnext_product_dependencies(
        &mut self,
        dependencies: VNextProductRuntimeDependencies,
    ) -> Result<(), NodeError> {
        if self.vnext_product_runtime.is_some() {
            return Err(NodeError::Config(
                "vNext product dependencies cannot change after runtime startup".into(),
            ));
        }
        self.vnext_product_dependencies = Some(dependencies);
        Ok(())
    }

    /// Whether [`Self::start_network`] has anything to start: the explicitly
    /// opted-in legacy TCP listener or the configured vNext OBP-RP runtime.
    pub fn network_listener_requested(&self) -> bool {
        if self
            .config
            .legacy_tcp_listener
            .bind_addr(self.config.port)
            .is_some()
        {
            return true;
        }
        #[cfg(feature = "vnext-network-runtime")]
        if self
            .config
            .vnext
            .is_active(crate::vnext_config::VNextFeature::ObpRp)
        {
            return true;
        }
        false
    }

    /// Start the configured network listeners.
    ///
    /// The authenticated vNext runtime starts when OBP-RP is active. The
    /// unauthenticated legacy TCP/JSON listener binds only when
    /// [`NodeConfig::legacy_tcp_listener`] opts in, on loopback or on every
    /// interface. Returns the legacy listener address when it is bound,
    /// otherwise the vNext listener address. Fails without side effects when
    /// neither listener is requested.
    pub async fn start_network(&mut self) -> Result<SocketAddr, NodeError> {
        if self.listener_task.is_some() {
            return Err(NodeError::Network(
                "network listener is already running".into(),
            ));
        }
        #[cfg(feature = "vnext-network-runtime")]
        if self.vnext_product_runtime.is_some() {
            return Err(NodeError::Network(
                "network listener is already running".into(),
            ));
        }
        if !self.network_listener_requested() {
            return Err(NodeError::Config(NO_NETWORK_LISTENER_REQUESTED.into()));
        }
        let legacy_bind = self.config.legacy_tcp_listener.bind_addr(self.config.port);
        #[cfg(feature = "vnext-outbound-first")]
        if let Some(dependencies) = self.vnext_product_dependencies.as_ref() {
            dependencies
                .validate_outbound_first(&self.config.vnext)
                .map_err(|error| NodeError::Config(error.to_string()))?;
        }
        // The authenticated vNext carrier keeps its all-interface bind.
        #[cfg(feature = "vnext-network-runtime")]
        let mut pending_vnext = self
            .start_vnext_product_runtime(([0, 0, 0, 0], self.config.port).into())
            .await?;
        let Some(bind_addr) = legacy_bind else {
            #[cfg(feature = "vnext-network-runtime")]
            return self.adopt_vnext_only_runtime(pending_vnext.take());
            #[cfg(not(feature = "vnext-network-runtime"))]
            return Err(NodeError::Config(NO_NETWORK_LISTENER_REQUESTED.into()));
        };
        let listener = match TcpListener::bind(bind_addr).await {
            Ok(listener) => listener,
            Err(error) => {
                #[cfg(feature = "vnext-network-runtime")]
                if let Some(runtime) = pending_vnext.take() {
                    if let Err(rollback_error) = runtime.rollback_startup().await {
                        return Err(NodeError::Network(format!(
                            "Failed to bind TCP on {bind_addr}: {error}; vNext startup rollback failed: {rollback_error}"
                        )));
                    }
                }
                return Err(NodeError::Network(format!(
                    "Failed to bind TCP on {}: {}",
                    bind_addr, error
                )));
            }
        };

        let local_addr = listener
            .local_addr()
            .map_err(|e| NodeError::Network(format!("Failed to get local addr: {}", e)))?;
        self.listener_addr = Some(local_addr);
        #[cfg(feature = "vnext-network-runtime")]
        {
            self.vnext_product_runtime = pending_vnext;
        }

        // Spawn background listener task
        let shared = Arc::clone(&self.shared);
        let event_tx = self.event_tx.clone();
        self.listener_task = Some(tokio::spawn(async move {
            listener_loop(listener, shared, event_tx).await;
        }));

        Ok(local_addr)
    }

    #[cfg(feature = "vnext-network-runtime")]
    fn adopt_vnext_only_runtime(
        &mut self,
        runtime: Option<VNextProductRuntime>,
    ) -> Result<SocketAddr, NodeError> {
        let runtime =
            runtime.ok_or_else(|| NodeError::Config(NO_NETWORK_LISTENER_REQUESTED.into()))?;
        let addr = runtime.services().local_addr();
        self.vnext_product_runtime = Some(runtime);
        Ok(addr)
    }

    #[cfg(feature = "vnext-network-runtime")]
    async fn start_vnext_product_runtime(
        &mut self,
        bind_addr: SocketAddr,
    ) -> Result<Option<VNextProductRuntime>, NodeError> {
        if !self
            .config
            .vnext
            .is_active(crate::vnext_config::VNextFeature::ObpRp)
        {
            return Ok(None);
        }
        let dependencies = self.vnext_product_dependencies.take().ok_or_else(|| {
            NodeError::Config(
                "active OBP-RP requires caller-owned vNext Vault and Policy dependencies".into(),
            )
        })?;
        let identity_signer = match self.vnext_identity_signer.clone() {
            Some(signer) => Some(signer),
            None => self
                ._dataset_generations
                .session_identity_signer()
                .map_err(|error| NodeError::Storage(error.to_string()))?,
        };
        VNextProductRuntime::start_in_dataset(
            self.dataset_paths.as_ref(),
            bind_addr,
            &self.config.vnext,
            dependencies,
            identity_signer,
        )
        .await
        .map(Some)
        .map_err(|error| {
            NodeError::Network(format!(
                "Failed to start integrated vNext product runtime: {error}"
            ))
        })
    }

    /// Start only the node-owned vNext runtime. Legacy TCP discovery and
    /// inbound peer service remain stopped; existing legacy state is untouched.
    #[cfg(feature = "vnext-network-runtime")]
    pub async fn start_vnext_network_only(&mut self) -> Result<SocketAddr, NodeError> {
        if self.listener_task.is_some() || self.vnext_product_runtime.is_some() {
            return Err(NodeError::Network(
                "network listener is already running".into(),
            ));
        }
        if !self
            .config
            .vnext
            .is_active(crate::vnext_config::VNextFeature::ObpRp)
        {
            return Err(NodeError::Config(
                "vNext product runtime is not requested".into(),
            ));
        }
        #[cfg(feature = "vnext-outbound-first")]
        if let Some(dependencies) = self.vnext_product_dependencies.as_ref() {
            dependencies
                .validate_outbound_first(&self.config.vnext)
                .map_err(|error| NodeError::Config(error.to_string()))?;
        }
        let bind_addr: SocketAddr = ([0, 0, 0, 0], self.config.port).into();
        let runtime = self
            .start_vnext_product_runtime(bind_addr)
            .await?
            .ok_or_else(|| NodeError::Config("vNext product runtime is not requested".into()))?;
        let addr = runtime.services().local_addr();
        self.vnext_product_runtime = Some(runtime);
        Ok(addr)
    }

    /// Fence new network operations and stop every node-owned listener and
    /// integrated vNext owner in deterministic order.
    pub async fn shutdown_network(&mut self) {
        if let Some(mut task) = self.listener_task.take() {
            task.abort();
            let _ = (&mut task).await;
        }
        self.listener_addr = None;
        #[cfg(feature = "vnext-network-runtime")]
        if let Some(mut runtime) = self.vnext_product_runtime.take() {
            runtime.shutdown().await;
        }
    }

    /// Legacy TCP peer handshake; never vNext seed or relay authority.
    pub async fn connect_to_seed(&self, addr: SocketAddr) -> Result<(), NodeError> {
        let mut stream = TcpStream::connect(addr)
            .await
            .map_err(|e| NodeError::Network(format!("Failed to connect to {}: {}", addr, e)))?;

        // Send our hello
        let ku_count = {
            let state = self.shared.lock().await;
            state.storage.count().unwrap_or(0) as u64
        };
        let hello = NetMessage::PeerHello {
            name: self.config.name.clone(),
            port: self.config.port,
            ku_count,
        };
        send_message(&mut stream, &hello)
            .await
            .map_err(|e| NodeError::Network(format!("Failed to send hello: {}", e)))?;

        // Receive peer's hello
        match recv_message(&mut stream).await {
            Ok(NetMessage::PeerHello {
                name,
                port: _,
                ku_count: peer_ku_count,
            }) => {
                let peer_info = PeerInfo {
                    name: name.clone(),
                    addr,
                    ku_count: peer_ku_count,
                };
                let mut state = self.shared.lock().await;
                state.peer_manager.add_peer(peer_info);
                eprintln!(
                    "  ✓ Connected to peer '{}' at {} ({} KUs)",
                    name, addr, peer_ku_count
                );
            }
            Ok(other) => {
                eprintln!("  ⚠ Unexpected message from seed {}: {:?}", addr, other);
            }
            Err(e) => {
                eprintln!("  ⚠ No hello response from {}: {}", addr, e);
                // Still add as peer (one-way handshake is OK for demo)
                let peer_info = PeerInfo {
                    name: format!("peer@{}", addr),
                    addr,
                    ku_count: 0,
                };
                let mut state = self.shared.lock().await;
                state.peer_manager.add_peer(peer_info);
            }
        }

        Ok(())
    }

    /// Broadcast a KU to all connected peers.
    ///
    /// Sends KuPush to all known peers (fire-and-forget, non-blocking).
    pub async fn broadcast_ku(&self, cid_hex: &str, wire_bytes: &[u8], source_text: &str) -> usize {
        let addrs = {
            let state = self.shared.lock().await;
            state.peer_manager.known_addrs()
        };

        if addrs.is_empty() {
            return 0;
        }

        let peer_count = addrs.len();
        let msg = NetMessage::KuPush {
            cid_hex: cid_hex.to_string(),
            wire_bytes: wire_bytes.to_vec(),
            source_text: source_text.to_string(),
        };

        for addr in &addrs {
            let msg = msg.clone();
            let addr = *addr;
            tokio::spawn(async move {
                if let Ok(mut stream) = TcpStream::connect(addr).await {
                    let _ = send_message(&mut stream, &msg).await;
                }
            });
        }

        peer_count
    }

    /// Request verification from all connected peers.
    ///
    /// Sends VerifyRequest and collects responses asynchronously.
    pub async fn request_verification(&self, cid_hex: &str, source_text: &str) {
        let addrs = {
            let state = self.shared.lock().await;
            state.peer_manager.known_addrs()
        };

        if addrs.is_empty() {
            return;
        }

        let event_tx = self.event_tx.clone();

        for addr in addrs {
            let msg = NetMessage::VerifyRequest {
                cid_hex: cid_hex.to_string(),
                source_text: source_text.to_string(),
            };
            let event_tx = event_tx.clone();
            let cid_hex = cid_hex.to_string();
            tokio::spawn(async move {
                if let Ok(mut stream) = TcpStream::connect(addr).await {
                    if send_message(&mut stream, &msg).await.is_ok() {
                        // Wait for response (with timeout)
                        match tokio::time::timeout(
                            std::time::Duration::from_secs(60),
                            recv_message(&mut stream),
                        )
                        .await
                        {
                            Ok(Ok(NetMessage::VerifyResponse {
                                agreement_score,
                                verified,
                                ..
                            })) => {
                                let _ = event_tx
                                    .send(NodeEvent::VerifyResult {
                                        cid_hex,
                                        agreement_score,
                                        verified,
                                        from: format!("{}", addr),
                                    })
                                    .await;
                            }
                            _ => {
                                let _ = event_tx
                                    .send(NodeEvent::Notification(format!(
                                        "  ⚠ Verification timeout from {}",
                                        addr
                                    )))
                                    .await;
                            }
                        }
                    }
                }
            });
        }
    }

    /// Address of the real UDP/QUIC OBP-RP listener, if it is running.
    #[cfg(feature = "vnext-network-runtime")]
    pub fn vnext_listener_addr(&self) -> Option<SocketAddr> {
        self.vnext_product_runtime
            .as_ref()
            .map(|runtime| runtime.services().local_addr())
    }

    /// Typed product façade for API/CLI/Desktop integration. Raw subsystem
    /// runtime references are deliberately not exposed.
    #[cfg(feature = "vnext-outbound-first")]
    pub fn obp_host(&self) -> Option<crate::vnext_product_runtime::obp::Host> {
        self.vnext_product_runtime.as_ref().map(|r| r.obp_host())
    }

    #[cfg(feature = "vnext-network-runtime")]
    pub fn vnext_product_services(&self) -> Option<VNextProductServices> {
        self.vnext_product_runtime
            .as_ref()
            .map(VNextProductRuntime::services)
    }

    #[cfg(feature = "vnext-network-runtime")]
    pub fn vnext_product_runtime_status(
        &self,
    ) -> Result<Option<VNextProductRuntimeStatus>, NodeError> {
        self.vnext_product_services()
            .map(|services| {
                services
                    .status()
                    .map_err(|error| NodeError::Network(error.to_string()))
            })
            .transpose()
    }

    /// Establish a transcript-authenticated outbound vNext session.
    #[cfg(feature = "vnext-network-runtime")]
    pub async fn connect_vnext_peer(
        &self,
        addr: SocketAddr,
    ) -> Result<OutboundVNextSession, NodeError> {
        self.vnext_product_services()
            .ok_or_else(|| NodeError::Network("authenticated OBP-RP runtime is not active".into()))?
            .connect_peer(addr)
            .await
            .map_err(|error| NodeError::Network(error.to_string()))
    }

    /// Persist an emergency kill for one vNext runtime lane. Legacy TCP and
    /// local/offline knowledge paths remain owned and available independently.
    #[cfg(feature = "vnext-network-runtime")]
    pub fn kill_vnext_runtime_lane(
        &self,
        lane: VNextRuntimeLane,
    ) -> Result<VNextRuntimeRolloutSnapshot, NodeError> {
        self.vnext_product_services()
            .ok_or_else(|| NodeError::Network("authenticated OBP-RP runtime is not active".into()))?
            .kill_runtime_lane(lane)
            .map_err(|error| NodeError::Network(error.to_string()))
    }

    /// Explicitly re-enable one configured lane on a later generation.
    #[cfg(feature = "vnext-network-runtime")]
    pub fn reenable_vnext_runtime_lane(
        &self,
        lane: VNextRuntimeLane,
    ) -> Result<VNextRuntimeRolloutSnapshot, NodeError> {
        self.vnext_product_services()
            .ok_or_else(|| NodeError::Network("authenticated OBP-RP runtime is not active".into()))?
            .reenable_runtime_lane(lane)
            .map_err(|error| NodeError::Network(error.to_string()))
    }

    /// Atomically disable every vNext network/product lane without deleting
    /// raw, journal, outbox, quarantine, wallet, or OBT state.
    #[cfg(feature = "vnext-network-runtime")]
    pub fn rollback_vnext_runtime(&self) -> Result<VNextRuntimeRolloutSnapshot, NodeError> {
        self.vnext_product_services()
            .ok_or_else(|| NodeError::Network("authenticated OBP-RP runtime is not active".into()))?
            .rollback_runtime()
            .map_err(|error| NodeError::Network(error.to_string()))
    }

    /// Get the listener address (if network is started).
    pub fn listener_addr(&self) -> Option<SocketAddr> {
        self.listener_addr
    }

    /// Drain pending events (non-blocking).
    pub fn drain_events(&mut self) -> Vec<NodeEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_rx.try_recv() {
            events.push(event);
        }
        events
    }

    /// Get peer count from shared state (non-blocking).
    pub fn peer_count(&self) -> usize {
        match self.shared.try_lock() {
            Ok(state) => state.peer_manager.peer_count(),
            Err(_) => 0,
        }
    }

    /// Get peer list snapshot (non-blocking).
    pub fn peer_list_snapshot(&self) -> Vec<PeerInfo> {
        match self.shared.try_lock() {
            Ok(state) => state.peer_manager.peer_list().to_vec(),
            Err(_) => Vec::new(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Background Listener
// ═══════════════════════════════════════════════════════════════════════════

/// Returned by [`OneBrainNode::start_network`] when nothing is configured.
const NO_NETWORK_LISTENER_REQUESTED: &str = "no network listener requested: the unauthenticated legacy TCP listener is disabled (set legacy_tcp_listener to loopback or all_interfaces to opt in) and the vNext OBP-RP runtime is not active";

/// Concurrent connections served by the opt-in legacy listener; excess
/// connections are closed immediately instead of spawning more tasks.
const LEGACY_MAX_CONNECTIONS: usize = 64;

/// Deadline for a legacy peer to deliver its single request message.
const LEGACY_FIRST_MESSAGE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Concurrent remote `VerifyRequest`s allowed to drive local Ollama calls.
const LEGACY_MAX_CONCURRENT_VERIFICATIONS: usize = 2;

/// Background TCP listener loop.
///
/// Accepts incoming connections and processes messages:
/// - PeerHello → register peer, send our hello back
/// - KuPush → store KU locally, index in retriever
/// - VerifyRequest → re-encode and send VerifyResponse
///
/// The protocol is unauthenticated, so the loop only runs after an explicit
/// [`NodeConfig::legacy_tcp_listener`] opt-in and bounds its own work.
async fn listener_loop(
    listener: TcpListener,
    shared: Arc<Mutex<SharedState>>,
    event_tx: mpsc::Sender<NodeEvent>,
) {
    let connections = Arc::new(tokio::sync::Semaphore::new(LEGACY_MAX_CONNECTIONS));
    let verifications = Arc::new(tokio::sync::Semaphore::new(
        LEGACY_MAX_CONCURRENT_VERIFICATIONS,
    ));
    loop {
        match listener.accept().await {
            Ok((stream, peer_addr)) => {
                let Ok(permit) = Arc::clone(&connections).try_acquire_owned() else {
                    // Over capacity: close without reading or spawning.
                    drop(stream);
                    continue;
                };
                let shared = Arc::clone(&shared);
                let event_tx = event_tx.clone();
                let verifications = Arc::clone(&verifications);
                tokio::spawn(async move {
                    let _permit = permit;
                    handle_connection(stream, peer_addr, shared, event_tx, verifications).await;
                });
            }
            Err(e) => {
                eprintln!("  ⚠ Listener accept error: {}", e);
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

/// Handle a single incoming TCP connection.
async fn handle_connection(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    shared: Arc<Mutex<SharedState>>,
    event_tx: mpsc::Sender<NodeEvent>,
    verifications: Arc<tokio::sync::Semaphore>,
) {
    // Read the incoming message
    let msg =
        match tokio::time::timeout(LEGACY_FIRST_MESSAGE_TIMEOUT, recv_message(&mut stream)).await {
            Ok(Ok(m)) => m,
            Ok(Err(e)) => {
                // Connection closed or error — silently ignore
                if e.kind() != std::io::ErrorKind::UnexpectedEof {
                    eprintln!("  ⚠ Read error from {}: {}", peer_addr, e);
                }
                return;
            }
            Err(_) => return,
        };

    match msg {
        NetMessage::PeerHello {
            name,
            port: _,
            ku_count,
        } => {
            let peer_info = PeerInfo {
                name: name.clone(),
                addr: peer_addr,
                ku_count,
            };

            // Send our hello back
            let (our_name, our_port, our_ku_count) = {
                let state = shared.lock().await;
                let count = state.storage.count().unwrap_or(0) as u64;
                (state.config.name.clone(), state.config.port, count)
            };
            let our_hello = NetMessage::PeerHello {
                name: our_name,
                port: our_port,
                ku_count: our_ku_count,
            };
            let _ = send_message(&mut stream, &our_hello).await;

            // Register peer
            {
                let mut state = shared.lock().await;
                state.peer_manager.add_peer(peer_info.clone());
            }

            let _ = event_tx.send(NodeEvent::PeerConnected(peer_info)).await;
        }

        NetMessage::KuPush {
            cid_hex: _peer_claimed_cid,
            wire_bytes,
            source_text,
        } => {
            // Decode and store the KU
            match KuRuntime::from_wire(wire_bytes.clone()) {
                Ok(ku) => {
                    let state = shared.lock().await;
                    match state.storage.put(&ku) {
                        Ok(actual_cid) => {
                            let verified_cid_hex = hex_cid(&actual_cid);
                            let _ = event_tx
                                .send(NodeEvent::KuReceived {
                                    cid_hex: verified_cid_hex,
                                    wire_bytes,
                                    source_text,
                                    from: format!("{}", peer_addr),
                                })
                                .await;
                        }
                        Err(e) => {
                            let _ = event_tx
                                .send(NodeEvent::Notification(format!(
                                    "  ⚠ Failed to store KU from {}: {}",
                                    peer_addr, e
                                )))
                                .await;
                        }
                    }
                }
                Err(e) => {
                    let _ = event_tx
                        .send(NodeEvent::Notification(format!(
                            "  ⚠ Failed to decode KU from {}: {}",
                            peer_addr, e
                        )))
                        .await;
                }
            }
        }

        NetMessage::VerifyRequest {
            cid_hex,
            source_text,
        } => {
            // Remote peers must not be able to queue unbounded model work.
            // Over capacity, close without a response; the requester sees EOF.
            let Ok(_verification) = verifications.try_acquire() else {
                return;
            };
            #[cfg(not(feature = "legacy"))]
            {
                let _ = source_text;
                let response = NetMessage::VerifyResponse {
                    cid_hex,
                    agreement_score: 0.0,
                    verified: false,
                };
                let _ = send_message(&mut stream, &response).await;
            }
            #[cfg(feature = "legacy")]
            {
                // Get config for Ollama
                let (ollama_url, model, _original_wire) = {
                    let state = shared.lock().await;
                    (
                        state.config.ollama_url.clone(),
                        state.config.model.clone(),
                        None::<Vec<u8>>,
                    )
                };

                let result =
                    verifier_service::verify_ku(&source_text, &[], &ollama_url, &model).await;

                // Send response
                let response = NetMessage::VerifyResponse {
                    cid_hex: cid_hex.clone(),
                    agreement_score: result.agreement_score,
                    verified: result.verified,
                };
                let _ = send_message(&mut stream, &response).await;

                let _ = event_tx
                    .send(NodeEvent::Notification(format!(
                        "  📋 Verified KU {} for {} (score: {:.0}%)",
                        &cid_hex[..8],
                        peer_addr,
                        result.agreement_score * 100.0
                    )))
                    .await;
            }
        }

        NetMessage::VerifyResponse {
            cid_hex,
            agreement_score,
            verified,
        } => {
            let _ = event_tx
                .send(NodeEvent::VerifyResult {
                    cid_hex,
                    agreement_score,
                    verified,
                    from: format!("{}", peer_addr),
                })
                .await;
        }

        NetMessage::PeerList { peers } => {
            let mut state = shared.lock().await;
            for addr in peers {
                // Add as unnamed peer (we'll get their name on hello)
                let info = PeerInfo {
                    name: format!("peer@{}", addr),
                    addr,
                    ku_count: 0,
                };
                state.peer_manager.add_peer(info);
            }
        }
    }
}
