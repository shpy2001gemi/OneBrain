import 'package:flutter/material.dart';

import '../../design/onebrain_theme_extensions.dart';
import 'obm_button.dart';
import 'obm_icon.dart';
import 'obm_registry_plan.dart';
import 'obm_status_badge.dart';

/// `OBM-CMP-STA-003` — bounded deterministic progress bar using semantic status tokens.
class ObmProgressBar extends StatelessWidget {
  const ObmProgressBar({
    required this.label,
    required this.progressFraction,
    required this.detailLabel,
    this.tone = ObmStatusTone.information,
    super.key,
  });

  final String label;
  final double progressFraction;
  final String detailLabel;
  final ObmStatusTone tone;

  @override
  Widget build(BuildContext context) {
    final palette = context.statusColors.resolve(tone);
    final clamped = progressFraction.clamp(0.0, 1.0);
    return Semantics(
      label: '$label: $detailLabel',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  label,
                  style: Theme.of(context).textTheme.titleSmall,
                ),
              ),
              SizedBox(width: context.spacing.sm),
              Flexible(
                child: Text(
                  detailLabel,
                  style: context.dataStyle.value,
                  textAlign: TextAlign.end,
                ),
              ),
            ],
          ),
          SizedBox(height: context.spacing.xs),
          ClipRRect(
            borderRadius: BorderRadius.circular(context.spacing.xs),
            child: LinearProgressIndicator(
              value: clamped,
              minHeight: context.spacing.sm,
              backgroundColor: palette.container,
              valueColor: AlwaysStoppedAnimation<Color>(palette.content),
            ),
          ),
        ],
      ),
    );
  }
}

@immutable
class ObmTimelineStep {
  const ObmTimelineStep({
    required this.title,
    required this.detail,
    required this.tone,
    required this.statusLabel,
  });

  final String title;
  final String detail;
  final ObmStatusTone tone;
  final String statusLabel;
}

/// `OBM-CMP-STA-004` — multi-stage deterministic step timeline.
class ObmStepTimeline extends StatelessWidget {
  const ObmStepTimeline({required this.steps, super.key});

  final List<ObmTimelineStep> steps;

  @override
  Widget build(BuildContext context) => Card(
    child: Padding(
      padding: EdgeInsets.all(context.spacing.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (var i = 0; i < steps.length; i++) ...[
            if (i > 0) Divider(height: context.spacing.xl),
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                ObmIcon(
                  steps[i].tone == ObmStatusTone.ready
                      ? ObmSymbol.checkCircle
                      : steps[i].tone == ObmStatusTone.failed
                      ? ObmSymbol.cancel
                      : ObmSymbol.schedule,
                  color: context.statusColors.resolve(steps[i].tone).content,
                ),
                SizedBox(width: context.spacing.md),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        steps[i].title,
                        style: Theme.of(context).textTheme.titleSmall,
                      ),
                      SizedBox(height: context.spacing.xxs),
                      Text(
                        steps[i].detail,
                        style: Theme.of(context).textTheme.bodyMedium,
                      ),
                      SizedBox(height: context.spacing.xs),
                      ObmStatusBadge(
                        label: steps[i].statusLabel,
                        tone: steps[i].tone,
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ],
        ],
      ),
    ),
  );
}

/// `OBM-CMP-STA-005` — honest capability availability row.
class ObmCapabilityRow extends StatelessWidget {
  const ObmCapabilityRow({
    required this.title,
    required this.reason,
    required this.statusLabel,
    required this.tone,
    this.icon = ObmSymbol.shield,
    super.key,
  });

  final String title;
  final String reason;
  final String statusLabel;
  final ObmStatusTone tone;
  final ObmSymbol icon;

  @override
  Widget build(BuildContext context) {
    final palette = context.statusColors.resolve(tone);
    return Card(
      child: Padding(
        padding: EdgeInsets.all(context.spacing.md),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            ObmIcon(icon, color: palette.content),
            SizedBox(width: context.spacing.md),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(title, style: Theme.of(context).textTheme.titleSmall),
                  SizedBox(height: context.spacing.xxs),
                  Text(reason, style: Theme.of(context).textTheme.bodyMedium),
                ],
              ),
            ),
            SizedBox(width: context.spacing.sm),
            ObmStatusBadge(label: statusLabel, tone: tone),
          ],
        ),
      ),
    );
  }
}

/// `OBM-CMP-OPS-001` — resumable durable operation summary card.
class ObmOperationCard extends StatelessWidget {
  const ObmOperationCard({
    required this.operationId,
    required this.title,
    required this.sourceLabel,
    required this.stateLabel,
    required this.progressFraction,
    required this.progressDetail,
    required this.tone,
    this.primaryActionLabel,
    this.onPrimaryAction,
    super.key,
  });

  final String operationId;
  final String title;
  final String sourceLabel;
  final String stateLabel;
  final double progressFraction;
  final String progressDetail;
  final ObmStatusTone tone;
  final String? primaryActionLabel;
  final VoidCallback? onPrimaryAction;

  @override
  Widget build(BuildContext context) => Card(
    child: Padding(
      padding: EdgeInsets.all(context.spacing.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              const ObmIcon(ObmSymbol.operations),
              SizedBox(width: context.spacing.md),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(title, style: Theme.of(context).textTheme.titleMedium),
                    Text(sourceLabel, style: context.dataStyle.value),
                  ],
                ),
              ),
              ObmStatusBadge(label: stateLabel, tone: tone),
            ],
          ),
          SizedBox(height: context.spacing.md),
          ObmProgressBar(
            label: operationId,
            progressFraction: progressFraction,
            detailLabel: progressDetail,
            tone: tone,
          ),
          if (primaryActionLabel != null) ...[
            SizedBox(height: context.spacing.md),
            ObmButton(
              label: primaryActionLabel!,
              variant: ObmButtonVariant.tonal,
              onPressed: onPrimaryAction,
            ),
          ],
        ],
      ),
    ),
  );
}

/// `OBM-CMP-OPS-004` — explicit foreground Resume / Pause / Cancel decision bar.
class ObmResumeDecision extends StatelessWidget {
  const ObmResumeDecision({
    required this.reasonBody,
    required this.resumeLabel,
    required this.secondaryLabel,
    required this.onResume,
    required this.onSecondary,
    this.busy = false,
    super.key,
  });

  final String reasonBody;
  final String resumeLabel;
  final String secondaryLabel;
  final VoidCallback? onResume;
  final VoidCallback? onSecondary;
  final bool busy;

  @override
  Widget build(BuildContext context) => Card(
    child: Padding(
      padding: EdgeInsets.all(context.spacing.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(reasonBody, style: Theme.of(context).textTheme.bodyMedium),
          SizedBox(height: context.spacing.md),
          ObmSplitReviewActions(
            deferLabel: secondaryLabel,
            confirmLabel: resumeLabel,
            onDefer: onSecondary,
            onConfirm: onResume,
            busy: busy,
          ),
        ],
      ),
    ),
  );
}

/// `OBM-CMP-OPS-005` — cryptographic & health verification receipt card.
class ObmVerificationReceipt extends StatelessWidget {
  const ObmVerificationReceipt({
    required this.title,
    required this.statusLabel,
    required this.tone,
    required this.facts,
    super.key,
  });

  final String title;
  final String statusLabel;
  final ObmStatusTone tone;
  final List<ObmPlanFact> facts;

  @override
  Widget build(BuildContext context) => ObmExactPlanPanel(
    title: title,
    subtitle: statusLabel,
    facts: facts,
    statusLabel: statusLabel,
    tone: tone,
  );
}

/// `OBM-CMP-AI-001` — explicit AI provider mode chip (`No LLM`, `Local`, `Cloud`).
class ObmProviderChip extends StatelessWidget {
  const ObmProviderChip({
    required this.providerLabel,
    required this.tone,
    this.onTap,
    super.key,
  });

  final String providerLabel;
  final ObmStatusTone tone;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) => ActionChip(
    avatar: ObmIcon(
      ObmSymbol.assistant,
      size: ObmIconSize.inline,
      color: context.statusColors.resolve(tone).content,
    ),
    label: Text(providerLabel),
    onPressed: onTap,
  );
}

/// `OBM-CMP-AI-002` — non-authoritative assistant / user turn bubble.
class ObmAssistantMessage extends StatelessWidget {
  const ObmAssistantMessage({
    required this.roleLabel,
    required this.body,
    required this.isUser,
    this.providerBadge,
    super.key,
  });

  final String roleLabel;
  final String body;
  final bool isUser;
  final String? providerBadge;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Card(
      color: isUser ? scheme.secondaryContainer : scheme.surfaceContainerLow,
      child: Padding(
        padding: EdgeInsets.all(context.spacing.md),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(
                    roleLabel,
                    style: Theme.of(context).textTheme.labelLarge,
                  ),
                ),
                if (providerBadge != null)
                  ObmStatusBadge(
                    label: providerBadge!,
                    tone: ObmStatusTone.information,
                  ),
              ],
            ),
            SizedBox(height: context.spacing.xs),
            Text(body, style: Theme.of(context).textTheme.bodyMedium),
          ],
        ),
      ),
    );
  }
}

/// `OBM-CMP-AI-003` — explicit scoped context attachment capsule.
class ObmContextCapsule extends StatelessWidget {
  const ObmContextCapsule({
    required this.refLabel,
    required this.scopeLabel,
    this.onRemove,
    super.key,
  });

  final String refLabel;
  final String scopeLabel;
  final VoidCallback? onRemove;

  @override
  Widget build(BuildContext context) => InputChip(
    avatar: const ObmIcon(ObmSymbol.lock, size: ObmIconSize.inline),
    label: Text('$refLabel · $scopeLabel'),
    onDeleted: onRemove,
  );
}

/// `OBM-CMP-AI-004` — durable tool proposal consent card (`Proposed -> AwaitingConsent`).
class ObmToolProposalCard extends StatelessWidget {
  const ObmToolProposalCard({
    required this.proposalId,
    required this.toolName,
    required this.argumentsSummary,
    required this.stateLabel,
    required this.approveLabel,
    required this.denyLabel,
    this.onApprove,
    this.onDeny,
    super.key,
  });

  final String proposalId;
  final String toolName;
  final String argumentsSummary;
  final String stateLabel;
  final String approveLabel;
  final String denyLabel;
  final VoidCallback? onApprove;
  final VoidCallback? onDeny;

  @override
  Widget build(BuildContext context) => Card(
    child: Padding(
      padding: EdgeInsets.all(context.spacing.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              const ObmIcon(ObmSymbol.buildCircle),
              SizedBox(width: context.spacing.sm),
              Expanded(
                child: Text(
                  toolName,
                  style: Theme.of(context).textTheme.titleMedium,
                ),
              ),
              ObmStatusBadge(label: stateLabel, tone: ObmStatusTone.waiting),
            ],
          ),
          SizedBox(height: context.spacing.xs),
          Text(proposalId, style: context.dataStyle.value),
          SizedBox(height: context.spacing.sm),
          Text(argumentsSummary, style: context.dataStyle.value),
          if (onApprove != null || onDeny != null) ...[
            SizedBox(height: context.spacing.md),
            ObmSplitReviewActions(
              deferLabel: denyLabel,
              confirmLabel: approveLabel,
              onDefer: onDeny,
              onConfirm: onApprove,
            ),
          ],
        ],
      ),
    ),
  );
}

/// `OBM-CMP-AI-005` — durable tool execution receipt (`Committed | Aborted | Unknown`).
class ObmToolResultCard extends StatelessWidget {
  const ObmToolResultCard({
    required this.proposalId,
    required this.toolName,
    required this.stateLabel,
    required this.summary,
    required this.tone,
    super.key,
  });

  final String proposalId;
  final String toolName;
  final String stateLabel;
  final String summary;
  final ObmStatusTone tone;

  @override
  Widget build(BuildContext context) => Card(
    child: Padding(
      padding: EdgeInsets.all(context.spacing.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  toolName,
                  style: Theme.of(context).textTheme.titleMedium,
                ),
              ),
              ObmStatusBadge(label: stateLabel, tone: tone),
            ],
          ),
          SizedBox(height: context.spacing.xs),
          Text(proposalId, style: context.dataStyle.value),
          SizedBox(height: context.spacing.sm),
          Text(summary, style: Theme.of(context).textTheme.bodyMedium),
        ],
      ),
    ),
  );
}

/// `OBM-CMP-AI-006` — signed local model catalog entry card.
class ObmModelDownloadCard extends StatelessWidget {
  const ObmModelDownloadCard({
    required this.modelId,
    required this.releaseId,
    required this.quantization,
    required this.sizeLabel,
    required this.stateLabel,
    required this.tone,
    this.primaryLabel,
    this.onPrimary,
    this.secondaryLabel,
    this.onSecondary,
    super.key,
  });

  final String modelId;
  final String releaseId;
  final String quantization;
  final String sizeLabel;
  final String stateLabel;
  final ObmStatusTone tone;
  final String? primaryLabel;
  final VoidCallback? onPrimary;
  final String? secondaryLabel;
  final VoidCallback? onSecondary;

  @override
  Widget build(BuildContext context) => Card(
    child: Padding(
      padding: EdgeInsets.all(context.spacing.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              const ObmIcon(ObmSymbol.memory),
              SizedBox(width: context.spacing.sm),
              Expanded(
                child: Text(
                  modelId,
                  style: Theme.of(context).textTheme.titleMedium,
                ),
              ),
              ObmStatusBadge(label: stateLabel, tone: tone),
            ],
          ),
          SizedBox(height: context.spacing.xs),
          Text(
            '$releaseId · $quantization · $sizeLabel',
            style: context.dataStyle.value,
          ),
          if (primaryLabel != null) ...[
            SizedBox(height: context.spacing.md),
            ObmButton(label: primaryLabel!, onPressed: onPrimary),
          ],
          if (secondaryLabel != null) ...[
            SizedBox(height: context.spacing.sm),
            ObmButton(
              label: secondaryLabel!,
              variant: ObmButtonVariant.outline,
              onPressed: onSecondary,
            ),
          ],
        ],
      ),
    ),
  );
}

/// `OBM-CMP-SEC-001` — privacy scope pill (`PrivateLocal` / `LOCAL_ONLY` / `ReviewedExport`).
class ObmPrivacyScopePill extends StatelessWidget {
  const ObmPrivacyScopePill({
    required this.scopeLabel,
    this.tone = ObmStatusTone.pausedPrivate,
    super.key,
  });

  final String scopeLabel;
  final ObmStatusTone tone;

  @override
  Widget build(BuildContext context) =>
      ObmStatusBadge(label: scopeLabel, tone: tone);
}

/// `OBM-CMP-SEC-002` — explicit byte/field disclosure summary before cloud or export action.
class ObmDisclosureSummary extends StatelessWidget {
  const ObmDisclosureSummary({
    required this.destinationLabel,
    required this.byteCountLabel,
    required this.fieldsIncluded,
    required this.excludedSecretsNote,
    super.key,
  });

  final String destinationLabel;
  final String byteCountLabel;
  final List<String> fieldsIncluded;
  final String excludedSecretsNote;

  @override
  Widget build(BuildContext context) => ObmExactPlanPanel(
    title: destinationLabel,
    subtitle: excludedSecretsNote,
    statusLabel: byteCountLabel,
    tone: ObmStatusTone.waiting,
    facts: fieldsIncluded
        .map((field) => ObmPlanFact(label: field, value: 'INCLUDED'))
        .toList(growable: false),
  );
}

/// `OBM-CMP-SEC-003` — authority transition confirmation panel.
class ObmAuthorityTransitionPanel extends StatelessWidget {
  const ObmAuthorityTransitionPanel({
    required this.fromScope,
    required this.toScope,
    required this.explanation,
    required this.cancelLabel,
    required this.confirmLabel,
    required this.onCancel,
    required this.onConfirm,
    super.key,
  });

  final String fromScope;
  final String toScope;
  final String explanation;
  final String cancelLabel;
  final String confirmLabel;
  final VoidCallback? onCancel;
  final VoidCallback? onConfirm;

  @override
  Widget build(BuildContext context) => Card(
    child: Padding(
      padding: EdgeInsets.all(context.spacing.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              ObmPrivacyScopePill(scopeLabel: fromScope),
              SizedBox(width: context.spacing.sm),
              const Text('→'),
              SizedBox(width: context.spacing.sm),
              ObmPrivacyScopePill(
                scopeLabel: toScope,
                tone: ObmStatusTone.degraded,
              ),
            ],
          ),
          SizedBox(height: context.spacing.md),
          Text(explanation, style: Theme.of(context).textTheme.bodyMedium),
          SizedBox(height: context.spacing.md),
          ObmSplitReviewActions(
            deferLabel: cancelLabel,
            confirmLabel: confirmLabel,
            onDefer: onCancel,
            onConfirm: onConfirm,
          ),
        ],
      ),
    ),
  );
}

/// `OBM-CMP-SEC-004` — explicit session re-authentication gate card.
class ObmReauthGate extends StatelessWidget {
  const ObmReauthGate({
    required this.title,
    required this.body,
    required this.unlockLabel,
    required this.onUnlock,
    super.key,
  });

  final String title;
  final String body;
  final String unlockLabel;
  final VoidCallback? onUnlock;

  @override
  Widget build(BuildContext context) => Card(
    child: Padding(
      padding: EdgeInsets.all(context.spacing.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              const ObmIcon(ObmSymbol.lock),
              SizedBox(width: context.spacing.sm),
              Expanded(
                child: Text(
                  title,
                  style: Theme.of(context).textTheme.titleMedium,
                ),
              ),
            ],
          ),
          SizedBox(height: context.spacing.sm),
          Text(body, style: Theme.of(context).textTheme.bodyMedium),
          SizedBox(height: context.spacing.md),
          ObmButton(
            label: unlockLabel,
            leadingIcon: ObmSymbol.shield,
            onPressed: onUnlock,
          ),
        ],
      ),
    ),
  );
}

/// `OBM-CMP-SEC-005` — honest release-gate unavailable notice (`MOB-GATE-NETWORKED-BETA`).
class ObmGateUnavailableNotice extends StatelessWidget {
  const ObmGateUnavailableNotice({
    required this.gateId,
    required this.summary,
    required this.blockedCapabilities,
    super.key,
  });

  final String gateId;
  final String summary;
  final List<String> blockedCapabilities;

  @override
  Widget build(BuildContext context) {
    final palette = context.statusColors.resolve(
      ObmStatusTone.offlineUnavailable,
    );
    return Card(
      color: palette.container,
      child: Padding(
        padding: EdgeInsets.all(context.spacing.lg),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                ObmIcon(ObmSymbol.cloudOff, color: palette.content),
                SizedBox(width: context.spacing.sm),
                Expanded(
                  child: Text(
                    gateId,
                    style: Theme.of(
                      context,
                    ).textTheme.titleMedium?.copyWith(color: palette.content),
                  ),
                ),
                ObmStatusBadge(
                  label: 'GATED OFF',
                  tone: ObmStatusTone.offlineUnavailable,
                ),
              ],
            ),
            SizedBox(height: context.spacing.sm),
            Text(
              summary,
              style: Theme.of(
                context,
              ).textTheme.bodyMedium?.copyWith(color: palette.content),
            ),
            SizedBox(height: context.spacing.sm),
            for (final cap in blockedCapabilities)
              Padding(
                padding: EdgeInsets.only(top: context.spacing.xxs),
                child: Text(
                  '• $cap',
                  style: Theme.of(
                    context,
                  ).textTheme.bodySmall?.copyWith(color: palette.content),
                ),
              ),
          ],
        ),
      ),
    );
  }
}
