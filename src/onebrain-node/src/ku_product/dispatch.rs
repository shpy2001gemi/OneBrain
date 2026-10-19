//! ARCH-09: KU product dispatch over the narrow Base product port.
//!
//! `base_runtime` owns admission, lifecycle, generation fences and the Base
//! operation journal. This module owns the KU-specific mapping of
//! `KuRequestV1` / editor requests onto that journal and the `KuStore`, so the
//! Base runtime no longer carries KU request semantics. Behavior is unchanged
//! from the former `BaseServices::ku_*` methods (same admission class, budget
//! checks, error precedence and receipt projection).

use onebrain_base_contract::ku::*;
use onebrain_base_contract::ku_payload::KuPayload;
use onebrain_base_contract::{
    BaseErrorCodeV1, BaseIdempotencyKey, BaseOperationId, BaseOperationKindV1,
    BaseOperationReservationId, BaseRequestV1, ResourceBudgetV1,
};

use super::{conflict, corrupt, invalid, resource, unavailable, unknown, KU_PREPARED_MARKER};
use crate::base_operation_store::{BaseOperationReceiptV1, BaseOperationStateV1 as State};
use crate::base_runtime::{
    internal_error, store_error, validate_budget, BaseResponseV1, BaseServiceError, BaseServices,
    ProductAdmission,
};

/// Authenticated KU session façade handed to API/Desktop/CLI hosts.
#[derive(Clone)]
pub struct KuServices {
    pub(crate) base: BaseServices,
}

impl KuServices {
    pub fn experimental_ai_allowed(&self, commitment: [u8; 32]) -> Result<bool, BaseServiceError> {
        experimental_ai_allowed(&self.base, commitment)
    }
    pub async fn editor(
        &self,
        request: crate::ku_manual::ManualEditorRequest,
        budget: ResourceBudgetV1,
    ) -> Result<crate::ku_manual::ManualEditorResponse, BaseServiceError> {
        editor(&self.base, request, budget).await
    }
    pub async fn reserve(&self) -> Result<OperationId, BaseServiceError> {
        reserve(&self.base).await
    }
    pub async fn invoke(
        &self,
        request: KuRequestV1,
        budget: ResourceBudgetV1,
    ) -> Result<KuResponseV1, BaseServiceError> {
        request.validate().map_err(|_| invalid())?;
        invoke(&self.base, request, budget).await
    }
    pub async fn invoke_payload(
        &self,
        kind: u16,
        bytes: &[u8],
        budget: ResourceBudgetV1,
    ) -> Result<KuResponseV1, BaseServiceError> {
        if bytes.len() as u64 > budget.max_bytes {
            return Err(resource());
        }
        let request = KuRequestV1::decode(kind, bytes).map_err(|_| invalid())?;
        self.invoke(request, budget).await
    }
}

async fn editor(
    base: &BaseServices,
    request: crate::ku_manual::ManualEditorRequest,
    budget: ResourceBudgetV1,
) -> Result<crate::ku_manual::ManualEditorResponse, BaseServiceError> {
    use crate::ku_manual::ManualEditorRequest as Editor;
    let recovery = matches!(
        &request,
        Editor::ReviewGet { .. }
            | Editor::ReviewCancel { .. }
            | Editor::ReviewList { .. }
            | Editor::ShareInspect(_)
            | Editor::ShareIntent(_)
    );
    let lease = base.product_lease(if recovery {
        ProductAdmission::Recovery
    } else {
        ProductAdmission::NewWork
    })?;
    validate_budget(&budget)?;
    if budget.max_bytes < 16384 || budget.max_items == 0 || budget.max_work_units < 1024 {
        return Err(resource());
    }
    let ku = lease.ku().ok_or_else(unavailable)?;
    ku.check_dataset(lease.dataset_generation())?;
    let principal = lease.principal();
    let operation = match &request {
        Editor::Draft(input) => Some(input.operation_id.0),
        Editor::EncodeText(input) => Some(input.operation_id.0),
        Editor::ReviewStart(input) => Some(input.operation_id.0),
        Editor::ReviewResume { operation_id } => Some(operation_id.0),
        Editor::ReviewLower(input) => Some(input.operation_id.0),
        _ => None,
    };
    if let Some(operation) = operation {
        let receipt = lease
            .operations()?
            .reconcile(BaseOperationId(operation), principal)
            .map_err(store_error)?
            .receipt;
        let draft_recovery = matches!(&request, Editor::ReviewResume { .. })
            && receipt.state == State::UnknownOutcome;
        if receipt.state != State::Reserved && !draft_recovery {
            return Err(conflict());
        }
    }
    let start = match request {
        Editor::ReviewList {} => return ku.review_list(principal, &budget),
        Editor::ReviewGet { operation_id } => return ku.review_get(principal, operation_id.0),
        Editor::ReviewCancel { operation_id } => {
            ku.review_cancel(principal, operation_id.0)?;
            return ku.review_get(principal, operation_id.0);
        }
        Editor::ReviewStart(input) => {
            let id = input.operation_id.0;
            if !ku.review_start(principal, input, &budget)? {
                return ku.review_get(principal, id);
            }
            id
        }
        Editor::ReviewResume { operation_id } => {
            ku.review_resume(principal, operation_id.0)?;
            operation_id.0
        }
        other => return ku.editor(principal, other, &budget),
    };
    let worker_lease = base.product_lease(ProductAdmission::NewWork)?;
    let response = ku.review_get(principal, start)?;
    // Admission and drain share the lifecycle fence, so no worker can join
    // after drain has signaled the existing review jobs.
    let Some(cancel) = worker_lease.with_open_lifecycle(|| ku.review_admit(start))? else {
        return Err(conflict());
    };
    tokio::spawn(async move {
        if let Some(ku) = worker_lease.ku() {
            ku.run_review(principal, start, cancel).await;
        }
    });
    Ok(response)
}

fn experimental_ai_allowed(
    base: &BaseServices,
    commitment: [u8; 32],
) -> Result<bool, BaseServiceError> {
    let lease = base.product_lease(ProductAdmission::NewWork)?;
    let ku = lease.ku().ok_or_else(unavailable)?;
    ku.check_dataset(lease.dataset_generation())?;
    Ok(ku.inputs.experimental_ai_allowed(commitment))
}

async fn reserve(base: &BaseServices) -> Result<OperationId, BaseServiceError> {
    let lease = base.product_lease(ProductAdmission::NewWork)?;
    let ku = lease.ku().ok_or_else(unavailable)?;
    ku.check_dataset(lease.dataset_generation())?;
    let op = lease
        .operations()?
        .reserve_operation(BaseOperationKindV1::ExistingLocalCommand, lease.principal())
        .map_err(store_error)?;
    Ok(OperationId(op.0))
}

fn map_receipt(r: &BaseOperationReceiptV1) -> Result<KuReceiptV1, BaseServiceError> {
    if r.state == State::Committed && !r.result.is_empty() {
        return KuReceiptV1::decode(&r.result).map_err(|_| corrupt());
    }
    Ok(KuReceiptV1 {
        operation_id: OperationId(r.operation_id.0),
        state: match r.state {
            State::Reserved => BaseState::Reserved,
            State::Prepared => BaseState::Prepared,
            State::Confirming => BaseState::Confirming,
            State::Committed => BaseState::Committed,
            State::Canceled => BaseState::Canceled,
            State::Failed => BaseState::Failed,
            State::UnknownOutcome => BaseState::UnknownOutcome,
        },
        object_cids: vec![],
        limitations: vec![],
        published: false,
        authorizes_reward: false,
    })
}

async fn invoke(
    base: &BaseServices,
    request: KuRequestV1,
    budget: ResourceBudgetV1,
) -> Result<KuResponseV1, BaseServiceError> {
    let admission = match request {
        KuRequestV1::Cancel(_)
        | KuRequestV1::Reconcile(_)
        | KuRequestV1::Status(_)
        | KuRequestV1::Preview(_) => ProductAdmission::Recovery,
        _ => ProductAdmission::NewWork,
    };
    let lease = base.product_lease(admission)?;
    validate_budget(&budget)?;
    if budget.max_bytes == 0 || budget.max_items == 0 || budget.max_work_units == 0 {
        return Err(resource());
    }
    if request.payload_bytes().map_err(|_| resource())?.len() as u64 > budget.max_bytes {
        return Err(resource());
    }
    let ku = lease.ku().ok_or_else(unavailable)?;
    ku.check_dataset(lease.dataset_generation())?;
    let store = lease.operations()?;
    let principal = lease.principal();
    let admit_receipt = |receipt: &KuReceiptV1| -> Result<(), BaseServiceError> {
        if receipt.encode().map_err(|_| resource())?.len() as u64 > budget.max_bytes {
            return Err(resource());
        }
        Ok(())
    };
    let complete_ku = |id: OperationId, receipt: &KuReceiptV1| -> Result<(), BaseServiceError> {
        let result = receipt.encode().map_err(|_| unknown()).and_then(|bytes| {
            store
                .complete(BaseOperationId(id.0), bytes)
                .map(|_| ())
                .map_err(|_| unknown())
        });
        if result.is_err() {
            let _ = store.mark_unknown(BaseOperationId(id.0));
        }
        result
    };
    let preparation_store = &store;
    let preparation_budget = &budget;
    let prepare = |preparation: KuPrepareV1, revision: Option<(ObjectCID, RevisionFrontier)>| async move {
        let store = preparation_store;
        let budget = preparation_budget;
        let id = preparation.operation_id;
        if store
            .operation_kind(BaseOperationId(id.0), principal)
            .map_err(store_error)?
            != BaseOperationKindV1::ExistingLocalCommand
        {
            return Err(conflict());
        }
        let state = store
            .reconcile(BaseOperationId(id.0), principal)
            .map_err(store_error)?
            .receipt
            .state;
        if !matches!(state, State::Reserved | State::Prepared) {
            return Err(conflict());
        }
        if state == State::Prepared && !ku.is_prepared(principal, id)? {
            return Err(corrupt());
        }
        let preview = ku.prepare(principal, preparation, revision, budget).await?;
        let mut exact = KU_PREPARED_MARKER.to_vec();
        exact.extend_from_slice(&id.0);
        store
            .prepare(
                BaseOperationReservationId(id.0),
                BaseOperationKindV1::ExistingLocalCommand,
                exact,
                None,
                principal,
            )
            .map_err(store_error)?;
        Ok(preview)
    };
    let response = match request {
        KuRequestV1::Prepare(r) => KuResponseV1::Prepare(prepare(r, None).await?),
        KuRequestV1::Revise(r) => KuResponseV1::Revise(
            prepare(
                r.preparation,
                Some((r.predecessor_object_cid, r.expected_revision_frontier)),
            )
            .await?,
        ),
        KuRequestV1::Preview(r) => {
            let state = store
                .reconcile(BaseOperationId(r.operation_id.0), principal)
                .map_err(store_error)?
                .receipt
                .state;
            if state == State::Canceled {
                return Err(conflict());
            }
            KuResponseV1::Preview(ku.preview(principal, r.operation_id)?)
        }
        KuRequestV1::Save(r) => {
            r.validate().map_err(|_| invalid())?;
            ku.preflight_save(principal, &r)?;
            admit_receipt(&ku.confirmation_receipt(principal, r.operation_id)?)?;
            if let Some(receipt) = store
                .begin_confirm(
                    BaseOperationId(r.operation_id.0),
                    BaseIdempotencyKey(r.idempotency_key.0),
                    principal,
                )
                .map_err(store_error)?
            {
                KuResponseV1::Save(map_receipt(&receipt)?)
            } else {
                match ku.save(principal, &r) {
                    Ok(result) => {
                        complete_ku(r.operation_id, &result)?;
                        KuResponseV1::Save(result)
                    }
                    Err(_) => {
                        let _ = store.mark_unknown(BaseOperationId(r.operation_id.0));
                        return Err(unknown());
                    }
                }
            }
        }
        KuRequestV1::Get(r) => KuResponseV1::Get(ku.get(principal, r.object_cid)?),
        KuRequestV1::List(r) => {
            KuResponseV1::List(ku.page(principal, None, r.limit, r.continuation, &budget)?)
        }
        KuRequestV1::Search(r) => KuResponseV1::Search(ku.page(
            principal,
            Some(r.query),
            r.limit,
            r.continuation,
            &budget,
        )?),
        KuRequestV1::Status(r) => {
            let mut result = ku.status();
            if !lease.lifecycle_open()? {
                result.lifecycle = Lifecycle::Degraded;
                result.limitations.push("base_runtime_draining".into());
            }
            if let Some(id) = r.operation_id {
                let base = store
                    .reconcile(BaseOperationId(id.0), principal)
                    .map_err(store_error)?;
                result.receipt = Some(map_receipt(&base.receipt)?);
            }
            KuResponseV1::Status(result)
        }
        KuRequestV1::Cancel(r) => {
            if store
                .operation_kind(BaseOperationId(r.operation_id.0), principal)
                .map_err(store_error)?
                != BaseOperationKindV1::ExistingLocalCommand
            {
                return Err(conflict());
            }
            let mut projected = map_receipt(
                &store
                    .reconcile(BaseOperationId(r.operation_id.0), principal)
                    .map_err(store_error)?
                    .receipt,
            )?;
            projected.state = BaseState::Canceled;
            admit_receipt(&projected)?;
            // The eligibility check and state change share the Base journal lock.
            let receipt = store
                .cancel_before_confirmation(BaseOperationId(r.operation_id.0), principal)
                .map_err(store_error)?;
            ku.cancel(principal, r.operation_id)
                .map_err(|_| unknown())?;
            KuResponseV1::Cancel(map_receipt(&receipt)?)
        }
        KuRequestV1::Reconcile(r) => {
            let base = store
                .reconcile(BaseOperationId(r.operation_id.0), principal)
                .map_err(store_error)?
                .receipt;
            if base.state == State::UnknownOutcome {
                if base.idempotency_key.is_some() {
                    admit_receipt(&ku.confirmation_receipt(principal, r.operation_id)?)?;
                }
                if let Some(result) = ku.saved_receipt(principal, r.operation_id)? {
                    admit_receipt(&result)?;
                    complete_ku(r.operation_id, &result)?;
                    KuResponseV1::Reconcile(result)
                } else if let Some(key) = base.idempotency_key {
                    // Repair the finite saved command from encrypted staging; never invoke an encoder.
                    let result = ku
                        .recovery_save(principal, r.operation_id, key.0)
                        .map_err(|_| unknown())?;
                    complete_ku(r.operation_id, &result)?;
                    KuResponseV1::Reconcile(result)
                } else {
                    // No confirmation key means save never began. Close the
                    // interrupted preparation without invoking its encoder.
                    let mut projected = map_receipt(&base)?;
                    projected.state = BaseState::Failed;
                    admit_receipt(&projected)?;
                    let failed = store
                        .fail(BaseOperationId(r.operation_id.0), BaseErrorCodeV1::Conflict)
                        .map_err(store_error)?;
                    // ARCH-03 GC: Base `failed` is final, so the KU active
                    // bundle/extraction row is compacted or dropped. The failed
                    // receipt is already durable; a cleanup error only leaves
                    // the row occupying one active slot and is not surfaced.
                    let _ = ku.retire_unsaved(principal, r.operation_id);
                    KuResponseV1::Reconcile(map_receipt(&failed)?)
                }
            } else {
                KuResponseV1::Reconcile(map_receipt(&base)?)
            }
        }
        KuRequestV1::Export(r) => {
            if r.mode == ExportMode::CanonicalPublicExchange {
                KuResponseV1::Export(ku.public_export(principal, r)?)
            } else {
                for cid in &r.object_cids {
                    ku.get(principal, *cid)?;
                }
                if !lease.archive_available() {
                    return Err(unavailable());
                }
                let mut projected = KuExportViewV1 {
                    mode: r.mode,
                    object_cids: r.object_cids,
                    limitations: vec!["base_archive_dataset_scope".into()],
                    requires_base_management: true,
                    public_records: None,
                    archive_operation_id: Some(OperationId([0; 32])),
                };
                if projected.encode().map_err(|_| resource())?.len() as u64 > budget.max_bytes {
                    return Err(resource());
                }
                let BaseResponseV1::Reserved(operation) = base
                    .invoke(BaseRequestV1::ReserveOperation(
                        BaseOperationKindV1::CreateArchive,
                    ))
                    .await?
                else {
                    return Err(internal_error());
                };
                projected.archive_operation_id = Some(OperationId(operation.0));
                KuResponseV1::Export(projected)
            }
        }
    };
    if response.payload_bytes().map_err(|_| resource())?.len() as u64 > budget.max_bytes {
        return Err(resource());
    }
    Ok(response)
}
