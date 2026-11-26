import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../domain/wave7_workspace_controller.dart';
import '../../l10n/app_localizations.dart';
import '../shared/obm_button.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_registry_plan.dart';
import '../shared/obm_scope_banner.dart';
import '../shared/obm_screen_frame.dart';
import '../shared/obm_wave7_components.dart';

/// `MOB-SCR-AI-002` — dedicated Assistant thread view.
class AssistantThreadScreen extends ConsumerWidget {
  const AssistantThreadScreen({required this.threadRef, super.key});

  final String threadRef;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: 'Thread $threadRef',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final turn in w7.assistantTurns) ...[
            ObmAssistantMessage(
              roleLabel: turn.roleLabel,
              body: turn.body,
              isUser: turn.isUser,
              providerBadge: turn.providerBadge,
            ),
            SizedBox(height: context.spacing.sm),
          ],
        ],
      ),
    );
  }
}

/// `MOB-SCR-AI-003` — explicit scoped context attachment selector.
class AssistantContextScreen extends ConsumerWidget {
  const AssistantContextScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: 'Scoped assistant context',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const ObmScopeBanner(
            title: 'Zero implicit vault scraping',
            body:
                'Only explicitly attached refs enter the assistant turn context. Switching from local to cloud resets all attached refs.',
            tone: ObmStatusTone.pausedPrivate,
            icon: ObmSymbol.lock,
          ),
          SizedBox(height: context.spacing.md),
          Wrap(
            spacing: context.spacing.sm,
            runSpacing: context.spacing.sm,
            children: [
              for (final refLabel in w7.attachedContextRefs)
                ObmContextCapsule(
                  refLabel: refLabel,
                  scopeLabel: 'LOCAL_ONLY',
                  onRemove: () => ref
                      .read(wave7WorkspaceProvider.notifier)
                      .removeContextRef(refLabel),
                ),
            ],
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            label: 'Attach active KU summary ref',
            onPressed: () => ref
                .read(wave7WorkspaceProvider.notifier)
                .attachContextRef('ku_ref_local_01'),
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-AI-004` — explicit Cloud `ContextDisclosureGate`.
class AssistantDisclosureGateScreen extends ConsumerWidget {
  const AssistantDisclosureGateScreen({required this.requestRef, super.key});

  final String requestRef;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: strings.wave7CloudDisclosureTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmDisclosureSummary(
            destinationLabel: 'Cloud disclosure request ($requestRef)',
            byteCountLabel: 'EXPLICIT CONSENT REQUIRED',
            fieldsIncluded: const [
              'Prompt UTF-8 bytes only',
              'Local -> Cloud context reset enforced',
            ],
            excludedSecretsNote:
                'Private vault keys, unattached drafts and local filesystem paths are never disclosed.',
          ),
          SizedBox(height: context.spacing.lg),
          ObmAuthorityTransitionPanel(
            fromScope: 'PrivateLocal',
            toScope: 'DisclosedCloudTurn',
            explanation:
                'Approving resets prior local-only context capsules before any cloud provider turn can be prepared.',
            cancelLabel: strings.backAction,
            confirmLabel: 'Approve disclosure & reset local context',
            onCancel: () => context.pop(),
            onConfirm: () => ref
                .read(wave7WorkspaceProvider.notifier)
                .approveCloudDisclosureAndResetContext(),
          ),
          if (w7.cloudDisclosureApproved) ...[
            SizedBox(height: context.spacing.md),
            const ObmScopeBanner(
              title: 'Disclosure receipt recorded',
              body: 'Local context capsules were cleared.',
              tone: ObmStatusTone.ready,
              icon: ObmSymbol.checkCircle,
            ),
          ],
        ],
      ),
    );
  }
}

/// `MOB-SCR-AI-005` — durable `ToolOrchestrator` proposal consent screen.
class ToolProposalConsentScreen extends ConsumerWidget {
  const ToolProposalConsentScreen({required this.proposalRef, super.key});

  final String proposalRef;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    final proposal = w7.toolProposals
        .where((p) => p.proposalId == proposalRef)
        .firstOrNull;
    if (proposal == null) {
      return ObmScreenFrame(
        title: strings.wave7ToolConsentTitle,
        child: Text(strings.unavailableTitle),
      );
    }
    return ObmScreenFrame(
      title: strings.wave7ToolConsentTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmToolProposalCard(
            proposalId: proposal.proposalId,
            toolName: proposal.toolName,
            argumentsSummary: proposal.argumentsJson,
            stateLabel: proposal.journalState,
            approveLabel: strings.wave7ToolApproveAction,
            denyLabel: strings.wave7ToolDenyAction,
            onApprove: proposal.journalState == 'AwaitingConsent'
                ? () {
                    ref
                        .read(wave7WorkspaceProvider.notifier)
                        .approveToolProposal(proposal.proposalId);
                    context.push('/assistant/tool/${proposal.proposalId}');
                  }
                : null,
            onDeny: proposal.journalState == 'AwaitingConsent'
                ? () => ref
                      .read(wave7WorkspaceProvider.notifier)
                      .denyToolProposal(proposal.proposalId)
                : null,
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-AI-006` — durable `ToolOrchestrator` execution receipt.
class ToolExecutionReceiptScreen extends ConsumerWidget {
  const ToolExecutionReceiptScreen({required this.operationRef, super.key});

  final String operationRef;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final w7 = ref.watch(wave7WorkspaceProvider);
    final proposal = w7.toolProposals
        .where((p) => p.proposalId == operationRef)
        .firstOrNull;
    return ObmScreenFrame(
      title: 'Tool execution receipt',
      child: ObmToolResultCard(
        proposalId: operationRef,
        toolName: proposal?.toolName ?? 'bounded_tool',
        stateLabel: proposal?.journalState ?? 'Unknown',
        summary:
            proposal?.resultSummary ??
            'Crash recovery marks interrupted Executing non-idempotent tools as Unknown and refuses blind replay.',
        tone: proposal?.journalState == 'Committed'
            ? ObmStatusTone.ready
            : ObmStatusTone.degraded,
      ),
    );
  }
}

/// `MOB-SCR-AI-007` — AI provider mode & boundary overview.
class AssistantProviderScreen extends ConsumerWidget {
  const AssistantProviderScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: 'Assistant provider boundary',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmVerificationReceipt(
            title: 'Active provider',
            statusLabel: w7.activeLlmProvider,
            tone: w7.activeLlmProvider == 'No LLM'
                ? ObmStatusTone.information
                : ObmStatusTone.ready,
            facts: [
              ObmPlanFact(label: 'Active route', value: w7.activeLlmProvider),
              const ObmPlanFact(
                label: 'Default fresh-install mode',
                value: 'No LLM (noLlmProvider == true)',
              ),
              const ObmPlanFact(
                label: 'Partial/cancelled stream tools',
                value: 'Never executed',
              ),
              const ObmPlanFact(
                label: 'FORBIDDEN_TO_LLM',
                value: 'Hard-blocked in Rust ToolOrchestrator',
              ),
            ],
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            label: strings.wave7ModelsTitle,
            onPressed: () => context.push('/settings/ai/models'),
          ),
          SizedBox(height: context.spacing.sm),
          ObmButton(
            label: strings.wave7CloudDisclosureTitle,
            variant: ObmButtonVariant.outline,
            onPressed: () => context.push('/assistant/disclosure/req_cloud_01'),
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-MOD-001` — AI & local model settings hub.
class AiSettingsHubScreen extends ConsumerWidget {
  const AiSettingsHubScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: 'AI & local models',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmScopeBanner(
            title: w7.activeLlmProvider,
            body:
                'No model weights are bundled in the APK or IPA. Local models require an Ed25519-signed manifest and whole-file BLAKE3 verification.',
            tone: ObmStatusTone.information,
            icon: ObmSymbol.memory,
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            label: strings.wave7ModelsTitle,
            onPressed: () => context.push('/settings/ai/models'),
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-MOD-002` — Signed local model catalog list.
class SignedModelCatalogScreen extends ConsumerWidget {
  const SignedModelCatalogScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: strings.wave7ModelsTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final model in w7.models) ...[
            ObmModelDownloadCard(
              modelId: model.modelId,
              releaseId: model.releaseId,
              quantization: model.quantization,
              sizeLabel: model.sizeLabel,
              stateLabel: model.state,
              tone: model.state == 'Active'
                  ? ObmStatusTone.ready
                  : ObmStatusTone.waiting,
              primaryLabel: 'Inspect signed manifest',
              onPrimary: () =>
                  context.push('/settings/ai/models/${model.releaseId}'),
            ),
            SizedBox(height: context.spacing.md),
          ],
        ],
      ),
    );
  }
}

/// `MOB-SCR-MOD-003` — Signed model verify / activate / rollback detail.
class SignedModelDetailScreen extends ConsumerWidget {
  const SignedModelDetailScreen({required this.releaseRef, super.key});

  final String releaseRef;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    final model = w7.models.firstWhere(
      (m) => m.releaseId == releaseRef,
      orElse: () => w7.models.first,
    );
    return ObmScreenFrame(
      title: model.modelId,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmModelDownloadCard(
            modelId: model.modelId,
            releaseId: model.releaseId,
            quantization: model.quantization,
            sizeLabel: model.sizeLabel,
            stateLabel: model.state,
            tone: model.state == 'Active'
                ? ObmStatusTone.ready
                : ObmStatusTone.information,
            primaryLabel: strings.wave7ActivateModelAction,
            onPrimary: () => ref
                .read(wave7WorkspaceProvider.notifier)
                .activateSignedModel(model.releaseId),
            secondaryLabel: strings.wave7RollbackModelAction,
            onSecondary: () =>
                ref.read(wave7WorkspaceProvider.notifier).rollbackSignedModel(),
          ),
        ],
      ),
    );
  }
}
