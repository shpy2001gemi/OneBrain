//! Encode-and-store pipeline for [`OneBrainNode`]: rate limit, AI encode, quality
//! gates, persistent storage, retriever indexing and peer broadcast. In the default
//! (non-`legacy`) build the pipeline reports that legacy Core DNA encoding is
//! unavailable; user-facing KU creation goes through the KU workspace.

use super::*;

impl OneBrainNode {
    /// Encode text into a KU and store it persistently.
    ///
    /// Pipeline:
    /// 1. Check rate limit (anti-gaming)
    /// 2. Create a fresh AiEncoder + OllamaBackend
    /// 3. Encode via AI → EncodingResult with wire_bytes
    /// 4. For the primary KU:
    ///    a. Decode wire bytes → KuRuntime
    ///    b. Check quality gates (anti-gaming)
    ///    c. Store in redb via KuStorage
    ///    d. Index source text in retriever
    ///    e. Record creation in rate tracker
    /// 5. Broadcast to peers + request verification
    /// 6. Return CID + stats
    pub async fn encode_and_store(&mut self, text: &str) -> Result<EncodeStoreResult, NodeError> {
        self.encode_and_store_with_progress(text, None).await
    }

    /// Encode with optional broadcast progress sender.
    /// When `progress_tx` is Some, progress events are sent directly to the
    /// broadcast channel (bypassing the node's mpsc event channel and avoiding
    /// the lock deadlock issue with WS handlers).
    pub async fn encode_and_store_with_progress(
        &mut self,
        text: &str,
        progress_tx: Option<&tokio::sync::broadcast::Sender<String>>,
    ) -> Result<EncodeStoreResult, NodeError> {
        #[cfg(not(feature = "legacy"))]
        {
            let _ = (text, progress_tx);
            return Err(NodeError::InvalidArgument(
                "Legacy Core DNA encoding (/api/encode) is disabled in the default build; use the canonical vNext KU workflow (/api/vnext/ku or `onebrain ku`) or build with `--features legacy`."
                    .into(),
            ));
        }

        #[cfg(feature = "legacy")]
        {
            let total_steps: u8 = 6;

            // Helper to send progress
            let send_progress = |step: u8, msg: String| {
                if let Some(tx) = progress_tx {
                    let event = serde_json::json!({
                        "event_type": "encode_progress",
                        "timestamp": std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default().as_secs(),
                        "data": { "step": step, "total_steps": total_steps, "message": msg }
                    });
                    let _ = tx.send(event.to_string());
                }
            };

            // 1. Rate limit check
            send_progress(1, "Rate limit check...".into());
            let _ = self
                .event_tx
                .send(NodeEvent::EncodeProgress {
                    step: 1,
                    total_steps,
                    message: "Rate limit check...".into(),
                })
                .await;
            self.guard.check_rate_limit().map_err(NodeError::Pipeline)?;

            // 2. Create a fresh encoder backend (OllamaBackend doesn't impl Clone)
            send_progress(
                2,
                format!("Creating AI encoder (model: {})...", self.config.model),
            );
            let _ = self
                .event_tx
                .send(NodeEvent::EncodeProgress {
                    step: 2,
                    total_steps,
                    message: format!("Creating AI encoder (model: {})...", self.config.model),
                })
                .await;
            let encoder_backend =
                model_host::model_backend(&self.config.ollama_url, &self.config.model, 300)?;

            let encoder =
                AiEncoder::new(encoder_backend, self.dict.clone(), EncoderConfig::default());

            // 3. Encode via AI (this is the slow step)
            send_progress(
                3,
                "AI generating tool calls (this may take a while)...".into(),
            );
            let _ = self
                .event_tx
                .send(NodeEvent::EncodeProgress {
                    step: 3,
                    total_steps,
                    message: "AI generating tool calls (this may take a while)...".into(),
                })
                .await;

            // Smart dispatch: v2 if ConceptRegistry available, v1 fallback
            let encoding_result: EncodingResult = if let Some(ref reg) = self.registry {
                encoder
                    .encode_v2(text, reg.as_ref())
                    .await
                    .map_err(NodeError::Encoder)?
            } else {
                encoder.encode(text).await.map_err(NodeError::Encoder)?
            };

            if encoding_result.wire_bytes.is_empty() {
                return Err(NodeError::Pipeline("Encoding produced no KUs".into()));
            }

            // 4. Process the first (primary) KU
            send_progress(
                4,
                format!(
                    "Processing KU ({} bytes wire data)...",
                    encoding_result.wire_bytes[0].len()
                ),
            );
            let _ = self
                .event_tx
                .send(NodeEvent::EncodeProgress {
                    step: 4,
                    total_steps,
                    message: format!(
                        "Processing KU ({} bytes wire data)...",
                        encoding_result.wire_bytes[0].len()
                    ),
                })
                .await;
            let wire_bytes = &encoding_result.wire_bytes[0];

            // 4a. Decode wire bytes → KuRuntime
            let ku = KuRuntime::from_wire(wire_bytes.clone())
                .map_err(|e| NodeError::Pipeline(format!("KuRuntime decode failed: {}", e)))?;

            let instruction_count = ku.dna.instructions.len();

            // 4b. Quality gate check
            self.guard
                .check_quality(wire_bytes, instruction_count)
                .map_err(NodeError::Pipeline)?;

            // 4c-4e. Store, index, record (using shared state)
            send_progress(5, "Storing KU and indexing...".into());
            let _ = self
                .event_tx
                .send(NodeEvent::EncodeProgress {
                    step: 5,
                    total_steps,
                    message: "Storing KU and indexing...".into(),
                })
                .await;
            let cid;
            let cid_hex;
            {
                let state = self.shared.lock().await;

                // 4c. Store in redb
                cid = state
                    .storage
                    .put(&ku)
                    .map_err(|e| NodeError::Storage(format!("{}", e)))?;

                // Legacy KU persistence is fenced in Base mode. It never grants
                // retriever authority; only a proven Vault source binding may do so.
                cid_hex = hex_cid(&cid);
            }

            // 4e. Record creation in rate tracker
            self.guard.record_creation();

            // 5. Also process any additional KUs (if encoding produced multiple)
            {
                let state = self.shared.lock().await;
                for extra_bytes in encoding_result.wire_bytes.iter().skip(1) {
                    if let Ok(extra_ku) = KuRuntime::from_wire(extra_bytes.clone()) {
                        let extra_instr = extra_ku.dna.instructions.len();
                        if self.guard.check_quality(extra_bytes, extra_instr).is_ok() {
                            let _ = state.storage.put(&extra_ku);
                        }
                    }
                }
            }

            // 6. Broadcast to peers + request verification
            send_progress(6, "Broadcasting to peers...".into());
            let _ = self
                .event_tx
                .send(NodeEvent::EncodeProgress {
                    step: 6,
                    total_steps,
                    message: "Broadcasting to peers...".into(),
                })
                .await;
            let peers_reached = self.broadcast_ku(&cid_hex, wire_bytes, text).await;
            self.request_verification(&cid_hex, text).await;

            Ok(EncodeStoreResult {
                cid,
                wire_size: wire_bytes.len(),
                instruction_count,
                gene_type: encoding_result.gene_type,
                confidence: encoding_result.confidence,
                source_text: text.to_string(),
                wire_bytes: wire_bytes.clone(),
                peers_reached,
            })
        }
    }
}
