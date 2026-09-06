use super::*;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::{atomic::AtomicUsize, Mutex};

fn row() -> Value {
    serde_json::from_str::<Value>(include_str!(
        "../../../../docs/specs/vnext/ku-encoder-v1/corpus.json"
    ))
    .unwrap()["cases"][0]
        .clone()
}
fn job(row: &Value) -> ExtractionJob {
    ExtractionJob {
        principal: [1; 32],
        operation: [2; 32],
        process: [3; 32],
        dataset: [4; 32],
        contexts: vec![row["context"].clone()],
    }
}
struct Authority {
    resolution: Value,
    allowed: AtomicBool,
}
impl ExtractionAuthority for Authority {
    fn check_context(&self, _: &ExtractionJob, _: &Value, b: &mut WorkBudget) -> Result<()> {
        b.charge(1)?;
        require(self.allowed.load(Ordering::Acquire), "source_revoked")
    }
    fn resolve(
        &self,
        _: &ExtractionJob,
        _: &Value,
        _: &Value,
        b: &mut WorkBudget,
    ) -> Result<Value> {
        b.charge(1)?;
        Ok(self.resolution.clone())
    }
    fn check_resolution(
        &self,
        _: &ExtractionJob,
        _: &Value,
        _: &Value,
        r: &Value,
        b: &mut WorkBudget,
    ) -> Result<()> {
        b.charge(1)?;
        require(*r == self.resolution, "resolution_authority")
    }
}
#[derive(Default)]
struct Journal {
    state: Mutex<Option<ExtractionCheckpoint>>,
    fail_reservation: AtomicBool,
    states: Mutex<Vec<String>>,
}
impl ExtractionJournal for Journal {
    fn load(&self, _: &ExtractionJob) -> Result<Option<ExtractionCheckpoint>> {
        Ok(self.state.lock().unwrap().clone())
    }
    fn store(&self, _: &ExtractionJob, c: &ExtractionCheckpoint) -> Result<()> {
        if self.fail_reservation.load(Ordering::Acquire) && c.phase == "extracting" {
            return Err(ExtractionError("journal_unavailable"));
        }
        self.states.lock().unwrap().push(c.phase.clone());
        *self.state.lock().unwrap() = Some(c.clone());
        Ok(())
    }
}
struct Provider {
    manifest: Value,
    candidate: Value,
    calls: AtomicUsize,
    invalid_first: bool,
    pending: bool,
    journal: Arc<Journal>,
}
#[async_trait]
impl ExtractionProvider for Provider {
    fn manifest(&self) -> &Value {
        &self.manifest
    }
    fn input_tokens(&self, _: &ProviderRequest) -> Result<u32> {
        Ok(100)
    }
    async fn extract(&self, request: ProviderRequest) -> Result<Vec<u8>> {
        let n = self.calls.fetch_add(1, Ordering::AcqRel);
        let state = self.journal.state.lock().unwrap().clone().unwrap();
        assert_eq!(state.phase, "extracting");
        assert_eq!(state.calls as usize, n + 1);
        assert!(request.input.get("source_text").is_none());
        if self.pending {
            std::future::pending::<()>().await;
        }
        if self.invalid_first && n == 0 {
            return Ok(b"{\"PRIVATE invalid".to_vec());
        }
        Ok(serde_json::to_vec(&self.candidate).unwrap())
    }
}
fn fixture(invalid_first: bool, pending: bool) -> (Value, Arc<Provider>, Arc<Journal>, Authority) {
    let row = row();
    let journal = Arc::new(Journal::default());
    let provider = Arc::new(Provider {
        manifest: json!({"profile":"ku-extraction-provider/1.0","provider_id":"fixture","backend_build_sha256":"aa".repeat(32),"mode":"json_schema","tools_enabled":false,"max_context_tokens":8192,"peak_bytes_reservation":1024,"schema_bundle_sha256":ExtractionWorkflow::bundle_hash(),"model_artifact_sha256":"bb".repeat(32),"tokenizer_sha256":"cc".repeat(32),"supported_schema_keywords":["type"],"temperature_milli":0}),
        candidate: row["candidate"].clone(),
        calls: AtomicUsize::new(0),
        invalid_first,
        pending,
        journal: journal.clone(),
    });
    let authority = Authority {
        resolution: row["resolution"].clone(),
        allowed: AtomicBool::new(true),
    };
    (row, provider, journal, authority)
}

#[tokio::test]
async fn reservation_precedes_call_and_replay_never_resamples() {
    let (row, p, j, a) = fixture(true, false);
    let w = ExtractionWorkflow::new(p.clone(), 1024).unwrap();
    let job = job(&row);
    let cancel = Arc::new(AtomicBool::new(false));
    let out = w
        .run(&job, &a, j.as_ref(), 1_000_000, cancel.clone())
        .await
        .unwrap();
    assert!(!out.needs_resolution);
    assert_eq!(out.drafts.len(), 1);
    assert_eq!(p.calls.load(Ordering::Acquire), 2);
    let repeated = w
        .run(&job, &a, j.as_ref(), 1_000_000, cancel.clone())
        .await
        .unwrap();
    assert_eq!(repeated.drafts, out.drafts);
    assert_eq!(p.calls.load(Ordering::Acquire), 2);
    a.allowed.store(false, Ordering::Release);
    assert!(matches!(
        w.run(&job, &a, j.as_ref(), 1_000_000, cancel).await,
        Err(ExtractionError("source_revoked"))
    ));
    assert_eq!(p.calls.load(Ordering::Acquire), 2);
}

#[tokio::test]
async fn failed_durable_reservation_cannot_dispatch_provider() {
    let (row, p, j, a) = fixture(false, false);
    j.fail_reservation.store(true, Ordering::Release);
    let w = ExtractionWorkflow::new(p.clone(), 1024).unwrap();
    assert!(w
        .run(
            &job(&row),
            &a,
            j.as_ref(),
            1_000_000,
            Arc::new(AtomicBool::new(false))
        )
        .await
        .is_err());
    assert_eq!(p.calls.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn canceled_provider_future_cannot_deliver_late_completion() {
    let (row, p, j, a) = fixture(false, true);
    let w = ExtractionWorkflow::new(p.clone(), 1024).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    let job = job(&row);
    let run = w.run(&job, &a, j.as_ref(), 1_000_000, cancel);
    let stop = async {
        tokio::time::sleep(Duration::from_millis(30)).await;
        signal.store(true, Ordering::Release);
    };
    let (result, _) = tokio::join!(run, stop);
    assert!(matches!(result, Err(ExtractionError("canceled"))));
    let state = j.state.lock().unwrap().clone().unwrap();
    assert_eq!(state.phase, "canceled");
    assert_eq!(state.calls, 1);
    assert!(state.candidates.is_empty());
}

#[tokio::test]
async fn process_change_interrupts_checkpoint_without_resampling() {
    let (row, p, j, a) = fixture(false, false);
    let w = ExtractionWorkflow::new(p.clone(), 1024).unwrap();
    let mut job = job(&row);
    w.run(
        &job,
        &a,
        j.as_ref(),
        1_000_000,
        Arc::new(AtomicBool::new(false)),
    )
    .await
    .unwrap();
    job.process = [8; 32];
    assert!(matches!(
        w.run(
            &job,
            &a,
            j.as_ref(),
            1_000_000,
            Arc::new(AtomicBool::new(false))
        )
        .await,
        Err(ExtractionError("interrupted"))
    ));
    assert_eq!(p.calls.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn memory_work_and_exhausted_repair_fail_with_finite_reserved_counters() {
    let (row, mut p, j, a) = fixture(false, false);
    assert!(matches!(
        ExtractionWorkflow::new(p.clone(), 1023),
        Err(ExtractionError("memory_admission"))
    ));
    Arc::get_mut(&mut p).unwrap().candidate = json!({"private_invalid":"payload"});
    let w = ExtractionWorkflow::new(p.clone(), 1024).unwrap();
    assert!(w
        .run(
            &job(&row),
            &a,
            j.as_ref(),
            1_000_000,
            Arc::new(AtomicBool::new(false))
        )
        .await
        .is_err());
    assert_eq!(p.calls.load(Ordering::Acquire), 2);
    let state = j.state.lock().unwrap().clone().unwrap();
    assert_eq!(state.calls, 2);
    assert_eq!(state.output_tokens, 4096);
    assert_eq!(state.phase, "failed");
    let (row, p, j, a) = fixture(false, false);
    let w = ExtractionWorkflow::new(p.clone(), 1024).unwrap();
    assert!(matches!(
        w.run(
            &job(&row),
            &a,
            j.as_ref(),
            1,
            Arc::new(AtomicBool::new(false))
        )
        .await,
        Err(ExtractionError("resource"))
    ));
    assert_eq!(p.calls.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn prior_elapsed_time_cannot_be_refunded_on_resume() {
    let (row, p, j, a) = fixture(false, false);
    let w = ExtractionWorkflow::new(p.clone(), 1024).unwrap();
    let job = job(&row);
    w.run(
        &job,
        &a,
        j.as_ref(),
        1_000_000,
        Arc::new(AtomicBool::new(false)),
    )
    .await
    .unwrap();
    j.state.lock().unwrap().as_mut().unwrap().elapsed_ms = 120_000;
    assert!(matches!(
        w.run(
            &job,
            &a,
            j.as_ref(),
            1_000_000,
            Arc::new(AtomicBool::new(false))
        )
        .await,
        Err(ExtractionError("resource"))
    ));
    assert_eq!(p.calls.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn pending_provider_hits_the_remaining_aggregate_deadline() {
    let (row, p, j, a) = fixture(false, true);
    let w = ExtractionWorkflow::new(p.clone(), 1024).unwrap();
    let started = Instant::now();
    let result = w
        .run_admitted(
            &job(&row),
            &a,
            j.as_ref(),
            1_000_000,
            0,
            119_900,
            Arc::new(AtomicBool::new(false)),
        )
        .await;
    assert!(matches!(result, Err(ExtractionError("deadline"))));
    assert!(started.elapsed() < Duration::from_secs(2));
    let state = j.state.lock().unwrap().clone().unwrap();
    assert_eq!(state.calls, 1);
    assert!(state.candidates.is_empty());
    assert_eq!(state.phase, "failed");
}

#[test]
fn manifest_rejects_duplicate_attempts_mixed_bindings_and_overlapping_scope() {
    let a = row()["context"].clone();
    let budget = || {
        WorkBudget::new(
            1_000_000,
            Duration::from_secs(1),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap()
    };
    assert_eq!(
        super::workflow::validate_manifest(&[a.clone(), a.clone()], &mut budget())
            .unwrap_err()
            .0,
        "duplicate_attempt"
    );
    let mut b = a.clone();
    b["attempt_id"] = "dd".repeat(32).into();
    assert_eq!(
        super::workflow::validate_manifest(&[a.clone(), b.clone()], &mut budget())
            .unwrap_err()
            .0,
        "overlapping_focus"
    );
    for field in [
        "source_ref",
        "source_text",
        "registry_root",
        "resource_profile",
    ] {
        let mut changed = b.clone();
        changed[field] = "different".into();
        assert_eq!(
            super::workflow::validate_manifest(&[a.clone(), changed], &mut budget())
                .unwrap_err()
                .0,
            "job_binding"
        );
    }
}

#[test]
fn assembler_preserves_order_and_rewrites_every_local_statement_reference() {
    use ku_core::foundation::semantic::{StatementId, TermRef};
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../../docs/specs/vnext/ku-encoder-v1/corpus.json"
    ))
    .unwrap();
    let row = corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| {
            r["expected"]["status"] == "compilable"
                && r["candidate"]["statements"].as_array().unwrap().len() > 1
        })
        .unwrap();
    let mut budget = WorkBudget::new(
        1_000_000,
        Duration::from_secs(1),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    let mut sem = super::compiler::compile(
        &row["context"],
        &row["candidate"],
        &row["resolution"],
        &mut budget,
    )
    .unwrap()
    .unwrap();
    let q = &mut sem.statements[1].qualifiers;
    q.condition = Some(StatementId(0));
    q.time = Some(TermRef::Statement(StatementId(0)));
    q.location = Some(TermRef::Statement(StatementId(0)));
    q.perspective = Some(TermRef::Statement(StatementId(0)));
    sem.statements[1]
        .arguments
        .push(TermRef::Statement(StatementId(0)));
    let count = sem.statements.len();
    let assembled = super::workflow::assemble(vec![sem.clone(), sem.clone()], &mut budget).unwrap();
    assert_eq!(&assembled.statements[..count], sem.statements.as_slice());
    let second = &assembled.statements[count + 1];
    let target = StatementId(count as u32);
    assert_eq!(second.statement_id, StatementId(count as u32 + 1));
    assert_eq!(second.qualifiers.condition, Some(target));
    for term in [
        &second.qualifiers.time,
        &second.qualifiers.location,
        &second.qualifiers.perspective,
    ] {
        assert_eq!(*term, Some(TermRef::Statement(target)));
    }
    assert_eq!(second.arguments.last(), Some(&TermRef::Statement(target)));
}

struct JobFixture {
    manifest: Value,
    rows: Vec<Value>,
}
#[async_trait]
impl ExtractionProvider for JobFixture {
    fn manifest(&self) -> &Value {
        &self.manifest
    }
    fn input_tokens(&self, _: &ProviderRequest) -> Result<u32> {
        Ok(100)
    }
    async fn extract(&self, request: ProviderRequest) -> Result<Vec<u8>> {
        let row = self
            .rows
            .iter()
            .find(|r| r["context"]["attempt_id"] == request.input["attempt_id"])
            .unwrap();
        Ok(serde_json::to_vec(&row["candidate"]).unwrap())
    }
}
impl ExtractionAuthority for JobFixture {
    fn check_context(&self, _: &ExtractionJob, c: &Value, b: &mut WorkBudget) -> Result<()> {
        b.charge(1)?;
        require(
            self.rows.iter().any(|r| r["context"] == *c),
            "context_authority",
        )
    }
    fn resolve(
        &self,
        _: &ExtractionJob,
        c: &Value,
        _: &Value,
        b: &mut WorkBudget,
    ) -> Result<Value> {
        b.charge(1)?;
        Ok(self.rows.iter().find(|r| r["context"] == *c).unwrap()["resolution"].clone())
    }
    fn check_resolution(
        &self,
        j: &ExtractionJob,
        c: &Value,
        v: &Value,
        r: &Value,
        b: &mut WorkBudget,
    ) -> Result<()> {
        require(self.resolve(j, c, v, b)? == *r, "resolution_authority")
    }
}

#[tokio::test]
async fn reviewed_multi_chunk_jobs_are_assembled_atomically_without_partial_drafts() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../../docs/specs/vnext/ku-encoder-v1/corpus.json"
    ))
    .unwrap();
    let (_, provider, _, _) = fixture(false, false);
    for case in corpus["jobs"].as_array().unwrap() {
        let rows = case["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| {
                corpus["cases"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|r| r["id"] == *id)
                    .unwrap()
                    .clone()
            })
            .collect::<Vec<_>>();
        let mut job = job(&rows[0]);
        job.contexts = rows.iter().map(|r| r["context"].clone()).collect();
        let fixture = Arc::new(JobFixture {
            manifest: provider.manifest.clone(),
            rows,
        });
        let workflow = ExtractionWorkflow::new(fixture.clone(), 1024).unwrap();
        let journal = Journal::default();
        let output = workflow
            .run(
                &job,
                fixture.as_ref(),
                &journal,
                1_000_000,
                Arc::new(AtomicBool::new(false)),
            )
            .await
            .unwrap();
        let unresolved = case["expected_status"] == "needs_resolution";
        assert_eq!(output.needs_resolution, unresolved);
        if unresolved {
            assert!(output.drafts.is_empty());
        } else {
            assert_eq!(output.drafts.len(), 1);
            assert_eq!(
                output.drafts[0].statements.len() as u64,
                case["expected_statement_count"].as_u64().unwrap()
            );
            assert_eq!(output.drafts[0].statements[1].statement_id.0, 1);
        }
        let state = journal.state.lock().unwrap().clone().unwrap();
        assert_eq!(state.calls, 2);
        assert!(state
            .attempts
            .iter()
            .all(|a| a["phase"] == "validated" && a["calls_reserved"] == 2));
    }
}
