//! # OBP Node Runtime
//!
//! Unified node that integrates all OBP subsystems:
//! - Identity (Ed25519 keypair + NodeId)
//! - DHT routing table
//! - Stigmergy pheromone routing
//! - Vacuum filter for KU advertisement
//! - Pub/Sub topic subscriptions
//! - CRDT sync manager
//! - KQL executor for local queries

use ku_core::crdt::*;
use ku_core::*;
use ku_kql::executor::LocalExecutor;
use ku_kql::parser::parse_query;
use ku_net::dht::{DhtNode, InsertResult, KBucketEntry};
use ku_net::identity::*;
use ku_net::messages::NetworkAddress;
use ku_net::pubsub::PubSubManager;
use ku_net::stigmergy::PheromoneTable;
use ku_net::sync::SyncManager;
use ku_net::vacuum::VacuumFilter;

use std::time::Instant;

/// A complete OBP node with all subsystems integrated.
#[allow(dead_code)]
pub struct OBPNode {
    /// Node name (for display).
    pub name: String,
    /// Cryptographic identity.
    pub keypair: KeyPair,
    /// Node ID (BLAKE3 crypto puzzle).
    pub node_id: NodeId,
    /// Network address.
    pub address: NetworkAddress,
    /// DHT routing + local KV store.
    pub dht: DhtNode,
    /// Pheromone routing table.
    pub pheromones: PheromoneTable,
    /// Vacuum filter (advertise owned KUs).
    pub vacuum: VacuumFilter,
    /// Topic subscriptions.
    pub pubsub: PubSubManager,
    /// CRDT sync manager.
    pub sync: SyncManager,
    /// KQL query executor.
    pub executor: LocalExecutor,
    /// Trust CRDT (PN-Counter per KU CID).
    pub trust_crdts: std::collections::HashMap<[u8; 32], PNCounter>,
    /// Stats.
    pub stats: NodeStats,
}

/// Runtime statistics for a node.
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct NodeStats {
    pub kus_created: u64,
    pub kus_received: u64,
    pub queries_executed: u64,
    pub syncs_completed: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
}

impl OBPNode {
    /// Create a new OBP node.
    pub fn new(name: &str, ip: [u8; 4], port: u16) -> Self {
        let keypair = KeyPair::generate();
        let proof = generate_node_id(&keypair.pubkey_bytes(), PUZZLE_C_SMALL);
        let node_id = proof.node_id;
        let address = NetworkAddress::new_v4(ip[0], ip[1], ip[2], ip[3], port);

        Self {
            name: name.to_string(),
            keypair,
            node_id,
            address,
            dht: DhtNode::new(node_id),
            pheromones: PheromoneTable::new(),
            vacuum: VacuumFilter::with_defaults(1000),
            pubsub: PubSubManager::new(),
            sync: SyncManager::new(node_id),
            executor: LocalExecutor::new(),
            trust_crdts: std::collections::HashMap::new(),
            stats: NodeStats::default(),
        }
    }

    /// Create a KU and store it locally.
    ///
    /// Accepts a legacy KnowledgeUnit, converts to KuRuntime (Core DNA),
    /// and stores it in all subsystems.
    pub fn create_ku(&mut self, ku: KnowledgeUnit) -> [u8; 32] {
        // Convert legacy KU → Core DNA → KuRuntime
        let core_dna = ku_core::core_dna::ku_to_core_dna(&ku).expect("convert to Core DNA");
        let mut runtime = KuRuntime::from_dna(core_dna).expect("create KuRuntime");

        // Carry over trust/epigenetics from legacy KU
        if let Some(trust) = ku.trust {
            runtime.epi.trust = trust;
        }
        if let Some(epigenetic) = ku.epigenetic {
            runtime.epi.epigenetic = Some(epigenetic);
        }
        if let Some(es) = ku.epistemic_status {
            runtime.epi.trust.epistemic_status = es;
        }
        if let Some(et) = ku.evidence_type {
            runtime.epi.trust.evidence_type = et;
        }

        let cid = runtime.cid;

        // Store in DHT
        self.dht.store(cid, runtime.wire_bytes.clone()).ok();

        // Store in sync manager
        self.sync.store_local(cid, runtime.wire_bytes.clone());

        // Add to vacuum filter
        self.vacuum.insert(&cid);

        // Add to KQL executor
        self.executor.insert(runtime);

        // Initialize trust CRDT
        self.trust_crdts.insert(cid, PNCounter::new());

        self.stats.kus_created += 1;
        cid
    }

    /// Register a peer node in our routing table.
    pub fn add_peer(&mut self, peer: &OBPNode) -> InsertResult {
        let entry = KBucketEntry {
            node_id: peer.node_id,
            address: peer.address,
            last_seen: Instant::now(),
            rtt_ms: 50,
            stale_count: 0,
        };
        self.dht.routing_table.insert(entry)
    }

    /// Execute a KQL query locally.
    pub fn query(&mut self, kql: &str) -> Result<Vec<KuRuntime>, String> {
        let ast = parse_query(kql).map_err(|e| e.to_string())?;
        let result = self.executor.execute(&ast).map_err(|e| format!("{}", e))?;
        self.stats.queries_executed += 1;
        Ok(result.rows)
    }

    /// Sync with a peer — exchange deltas bidirectionally.
    pub fn sync_with(&mut self, peer: &mut OBPNode) -> SyncReport {
        // Us → Peer
        let our_request = self.sync.create_sync_request();
        let peer_response = peer.sync.handle_sync_request(&our_request);
        let received = self.sync.apply_sync_response(&peer_response);

        // Peer → Us
        let peer_request = peer.sync.create_sync_request();
        let our_response = self.sync.handle_sync_request(&peer_request);
        let peer_received = peer.sync.apply_sync_response(&our_response);

        self.stats.syncs_completed += 1;
        peer.stats.syncs_completed += 1;
        self.stats.kus_received += received.len() as u64;
        peer.stats.kus_received += peer_received.len() as u64;

        SyncReport {
            deltas_to_us: received.len(),
            deltas_to_peer: peer_received.len(),
        }
    }

    /// Corroborate a KU (increase trust).
    pub fn corroborate(&mut self, cid: &[u8; 32]) {
        let node_num = u64::from_be_bytes(self.node_id.0[0..8].try_into().unwrap());
        if let Some(counter) = self.trust_crdts.get_mut(cid) {
            counter.increment(node_num);
        }
    }

    /// Challenge a KU (decrease trust).
    pub fn challenge(&mut self, cid: &[u8; 32]) {
        let node_num = u64::from_be_bytes(self.node_id.0[0..8].try_into().unwrap());
        if let Some(counter) = self.trust_crdts.get_mut(cid) {
            counter.decrement(node_num);
        }
    }

    /// Get trust score for a KU.
    pub fn trust_score(&self, cid: &[u8; 32]) -> i64 {
        self.trust_crdts.get(cid).map(|c| c.value()).unwrap_or(0)
    }

    /// Subscribe to a topic domain code.
    pub fn subscribe_topic(&mut self, domain_code: u16) {
        self.pubsub.subscribe(domain_code);
    }

    /// Summary string for display.
    pub fn summary(&self) -> String {
        format!(
            "[{}] KUs:{} DHT:{} Pheromones:{} Syncs:{}",
            self.name,
            self.executor.count(),
            self.dht.routing_table.total_entries(),
            self.pheromones.topic_count(),
            self.stats.syncs_completed,
        )
    }
}

/// Report from a sync operation.
#[derive(Debug)]
pub struct SyncReport {
    pub deltas_to_us: usize,
    pub deltas_to_peer: usize,
}
