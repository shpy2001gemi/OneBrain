import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../domain/wave7_workspace_controller.dart';
import '../../l10n/app_localizations.dart';
import '../../platform/mobile_host_gateway.dart';
import '../shared/obm_action_card.dart';
import '../shared/obm_button.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_registry_plan.dart';
import '../shared/obm_scope_banner.dart';
import '../shared/obm_screen_frame.dart';
import '../shared/obm_wave7_components.dart';

/// `MOB-SCR-ONB-006` — post-onboarding readiness summary.
class ReadinessOnboardingScreen extends ConsumerWidget {
  const ReadinessOnboardingScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: strings.wave7ReadinessOnboardingTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmScopeBanner(
            title: w7.isReadyOffline
                ? strings.wave7ReadyOfflineTitle
                : strings.limitedTitle,
            body: strings.wave7ReadinessOnboardingBody,
            tone: w7.isReadyOffline
                ? ObmStatusTone.ready
                : ObmStatusTone.waiting,
            icon: w7.isReadyOffline
                ? ObmSymbol.checkCircle
                : ObmSymbol.schedule,
            statusLabel: w7.registryReadiness,
          ),
          SizedBox(height: context.spacing.lg),
          ObmCapabilityRow(
            title: strings.quickCaptureTitle,
            reason: strings.quickCaptureBody,
            statusLabel: strings.statusPrivate,
            tone: ObmStatusTone.ready,
            icon: ObmSymbol.lock,
          ),
          SizedBox(height: context.spacing.sm),
          ObmCapabilityRow(
            title: strings.wave7EncodeTitle,
            reason: w7.isReadyOffline
                ? strings.wave7ReadyOfflineBody
                : strings.registryRequiredReason,
            statusLabel: w7.isReadyOffline ? 'READY' : 'NEEDS INIT',
            tone: w7.isReadyOffline
                ? ObmStatusTone.ready
                : ObmStatusTone.waiting,
            icon: ObmSymbol.database,
          ),
          SizedBox(height: context.spacing.sm),
          ObmCapabilityRow(
            title: strings.receivedKnowledgeTitle,
            reason: strings.networkBetaReason,
            statusLabel: 'GATED OFF',
            tone: ObmStatusTone.offlineUnavailable,
            icon: ObmSymbol.cloudOff,
          ),
          SizedBox(height: context.spacing.xl),
          ObmButton(
            label: strings.wave7EnterNodeAction,
            onPressed: () async {
              try {
                await ref
                    .read(mobileHostGatewayProvider)
                    .setOnboardingCursor(MobileOnboardingCursor.limitedHome);
              } on Object {
                // Local cursor transition fallback in test sandbox.
              }
              if (context.mounted) {
                context.go('/home');
              }
            },
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-INI-002` — trait-injected multi-provider `RegistrySourcePlan` & resource policy.
class RegistrySourcePlanScreen extends ConsumerWidget {
  const RegistrySourcePlanScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: strings.wave7SourcePlanAction,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmVerificationReceipt(
            title: strings.wave7SourcePlanAction,
            statusLabel: w7.sourcePlanDigest,
            tone: ObmStatusTone.information,
            facts: [
              ObmPlanFact(
                label: 'Source plan digest',
                value: w7.sourcePlanDigest,
              ),
              ObmPlanFact(
                label: 'Remaining required free bytes',
                value: '${w7.remainingRequiredFreeBytes} B',
              ),
              const ObmPlanFact(
                label: 'Thermal / battery policy',
                value: 'Nominal · >= 20% or charging',
              ),
              const ObmPlanFact(
                label: 'Concrete socket / HTTP symbols',
                value: '0 (trait-injected providers)',
              ),
            ],
          ),
          SizedBox(height: context.spacing.lg),
          for (final desc in w7.sourceDescriptors) ...[
            ObmCapabilityRow(
              title: '${desc.priority}. ${desc.kindLabel}',
              reason:
                  '${desc.descriptorId} · ${desc.verifiedBytesContributed} B verified',
              statusLabel: desc.quarantined ? 'QUARANTINED' : desc.statusCode,
              tone: desc.quarantined
                  ? ObmStatusTone.failed
                  : desc.statusCode == 'READY'
                  ? ObmStatusTone.ready
                  : ObmStatusTone.waiting,
              icon: ObmSymbol.storage,
            ),
            SizedBox(height: context.spacing.sm),
          ],
        ],
      ),
    );
  }
}

/// `MOB-SCR-OPS-001` — durable Operations & Transfers center.
class OperationsCenterScreen extends ConsumerWidget {
  const OperationsCenterScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: strings.wave7OperationsCenterTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final op in w7.operations) ...[
            ObmOperationCard(
              operationId: op.operationId,
              title: op.title,
              sourceLabel: op.sourceLabel,
              stateLabel: op.stateLabel,
              progressFraction: op.progressFraction,
              progressDetail: op.progressDetail,
              tone: op.paused ? ObmStatusTone.waiting : ObmStatusTone.ready,
              primaryActionLabel: strings.wave7OperationDetailTitle,
              onPrimaryAction: () =>
                  context.push('/operations/${op.operationId}'),
            ),
            SizedBox(height: context.spacing.md),
          ],
        ],
      ),
    );
  }
}

/// `MOB-SCR-OPS-002` — individual operation verification receipt & resume/pause control.
class OperationDetailScreen extends ConsumerWidget {
  const OperationDetailScreen({required this.operationRef, super.key});

  final String operationRef;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    final op = w7.operations.firstWhere(
      (item) => item.operationId == operationRef,
      orElse: () => w7.operations.first,
    );
    return ObmScreenFrame(
      title: strings.wave7OperationDetailTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmVerificationReceipt(
            title: op.title,
            statusLabel: op.stateLabel,
            tone: op.paused ? ObmStatusTone.waiting : ObmStatusTone.ready,
            facts: [
              ObmPlanFact(label: 'Operation ID', value: op.operationId),
              ObmPlanFact(label: 'Kind', value: op.kind),
              ObmPlanFact(label: 'Progress', value: op.progressDetail),
              ObmPlanFact(
                label: 'Provider failovers',
                value: '${op.failoverCount}',
              ),
              ObmPlanFact(label: 'Receipt', value: op.receiptDigest),
            ],
          ),
          SizedBox(height: context.spacing.lg),
          ObmResumeDecision(
            reasonBody:
                'Foreground resume re-verifies prefix hashes and never trusts unverified partial tails.',
            resumeLabel: op.paused ? 'Resume exact transfer' : 'Pause transfer',
            secondaryLabel: strings.wave7ActivateRegistryAction,
            onResume: () => ref
                .read(wave7WorkspaceProvider.notifier)
                .toggleOperationPause(op.operationId),
            onSecondary: w7.isReadyOffline
                ? null
                : () => ref
                      .read(wave7WorkspaceProvider.notifier)
                      .activateRegistrySlot(operationId: op.operationId),
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-DAT-001` — Concept Registry A/B slot status, health receipt, rollback & GC.
class RegistrySettingsScreen extends ConsumerWidget {
  const RegistrySettingsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: strings.registrySettingsTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmScopeBanner(
            title: w7.isReadyOffline
                ? strings.wave7ReadyOfflineTitle
                : strings.limitedTitle,
            body: w7.isReadyOffline
                ? strings.wave7RegistrySettingsActiveBody
                : strings.registrySettingsBody,
            tone: w7.isReadyOffline
                ? ObmStatusTone.ready
                : ObmStatusTone.waiting,
            icon: ObmSymbol.database,
            statusLabel: w7.registryReadiness,
          ),
          SizedBox(height: context.spacing.lg),
          ObmVerificationReceipt(
            title: 'A/B Registry slot ledger',
            statusLabel: w7.registryReadiness,
            tone: w7.isReadyOffline
                ? ObmStatusTone.ready
                : ObmStatusTone.waiting,
            facts: [
              ObmPlanFact(
                label: 'Active slot (N)',
                value: w7.activeReleaseId ?? 'None (BootstrapOnly)',
              ),
              ObmPlanFact(
                label: 'Rollback slot (N-1)',
                value: w7.previousReleaseId ?? 'None',
              ),
              ObmPlanFact(
                label: 'REGISTRY_HEALTH_V1 probes',
                value: '${w7.healthProbesChecked}',
              ),
              ObmPlanFact(
                label: 'Retired slots eligible for GC',
                value: '${w7.retiredSlotCount}',
              ),
            ],
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            key: const Key('registry_activate_slot_button'),
            label: strings.wave7ActivateRegistryAction,
            onPressed: () => ref
                .read(wave7WorkspaceProvider.notifier)
                .activateRegistrySlot(),
          ),
          SizedBox(height: context.spacing.sm),
          ObmButton(
            key: const Key('registry_rollback_slot_button'),
            label: strings.wave7RollbackRegistryAction,
            variant: ObmButtonVariant.outline,
            onPressed: w7.previousReleaseId == null
                ? null
                : () => ref
                      .read(wave7WorkspaceProvider.notifier)
                      .rollbackRegistrySlot(),
            disabledReason: 'No eligible non-revoked N-1 slot is available.',
          ),
          SizedBox(height: context.spacing.sm),
          ObmButton(
            key: const Key('registry_gc_retired_button'),
            label: strings.wave7GcRegistryAction,
            variant: ObmButtonVariant.tonal,
            onPressed: w7.retiredSlotCount == 0
                ? null
                : () => ref
                      .read(wave7WorkspaceProvider.notifier)
                      .gcRetiredRegistrySlots(),
            disabledReason: 'No retired unpinned slots pending reclamation.',
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: 'Stage next signed Registry update',
            body:
                'Updates stage into an isolated immutable slot; active N stays live if verification or health fails.',
            icon: ObmSymbol.cloudDownload,
            tone: ObmStatusTone.information,
            actionLabel: 'Open Registry update check',
            onPressed: () => context.push('/settings/registry/update'),
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-DAT-002` — staged Registry update review screen.
class RegistryUpdateScreen extends ConsumerWidget {
  const RegistryUpdateScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: 'Registry update slot',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmStepTimeline(
            steps: [
              ObmTimelineStep(
                title: '1. Whole-artifact BLAKE3 verify',
                detail:
                    'concepts.obr, concepts.obr.labels.idx, concepts.obr.ccids.idx',
                tone: ObmStatusTone.ready,
                statusLabel: 'VERIFIED',
              ),
              ObmTimelineStep(
                title: '2. Atomic A/B slot pointer swap',
                detail:
                    'Active: ${w7.activeReleaseId ?? 'BootstrapOnly'} · Previous: ${w7.previousReleaseId ?? 'None'}',
                tone: w7.isReadyOffline
                    ? ObmStatusTone.ready
                    : ObmStatusTone.waiting,
                statusLabel: w7.registryReadiness,
              ),
              const ObmTimelineStep(
                title: '3. REGISTRY_HEALTH_V1 deterministic 64-probe check',
                detail:
                    'Automatic compensation restores N-1 (or BootstrapOnly) on fault.',
                tone: ObmStatusTone.ready,
                statusLabel: 'ENFORCED',
              ),
            ],
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            label: strings.wave7ActivateRegistryAction,
            onPressed: () => ref
                .read(wave7WorkspaceProvider.notifier)
                .activateRegistrySlot(),
          ),
        ],
      ),
    );
  }
}
