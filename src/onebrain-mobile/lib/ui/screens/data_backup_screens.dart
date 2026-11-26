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

/// `MOB-SCR-DAT-003` — Storage breakdown across private vaults, Registry, media & derived cache.
class StorageBreakdownScreen extends ConsumerWidget {
  const StorageBreakdownScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    final media = ref.watch(ownedMediaProvider).asData?.value ?? const [];
    return ObmScreenFrame(
      title: strings.storageSettingsTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmVerificationReceipt(
            title: strings.storageSettingsTitle,
            statusLabel: 'ISOLATED DOMAINS',
            tone: ObmStatusTone.information,
            facts: [
              ObmPlanFact(
                label: 'Saved LOCAL_ONLY KUs',
                value: '${w7.savedKus.length} item(s)',
              ),
              ObmPlanFact(
                label: 'OwnedOriginal media (GC-protected)',
                value: '${media.length} item(s)',
              ),
              ObmPlanFact(
                label: 'Reclaimable derived cache',
                value: '${w7.derivedCacheBytes} B',
              ),
              ObmPlanFact(
                label: 'Retired Registry slots',
                value: '${w7.retiredSlotCount}',
              ),
            ],
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            label: strings.wave7StorageCleanupTitle,
            onPressed: () => context.push('/settings/storage/cleanup'),
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-DAT-004` — Derived-cache & staging GC (never evicts `OwnedOriginal`!).
class StorageCleanupScreen extends ConsumerWidget {
  const StorageCleanupScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: strings.wave7StorageCleanupTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmScopeBanner(
            title: 'OwnedOriginal hold invariant',
            body:
                'Two-phase recoverable GC evicts derived thumbnails and uncommitted staging only. OwnedOriginal encrypted packs and private drafts are never evicted.',
            tone: ObmStatusTone.pausedPrivate,
            icon: ObmSymbol.shield,
          ),
          SizedBox(height: context.spacing.lg),
          ObmVerificationReceipt(
            title: 'Reclaimable derived bytes',
            statusLabel: '${w7.derivedCacheBytes} B',
            tone: w7.derivedCacheBytes == 0
                ? ObmStatusTone.ready
                : ObmStatusTone.waiting,
            facts: [
              ObmPlanFact(
                label: 'Derived cache bytes',
                value: '${w7.derivedCacheBytes} B',
              ),
              const ObmPlanFact(
                label: 'OwnedOriginal eviction allowed',
                value: 'NEVER (0)',
              ),
            ],
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            key: const Key('run_derived_cache_gc_button'),
            label: strings.wave7RunGcAction,
            onPressed: w7.derivedCacheBytes == 0
                ? null
                : () => ref
                      .read(wave7WorkspaceProvider.notifier)
                      .reclaimDerivedCachesPreservingOwnedOriginals(),
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-DAT-005` — Authenticated `OBARV002` encrypted backup creation.
class BackupSettingsScreen extends ConsumerWidget {
  const BackupSettingsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: strings.backupSettingsTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmScopeBanner(
            title: 'OBARV002 authenticated archive',
            body:
                'Encrypts private drafts, LOCAL_ONLY KUs, fidelity portfolio and Registry high-water tuples with a user RecoveryKey. Installation private signing keys are non-exportable.',
            tone: ObmStatusTone.pausedPrivate,
            icon: ObmSymbol.backup,
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            key: const Key('create_obarv002_backup_button'),
            label: strings.wave7BackupCreateAction,
            leadingIcon: ObmSymbol.backup,
            onPressed: () => ref
                .read(wave7WorkspaceProvider.notifier)
                .createEncryptedBackupV2(),
          ),
          SizedBox(height: context.spacing.sm),
          ObmButton(
            label: strings.wave7RestoreVerifyAction,
            variant: ObmButtonVariant.outline,
            onPressed: () => context.push('/settings/restore'),
          ),
          SizedBox(height: context.spacing.sm),
          ObmButton(
            label: strings.wave7ScopedExportAction,
            variant: ObmButtonVariant.tonal,
            onPressed: () => context.push('/settings/export'),
          ),
          if (w7.backupReceipts.isNotEmpty) ...[
            SizedBox(height: context.spacing.lg),
            for (final id in w7.backupReceipts)
              ObmActionCard(
                title: id,
                body: 'Authenticated OBARV002 container sealed locally.',
                icon: ObmSymbol.checkCircle,
                tone: ObmStatusTone.ready,
              ),
          ],
        ],
      ),
    );
  }
}

/// `MOB-SCR-DAT-006` — Verified-spool `OBARV002` staged restore (`ImportDataKeepCurrentIdentity`).
class RestoreBackupScreen extends ConsumerWidget {
  const RestoreBackupScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: strings.wave7RestoreVerifyAction,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const ObmDisclosureSummary(
            destinationLabel: 'ImportDataKeepCurrentIdentity',
            byteCountLabel: 'OBARV002 ONLY',
            fieldsIncluded: [
              'Verified spool materialization before commit',
              'Local installation_epoch preserved',
              'Whole-tuple Registry high-water merge (equivocation fail-closed)',
              'Legacy OBARV001 rejected as restore source',
            ],
            excludedSecretsNote:
                'Foreign installation authority never overwrites this device node identity.',
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            key: const Key('verify_stage_restore_button'),
            label: strings.wave7RestoreVerifyAction,
            onPressed: () => ref
                .read(wave7WorkspaceProvider.notifier)
                .verifyAndStageRestoreV2(),
          ),
          if (w7.lastRestoreSummary != null) ...[
            SizedBox(height: context.spacing.lg),
            ObmScopeBanner(
              title: 'Staged restore verified',
              body: w7.lastRestoreSummary!,
              tone: ObmStatusTone.ready,
              icon: ObmSymbol.checkCircle,
            ),
          ],
        ],
      ),
    );
  }
}

/// `MOB-SCR-DAT-007` — Explicit user-reviewed scoped bundle export.
class ScopedExportScreen extends ConsumerWidget {
  const ScopedExportScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: strings.wave7ScopedExportAction,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmAuthorityTransitionPanel(
            fromScope: 'PrivateLocal',
            toScope: 'ReviewedExportFile',
            explanation:
                'Exports only explicitly selected KU / draft summaries to a user-chosen file without raw vault keys or network transmission.',
            cancelLabel: strings.backAction,
            confirmLabel: strings.wave7ScopedExportAction,
            onCancel: () => context.pop(),
            onConfirm: () => ref
                .read(wave7WorkspaceProvider.notifier)
                .exportReviewedScopedBundle(),
          ),
          if (w7.lastExportSummary != null) ...[
            SizedBox(height: context.spacing.lg),
            ObmScopeBanner(
              title: 'Scoped export prepared',
              body: w7.lastExportSummary!,
              tone: ObmStatusTone.ready,
              icon: ObmSymbol.checkCircle,
            ),
          ],
        ],
      ),
    );
  }
}
