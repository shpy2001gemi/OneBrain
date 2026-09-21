//! Draft jobs share the existing encrypted journal and Base lifecycle owner.
use super::*;
use ku_encoder::extraction::review_draft::DraftJob;
use redb::ReadableTableMetadata;
use serde_json::json;
use std::time::{Duration, Instant};

const REVIEW: TableDefinition<&[u8], &[u8]> = TableDefinition::new("ku_private_review_drafts_v1");
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    #[serde(default)]
    created_ms: u64,
    principal: [u8; 32],
    dataset: [u8; 32],
    operation: [u8; 32],
    process: [u8; 32],
    source: String,
    model: String,
    producer: String,
    job: DraftJob,
}
impl KuStore {
    pub(crate) fn review_list(
        &self,
        principal: [u8; 32],
        budget: &ResourceBudgetV1,
    ) -> Result<crate::ku_manual::ManualEditorResponse, BaseServiceError> {
        let tx = self.journal.begin_read().map_err(|_| corrupt())?;
        let table = tx.open_table(REVIEW).map_err(|_| corrupt())?;
        let mut records = Vec::new();
        for row in table.iter().map_err(|_| corrupt())? {
            let (k, v) = row.map_err(|_| corrupt())?;
            let record =
                self.decode_review(k.value().try_into().map_err(|_| corrupt())?, v.value())?;
            if record.principal == principal {
                records.push(record);
            }
        }
        records.sort_by_key(|r| std::cmp::Reverse(r.created_ms));
        records.truncate((budget.max_items as usize).min(16));
        Ok(crate::ku_manual::ManualEditorResponse::ReviewList{review_jobs:records.into_iter().map(|r|json!({"operation_id":hex(&r.operation),"model":r.model,"state":r.job.state,"created_ms":r.created_ms})).collect()})
    }
    fn review_key(&self, operation: [u8; 32]) -> [u8; 32] {
        let mut h = blake3::Hasher::new_derive_key("onebrain:ku:private-review-draft:1");
        h.update(&self.dataset.0);
        h.update(&operation);
        *h.finalize().as_bytes()
    }
    fn decode_review(&self, id: [u8; 32], sealed: &[u8]) -> Result<Record, BaseServiceError> {
        let plain = zeroize::Zeroizing::new(
            self.vault
                .open_local_metadata(self.review_key(id), sealed)
                .map_err(|_| corrupt())?,
        );
        if plain.len() > 1_048_576 {
            return Err(corrupt());
        }
        let record: Record = serde_json::from_slice(&plain).map_err(|_| corrupt())?;
        if record.operation != id || record.dataset != self.dataset.0 {
            return Err(corrupt());
        }
        Ok(record)
    }
    fn read_review(&self, principal: [u8; 32], id: [u8; 32]) -> Result<Record, BaseServiceError> {
        let tx = self.journal.begin_read().map_err(|_| corrupt())?;
        let table = tx.open_table(REVIEW).map_err(|_| corrupt())?;
        let value = table
            .get(id.as_slice())
            .map_err(|_| corrupt())?
            .ok_or_else(not_found)?;
        let record = self.decode_review(id, value.value())?;
        if record.principal != principal {
            return Err(not_found());
        }
        Ok(record)
    }
    fn write_review(&self, record: &Record) -> Result<(), BaseServiceError> {
        let plain = zeroize::Zeroizing::new(serde_json::to_vec(record).map_err(|_| corrupt())?);
        if plain.len() > 1_048_576 {
            return Err(resource());
        }
        let sealed = self
            .vault
            .seal_local_metadata(self.review_key(record.operation), &plain)
            .map_err(|_| corrupt())?;
        let tx = self.journal.begin_write().map_err(|_| corrupt())?;
        {
            let mut table = tx.open_table(REVIEW).map_err(|_| corrupt())?;
            let old = table
                .get(record.operation.as_slice())
                .map_err(|_| corrupt())?
                .map(|v| v.value().len());
            if old.is_none() && table.len().map_err(|_| corrupt())? >= 256 {
                return Err(resource());
            }
            let mut total = sealed.len();
            for row in table.iter().map_err(|_| corrupt())? {
                let (k, v) = row.map_err(|_| corrupt())?;
                if k.value() != record.operation {
                    total += v.value().len();
                }
            }
            if total > 16 * 1024 * 1024 {
                return Err(resource());
            }
            table
                .insert(record.operation.as_slice(), sealed.as_slice())
                .map_err(|_| corrupt())?;
        }
        tx.commit().map_err(|_| corrupt())
    }
    pub(super) fn recover_review_jobs(&self) -> Result<(), BaseServiceError> {
        let tx = self.journal.begin_write().map_err(|_| corrupt())?;
        {
            tx.open_table(REVIEW).map_err(|_| corrupt())?;
        }
        tx.commit().map_err(|_| corrupt())?;
        let records = {
            let tx = self.journal.begin_read().map_err(|_| corrupt())?;
            let table = tx.open_table(REVIEW).map_err(|_| corrupt())?;
            if table.len().map_err(|_| corrupt())? > 256 {
                return Err(corrupt());
            }
            let mut records = Vec::new();
            let mut total = 0;
            for row in table.iter().map_err(|_| corrupt())? {
                let (k, v) = row.map_err(|_| corrupt())?;
                total += v.value().len();
                if total > 16 * 1024 * 1024 {
                    return Err(corrupt());
                }
                records.push(
                    self.decode_review(k.value().try_into().map_err(|_| corrupt())?, v.value())?,
                );
            }
            records
        };
        for mut record in records {
            record.job.interrupt();
            self.write_review(&record)?;
        }
        Ok(())
    }
    pub(crate) fn review_get(
        &self,
        principal: [u8; 32],
        id: [u8; 32],
    ) -> Result<crate::ku_manual::ManualEditorResponse, BaseServiceError> {
        let record = self.read_review(principal, id)?;
        Ok(crate::ku_manual::ManualEditorResponse::Review {
            review_job: json!({"operation_id":hex(&id),"source":record.source,"model":record.model,"job":record.job,"canonical_ku":false,"semantic_verification":"unassessed","factual_verification":"unassessed","limitations":["local_only","experimental_model_unqualified","draft_is_not_ku"]}),
        })
    }
    pub(crate) fn review_start(
        &self,
        principal: [u8; 32],
        input: crate::ku_ollama::TextIntake,
        budget: &ResourceBudgetV1,
    ) -> Result<bool, BaseServiceError> {
        let id = input.operation_id.0;
        let provider = self
            .inputs
            .review_provider(&input.model)
            .ok_or_else(unavailable)?;
        let producer = ku_encoder::extraction::artifact_sha256(provider.manifest())
            .map_err(|_| unavailable())?;
        self.editor(
            principal,
            crate::ku_manual::ManualEditorRequest::EncodeText(input.clone()),
            budget,
        )?;
        let _lock = self.mutation.lock().map_err(|_| corrupt())?;
        let existing = {
            let tx = self.journal.begin_read().map_err(|_| corrupt())?;
            let table = tx.open_table(REVIEW).map_err(|_| corrupt())?;
            table.get(id.as_slice()).map_err(|_| corrupt())?.is_some()
        };
        if existing {
            let old = self.read_review(principal, id)?;
            if old.source != input.text || old.model != input.model {
                return Err(conflict());
            }
            return Ok(false);
        }
        let record = Record {
            created_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| corrupt())?
                .as_millis() as u64,
            principal,
            dataset: self.dataset.0,
            process: self.process,
            operation: id,
            source: input.text.clone(),
            model: input.model,
            producer,
            job: if self.inputs.review_selection_v2() {
                DraftJob::new_selection_v2(&input.text)
            } else { DraftJob::new_selection(&input.text) }.map_err(|_| invalid())?,
        };
        self.write_review(&record)?;
        Ok(true)
    }
    pub(crate) fn review_resume(
        &self,
        principal: [u8; 32],
        id: [u8; 32],
    ) -> Result<(), BaseServiceError> {
        let _lock = self.mutation.lock().map_err(|_| corrupt())?;
        if self
            .review_running
            .lock()
            .map_err(|_| corrupt())?
            .contains_key(&id)
        {
            return Err(conflict());
        }
        let mut r = self.read_review(principal, id)?;
        if r.job.state != "interrupted" && r.job.state != "queued" {
            return Err(conflict());
        }
        if r.job.commitment != r.job.expected_commitment().map_err(|_| unavailable())? {
            return Err(unavailable());
        }
        let provider = self
            .inputs
            .review_provider(&r.model)
            .ok_or_else(unavailable)?;
        if ku_encoder::extraction::artifact_sha256(provider.manifest())
            .map_err(|_| unavailable())?
            != r.producer
        {
            return Err(unavailable());
        }
        r.process = self.process;
        r.job.state = "queued".into();
        self.write_review(&r)
    }
    pub(crate) fn review_cancel(
        &self,
        principal: [u8; 32],
        id: [u8; 32],
    ) -> Result<(), BaseServiceError> {
        let _lock = self.mutation.lock().map_err(|_| corrupt())?;
        self.cancel_review_locked(principal, id)
    }
    fn cancel_review_locked(
        &self,
        principal: [u8; 32],
        id: [u8; 32],
    ) -> Result<(), BaseServiceError> {
        let mut r = self.read_review(principal, id)?;
        if let Some(cancel) = self.review_running.lock().map_err(|_| corrupt())?.get(&id) {
            cancel.store(true, Ordering::Release);
        }
        r.job.state = "canceled".into();
        r.job.active = None;
        self.write_review(&r)
    }
    // Called only while the shared mutation lock is held by operation cancel.
    pub(super) fn cancel_review_if_present(
        &self,
        principal: [u8; 32],
        id: [u8; 32],
    ) -> Result<(), BaseServiceError> {
        let exists = {
            let tx = self.journal.begin_read().map_err(|_| corrupt())?;
            let table = tx.open_table(REVIEW).map_err(|_| corrupt())?;
            table.get(id.as_slice()).map_err(|_| corrupt())?.is_some()
        };
        if exists {
            self.cancel_review_locked(principal, id)?;
        }
        Ok(())
    }
    pub(crate) fn review_admit(&self, id: [u8; 32]) -> Result<Arc<AtomicBool>, BaseServiceError> {
        let mut active = self.review_running.lock().map_err(|_| corrupt())?;
        if active.contains_key(&id) || active.len() >= 16 {
            return Err(resource());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        active.insert(id, cancel.clone());
        Ok(cancel)
    }
    pub(crate) fn interrupt_reviews(&self) {
        if let Ok(active) = self.review_running.lock() {
            for cancel in active.values() {
                cancel.store(true, Ordering::Release);
            }
        }
    }
    fn stop_review_at_bound(
        &self,
        principal: [u8; 32],
        id: [u8; 32],
        issue: &str,
    ) -> Result<(), BaseServiceError> {
        let _lock = self.mutation.lock().map_err(|_| corrupt())?;
        let mut record = self.read_review(principal, id)?;
        if record.job.state == "canceled" {
            return Ok(());
        }
        record.job.active = None;
        record.job.state = "needs_review".into();
        if record.job.issues.len() < 16 {
            record.job.issues.push(issue.into());
        }
        for window in &mut record.job.windows {
            if matches!(window.state.as_str(), "queued" | "reviewing" | "repairing") {
                window.state = "needs_review".into();
            }
        }
        self.write_review(&record)
    }
    pub(crate) async fn run_review(
        &self,
        principal: [u8; 32],
        id: [u8; 32],
        cancel: Arc<AtomicBool>,
    ) {
        let result = self.run_review_inner(principal, id, cancel.clone()).await;
        if result.is_err() {
            if let Ok(_lock) = self.mutation.lock() {
                if let Ok(mut record) = self.read_review(principal, id) {
                    if record.job.state != "canceled" {
                        record.job.interrupt();
                        record.job.state = "interrupted".into();
                        let _ = self.write_review(&record);
                    }
                }
            }
        }
        if let Ok(mut active) = self.review_running.lock() {
            active.remove(&id);
        }
    }
    async fn run_review_inner(
        &self,
        principal: [u8; 32],
        id: [u8; 32],
        cancel: Arc<AtomicBool>,
    ) -> Result<(), BaseServiceError> {
        let canceled = async {
            loop {
                if cancel.load(Ordering::Acquire) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        };
        tokio::pin!(canceled);
        let _permit = tokio::select! {p=self.review_gate.acquire()=>p.map_err(|_|unavailable())?,_=&mut canceled=>return Err(conflict())};
        loop {
            if cancel.load(Ordering::Acquire) {
                return Err(conflict());
            }
            let mut record = self.read_review(principal, id)?;
            if record.job.state == "canceled" {
                return Ok(());
            }
            let provider = self
                .inputs
                .review_provider(&record.model)
                .ok_or_else(unavailable)?;
            if ku_encoder::extraction::artifact_sha256(provider.manifest())
                .map_err(|_| unavailable())?
                != record.producer
            {
                return Err(unavailable());
            }
            let next = match record.job.next(&record.source) {
                Ok(next) => next,
                Err(error) => return self.stop_review_at_bound(principal, id, error.0),
            };
            let Some((window, request)) = next else {
                return Ok(());
            };
            let tokens = provider
                .review_task_tokens(&request)
                .map_err(|_| unavailable())?;
            let timeout = request.deadline;
            if let Err(error) = record.job.reserve(window, &request, tokens) {
                return self.stop_review_at_bound(principal, id, error.0);
            }
            {
                let _lock = self.mutation.lock().map_err(|_| corrupt())?;
                if cancel.load(Ordering::Acquire)
                    || self.read_review(principal, id)?.job.state == "canceled"
                {
                    return Err(conflict());
                }
                self.write_review(&record)?;
            }
            let started = Instant::now();
            let result = tokio::select! {
                result=tokio::time::timeout(timeout,provider.review_task(request))=>result.unwrap_or(Err(ku_encoder::extraction::ExtractionError("deadline"))),
                _=&mut canceled=>return Err(conflict())
            };
            let _lock = self.mutation.lock().map_err(|_| corrupt())?;
            if cancel.load(Ordering::Acquire)
                || self.read_review(principal, id)?.job.state == "canceled"
            {
                return Err(conflict());
            }
            let mut budget = ku_encoder::extraction::WorkBudget::new(
                1_000_000,
                Duration::from_secs(30),
                cancel.clone(),
            )
            .map_err(|_| resource())?;
            record
                .job
                .finish(
                    &record.source,
                    result,
                    started.elapsed().as_millis() as u64,
                    &record.producer,
                    &mut budget,
                )
                .map_err(|_| invalid())?;
            self.write_review(&record)?;
        }
    }
}
