use std::path::Path;

use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::MobileCoreError;

const NOTIFICATION_INTENTS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("notification_intents");
const NOTIFICATION_RECEIPTS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("notification_receipts");
const NOTIFICATION_POLICY: TableDefinition<&str, &[u8]> =
    TableDefinition::new("notification_policy");
const POLICY_KEY: &str = "current";
const MAX_INBOX_ITEMS: usize = 256;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationChannelKind {
    RegistryProvisioning,
    ApprovalsAndConsent,
    BackupAndStorage,
    SystemRecovery,
}

impl NotificationChannelKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RegistryProvisioning => "registry_provisioning",
            Self::ApprovalsAndConsent => "approvals_and_consent",
            Self::BackupAndStorage => "backup_and_storage",
            Self::SystemRecovery => "system_recovery",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationReceiptState {
    Scheduled,
    Submitted,
    ActiveObserved,
    Interacted,
    Cancelled,
    PermissionDenied,
    PlatformError,
    DeliveryUnknown,
}

impl NotificationReceiptState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scheduled => "Scheduled",
            Self::Submitted => "Submitted",
            Self::ActiveObserved => "ActiveObserved",
            Self::Interacted => "Interacted",
            Self::Cancelled => "Cancelled",
            Self::PermissionDenied => "PermissionDenied",
            Self::PlatformError => "PlatformError",
            Self::DeliveryUnknown => "DeliveryUnknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReversibleNotificationAction {
    OpenDetail,
    PauseTransfer,
    ResumeForeground,
    CancelScheduled,
    DismissNotice,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NotificationPolicyConfig {
    pub os_permission_granted: bool,
    pub quiet_hours_enabled: bool,
    pub quiet_start_hour_local: u8,
    pub quiet_end_hour_local: u8,
    pub lock_screen_redacted_preview: bool,
    pub enabled_channels: Vec<NotificationChannelKind>,
}

impl Default for NotificationPolicyConfig {
    fn default() -> Self {
        Self {
            os_permission_granted: false,
            quiet_hours_enabled: true,
            quiet_start_hour_local: 22,
            quiet_end_hour_local: 7,
            lock_screen_redacted_preview: true,
            enabled_channels: vec![
                NotificationChannelKind::RegistryProvisioning,
                NotificationChannelKind::ApprovalsAndConsent,
                NotificationChannelKind::BackupAndStorage,
                NotificationChannelKind::SystemRecovery,
            ],
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NotificationIntentRecord {
    pub intent_id: String,
    pub channel: NotificationChannelKind,
    pub title_code: String,
    pub redacted_summary: String,
    pub target_route: String,
    pub created_at_monotonic_ms: u64,
    pub action_nonce: String,
    pub nonce_consumed: bool,
    pub allowed_actions: Vec<ReversibleNotificationAction>,
    pub requires_in_app_approval: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NotificationReceiptRecord {
    pub intent_id: String,
    pub state: NotificationReceiptState,
    pub updated_at_monotonic_ms: u64,
    pub os_notification_tag: Option<String>,
    pub suppressed_by_quiet_hours: bool,
    pub detail_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActivityInboxEntry {
    pub intent: NotificationIntentRecord,
    pub receipt: NotificationReceiptRecord,
}

pub struct ActivityInboxStore {
    database: Database,
}

impl ActivityInboxStore {
    pub fn open(path: &Path) -> Result<Self, MobileCoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                MobileCoreError::Storage(format!(
                    "cannot create activity inbox directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        let database = Database::create(path)?;
        let write = database.begin_write()?;
        {
            write.open_table(NOTIFICATION_INTENTS)?;
            write.open_table(NOTIFICATION_RECEIPTS)?;
            let mut policy = write.open_table(NOTIFICATION_POLICY)?;
            if policy.get(POLICY_KEY)?.is_none() {
                let encoded = serde_json::to_vec(&NotificationPolicyConfig::default())?;
                policy.insert(POLICY_KEY, encoded.as_slice())?;
            }
        }
        write.commit()?;
        Ok(Self { database })
    }

    pub fn policy(&self) -> Result<NotificationPolicyConfig, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(NOTIFICATION_POLICY)?;
        table
            .get(POLICY_KEY)?
            .map(|entry| serde_json::from_slice(entry.value()).map_err(Into::into))
            .transpose()
            .map(|opt| opt.unwrap_or_default())
    }

    pub fn update_policy(&self, config: &NotificationPolicyConfig) -> Result<(), MobileCoreError> {
        if config.quiet_start_hour_local > 23 || config.quiet_end_hour_local > 23 {
            return Err(MobileCoreError::InvalidArgument(
                "quiet hours must be between 0 and 23".into(),
            ));
        }
        let encoded = serde_json::to_vec(config)?;
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(NOTIFICATION_POLICY)?;
            table.insert(POLICY_KEY, encoded.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn schedule_intent(
        &self,
        intent_id: &str,
        channel: NotificationChannelKind,
        title_code: &str,
        redacted_summary: &str,
        target_route: &str,
        allowed_actions: Vec<ReversibleNotificationAction>,
        requires_in_app_approval: bool,
        local_hour: u8,
        now_monotonic_ms: u64,
    ) -> Result<ActivityInboxEntry, MobileCoreError> {
        if intent_id.is_empty() || intent_id.len() > 96 {
            return Err(MobileCoreError::InvalidArgument(
                "intent_id must contain 1..=96 UTF-8 bytes".into(),
            ));
        }
        if title_code.is_empty() || title_code.len() > 96 {
            return Err(MobileCoreError::InvalidArgument(
                "title_code must contain 1..=96 UTF-8 bytes".into(),
            ));
        }
        if redacted_summary.len() > 256 || !target_route.starts_with('/') {
            return Err(MobileCoreError::InvalidArgument(
                "redacted_summary or target_route is invalid".into(),
            ));
        }
        let policy = self.policy()?;
        let in_quiet_hours = policy.quiet_hours_enabled
            && is_hour_in_window(
                local_hour % 24,
                policy.quiet_start_hour_local,
                policy.quiet_end_hour_local,
            );

        let mut nonce_bytes = [0u8; 16];
        getrandom::fill(&mut nonce_bytes).map_err(|_| {
            MobileCoreError::Security("notification action nonce CSPRNG unavailable".into())
        })?;
        let action_nonce = format!("ntfnonce_{}", hex::encode(nonce_bytes));

        let initial_state = if !policy.os_permission_granted {
            NotificationReceiptState::PermissionDenied
        } else if !policy.enabled_channels.contains(&channel) {
            NotificationReceiptState::Cancelled
        } else {
            NotificationReceiptState::Scheduled
        };

        let intent = NotificationIntentRecord {
            intent_id: intent_id.to_owned(),
            channel,
            title_code: title_code.to_owned(),
            redacted_summary: redacted_summary.to_owned(),
            target_route: target_route.to_owned(),
            created_at_monotonic_ms: now_monotonic_ms,
            action_nonce,
            nonce_consumed: false,
            allowed_actions,
            requires_in_app_approval,
        };
        let receipt = NotificationReceiptRecord {
            intent_id: intent_id.to_owned(),
            state: initial_state,
            updated_at_monotonic_ms: now_monotonic_ms,
            os_notification_tag: None,
            suppressed_by_quiet_hours: in_quiet_hours,
            detail_code: if !policy.os_permission_granted {
                Some("IN_APP_INBOX_ONLY_OS_PERMISSION_DENIED".into())
            } else if in_quiet_hours {
                Some("QUIET_HOURS_SILENCED_INBOX_RECORDED".into())
            } else {
                None
            },
        };

        let intent_bytes = serde_json::to_vec(&intent)?;
        let receipt_bytes = serde_json::to_vec(&receipt)?;
        let write = self.database.begin_write()?;
        {
            let mut intents = write.open_table(NOTIFICATION_INTENTS)?;
            let mut receipts = write.open_table(NOTIFICATION_RECEIPTS)?;
            intents.insert(intent_id, intent_bytes.as_slice())?;
            receipts.insert(intent_id, receipt_bytes.as_slice())?;
        }
        write.commit()?;
        Ok(ActivityInboxEntry { intent, receipt })
    }

    /// Record OS submit receipt. Per `MOB-NTF-001`, OS submission transitions to
    /// `Submitted` (or `ActiveObserved` only when enumerated from active OS tray),
    /// NEVER fabricating a `Delivered` state from submit alone.
    pub fn record_os_submission(
        &self,
        intent_id: &str,
        os_notification_tag: &str,
        observed_in_active_tray: bool,
        now_monotonic_ms: u64,
    ) -> Result<NotificationReceiptRecord, MobileCoreError> {
        let state = if observed_in_active_tray {
            NotificationReceiptState::ActiveObserved
        } else {
            NotificationReceiptState::Submitted
        };
        self.transition_receipt(
            intent_id,
            state,
            Some(os_notification_tag.to_owned()),
            Some(if observed_in_active_tray {
                "OS_ACTIVE_TRAY_ENUMERATED".into()
            } else {
                "OS_SUBMIT_RECORDED_DELIVERY_UNOBSERVED".into()
            }),
            now_monotonic_ms,
        )
    }

    pub fn reconcile_unobserved_after_restart(
        &self,
        active_os_tags: &[String],
        now_monotonic_ms: u64,
    ) -> Result<usize, MobileCoreError> {
        let entries = self.list_entries(MAX_INBOX_ITEMS)?;
        let mut updated = 0usize;
        for entry in entries {
            if matches!(
                entry.receipt.state,
                NotificationReceiptState::Submitted | NotificationReceiptState::Scheduled
            ) {
                let observed = entry
                    .receipt
                    .os_notification_tag
                    .as_ref()
                    .is_some_and(|tag| active_os_tags.contains(tag));
                let next_state = if observed {
                    NotificationReceiptState::ActiveObserved
                } else {
                    NotificationReceiptState::DeliveryUnknown
                };
                self.transition_receipt(
                    &entry.intent.intent_id,
                    next_state,
                    entry.receipt.os_notification_tag,
                    Some("RECONCILED_AFTER_RESTART".into()),
                    now_monotonic_ms,
                )?;
                updated = updated.saturating_add(1);
            }
        }
        Ok(updated)
    }

    /// Execute a reversible notification action gated by a one-time nonce.
    /// Irreversible actions (publish, delete, sign, grant, restore) are structurally
    /// unrepresentable in `ReversibleNotificationAction` and rejected if requested
    /// by string command (`validate_action_not_irreversible`).
    pub fn execute_reversible_action(
        &self,
        intent_id: &str,
        action_nonce: &str,
        action: ReversibleNotificationAction,
        now_monotonic_ms: u64,
    ) -> Result<ActivityInboxEntry, MobileCoreError> {
        let write = self.database.begin_write()?;
        let updated_entry;
        {
            let mut intents = write.open_table(NOTIFICATION_INTENTS)?;
            let mut receipts = write.open_table(NOTIFICATION_RECEIPTS)?;
            let existing_intent = intents
                .get(intent_id)?
                .ok_or_else(|| MobileCoreError::InvalidArgument("unknown intent_id".into()))?;
            let mut intent: NotificationIntentRecord =
                serde_json::from_slice(existing_intent.value())?;
            drop(existing_intent);

            if intent.nonce_consumed || intent.action_nonce != action_nonce {
                return Err(MobileCoreError::Security(
                    "notification action nonce is invalid or already consumed".into(),
                ));
            }
            if !intent.allowed_actions.contains(&action) {
                return Err(MobileCoreError::Security(
                    "requested notification action is not permitted for this intent".into(),
                ));
            }
            intent.nonce_consumed = true;

            let existing_receipt = receipts
                .get(intent_id)?
                .ok_or_else(|| MobileCoreError::Storage("missing notification receipt".into()))?;
            let mut receipt: NotificationReceiptRecord =
                serde_json::from_slice(existing_receipt.value())?;
            drop(existing_receipt);

            receipt.state = match action {
                ReversibleNotificationAction::CancelScheduled
                | ReversibleNotificationAction::DismissNotice => {
                    NotificationReceiptState::Cancelled
                }
                ReversibleNotificationAction::OpenDetail
                | ReversibleNotificationAction::PauseTransfer
                | ReversibleNotificationAction::ResumeForeground => {
                    NotificationReceiptState::Interacted
                }
            };
            receipt.updated_at_monotonic_ms = now_monotonic_ms;
            receipt.detail_code = Some(format!("ACTION_{action:?}").to_ascii_uppercase());

            let intent_bytes = serde_json::to_vec(&intent)?;
            let receipt_bytes = serde_json::to_vec(&receipt)?;
            intents.insert(intent_id, intent_bytes.as_slice())?;
            receipts.insert(intent_id, receipt_bytes.as_slice())?;
            updated_entry = ActivityInboxEntry { intent, receipt };
        }
        write.commit()?;
        Ok(updated_entry)
    }

    pub fn reject_if_irreversible_action_code(action_code: &str) -> Result<(), MobileCoreError> {
        let normalized = action_code.trim().to_ascii_lowercase();
        if matches!(
            normalized.as_str(),
            "publish"
                | "delete"
                | "sign"
                | "grant"
                | "restore"
                | "approve_tool"
                | "disclose_cloud"
                | "export_unencrypted"
        ) {
            return Err(MobileCoreError::Security(format!(
                "irreversible authority action '{normalized}' is forbidden from OS notifications; open in-app review"
            )));
        }
        Ok(())
    }

    pub fn entry(&self, intent_id: &str) -> Result<Option<ActivityInboxEntry>, MobileCoreError> {
        let read = self.database.begin_read()?;
        let intents = read.open_table(NOTIFICATION_INTENTS)?;
        let receipts = read.open_table(NOTIFICATION_RECEIPTS)?;
        let Some(intent_raw) = intents.get(intent_id)? else {
            return Ok(None);
        };
        let Some(receipt_raw) = receipts.get(intent_id)? else {
            return Ok(None);
        };
        Ok(Some(ActivityInboxEntry {
            intent: serde_json::from_slice(intent_raw.value())?,
            receipt: serde_json::from_slice(receipt_raw.value())?,
        }))
    }

    pub fn list_entries(&self, limit: usize) -> Result<Vec<ActivityInboxEntry>, MobileCoreError> {
        let bounded = limit.clamp(1, MAX_INBOX_ITEMS);
        let read = self.database.begin_read()?;
        let intents = read.open_table(NOTIFICATION_INTENTS)?;
        let receipts = read.open_table(NOTIFICATION_RECEIPTS)?;
        let mut items = Vec::new();
        for row in intents.iter()? {
            let (key, value) = row?;
            let intent: NotificationIntentRecord = serde_json::from_slice(value.value())?;
            if let Some(receipt_raw) = receipts.get(key.value())? {
                let receipt: NotificationReceiptRecord =
                    serde_json::from_slice(receipt_raw.value())?;
                items.push(ActivityInboxEntry { intent, receipt });
            }
        }
        items.sort_by(|left, right| {
            right
                .intent
                .created_at_monotonic_ms
                .cmp(&left.intent.created_at_monotonic_ms)
                .then_with(|| left.intent.intent_id.cmp(&right.intent.intent_id))
        });
        items.truncate(bounded);
        Ok(items)
    }

    fn transition_receipt(
        &self,
        intent_id: &str,
        state: NotificationReceiptState,
        os_notification_tag: Option<String>,
        detail_code: Option<String>,
        now_monotonic_ms: u64,
    ) -> Result<NotificationReceiptRecord, MobileCoreError> {
        let write = self.database.begin_write()?;
        let updated;
        {
            let mut receipts = write.open_table(NOTIFICATION_RECEIPTS)?;
            let existing = receipts
                .get(intent_id)?
                .ok_or_else(|| MobileCoreError::InvalidArgument("unknown intent_id".into()))?;
            let mut record: NotificationReceiptRecord = serde_json::from_slice(existing.value())?;
            drop(existing);
            record.state = state;
            record.updated_at_monotonic_ms = now_monotonic_ms;
            if os_notification_tag.is_some() {
                record.os_notification_tag = os_notification_tag;
            }
            if detail_code.is_some() {
                record.detail_code = detail_code;
            }
            let bytes = serde_json::to_vec(&record)?;
            receipts.insert(intent_id, bytes.as_slice())?;
            updated = record;
        }
        write.commit()?;
        Ok(updated)
    }
}

const fn is_hour_in_window(hour: u8, start: u8, end: u8) -> bool {
    if start == end {
        false
    } else if start < end {
        hour >= start && hour < end
    } else {
        hour >= start || hour < end
    }
}
