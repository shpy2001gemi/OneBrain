import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../app/locale_controller.dart';
import '../../design/onebrain_theme_extensions.dart';
import '../../domain/wave7_workspace_controller.dart';
import '../../l10n/app_localizations.dart';
import '../shared/obm_action_card.dart';
import '../shared/obm_button.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_registry_plan.dart';
import '../shared/obm_scope_banner.dart';
import '../shared/obm_screen_frame.dart';
import '../shared/obm_wave7_components.dart';

/// `MOB-SCR-MED-001` — Verified-range progressive private media viewer.
class MediaVerifiedViewerScreen extends ConsumerWidget {
  const MediaVerifiedViewerScreen({required this.manifestRef, super.key});

  final String manifestRef;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    final verifiedPieces = w7.verifiedMediaRangeByRef[manifestRef] ?? 1;
    return ObmScreenFrame(
      title: strings.wave7MediaViewerTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmVerificationReceipt(
            title: manifestRef,
            statusLabel: 'OWNED_ORIGINAL · PRIVATE_LOCAL',
            tone: ObmStatusTone.ready,
            facts: [
              const ObmPlanFact(
                label: 'Logical piece size',
                value: '262,144 B (256 KiB)',
              ),
              ObmPlanFact(
                label: 'Verified pieces streamed',
                value: '$verifiedPieces',
              ),
              const ObmPlanFact(
                label: 'AEAD + BLAKE3 leaf check',
                value: 'Verified before byte exposure',
              ),
            ],
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            key: const Key('verify_next_media_range_button'),
            label: 'Seek & verify next 256 KiB piece',
            onPressed: () => ref
                .read(wave7WorkspaceProvider.notifier)
                .verifyNextMediaPieceRange(manifestRef),
          ),
          SizedBox(height: context.spacing.sm),
          ObmButton(
            label: strings.wave7MediaInfoTitle,
            variant: ObmButtonVariant.outline,
            onPressed: () => context.push('/library/media/$manifestRef/info'),
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-MED-002` — Logical-piece / physical-pack manifest & pin toggle.
class MediaManifestInfoScreen extends ConsumerWidget {
  const MediaManifestInfoScreen({required this.manifestRef, super.key});

  final String manifestRef;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    final pinned = w7.pinnedMediaRefs.contains(manifestRef);
    return ObmScreenFrame(
      title: strings.wave7MediaInfoTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmVerificationReceipt(
            title: 'SignedMediaManifest',
            statusLabel: pinned ? 'PINNED + OWNED_HOLD' : 'OWNED_HOLD',
            tone: ObmStatusTone.pausedPrivate,
            facts: [
              ObmPlanFact(label: 'Media reference', value: manifestRef),
              const ObmPlanFact(
                label: 'Retention class',
                value: 'OwnedOriginal (GC-exempt)',
              ),
              const ObmPlanFact(
                label: 'Seed / share eligibility',
                value: 'false (PrivateLocal)',
              ),
            ],
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            label: pinned ? 'Unpin local priority' : 'Pin local priority',
            variant: ObmButtonVariant.tonal,
            onPressed: () => ref
                .read(wave7WorkspaceProvider.notifier)
                .toggleMediaPin(manifestRef),
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-NTF-001` — Durable Activity & Approvals inbox in `redb`.
class ActivityInboxScreen extends ConsumerWidget {
  const ActivityInboxScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: strings.wave7ActivityInboxTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const ObmScopeBanner(
            title: 'Observable receipt truth',
            body:
                'States reflect Scheduled, Submitted, ActiveObserved or Interacted — never claiming OS delivery from submission alone.',
            tone: ObmStatusTone.information,
            icon: ObmSymbol.info,
          ),
          SizedBox(height: context.spacing.lg),
          for (final item in w7.activityItems) ...[
            ObmActionCard(
              title: item.title,
              body: item.body,
              icon: ObmSymbol.operations,
              tone: item.actionCompleted
                  ? ObmStatusTone.ready
                  : ObmStatusTone.waiting,
              statusLabel: item.observableReceiptState,
              actionLabel: 'Inspect activity receipt',
              onPressed: () => context.push('/activity/${item.intentRef}'),
            ),
            SizedBox(height: context.spacing.sm),
          ],
        ],
      ),
    );
  }
}

/// `MOB-SCR-NTF-002` — Individual Activity receipt & one-time-nonce reversible action.
class ActivityDetailScreen extends ConsumerWidget {
  const ActivityDetailScreen({required this.intentRef, super.key});

  final String intentRef;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final w7 = ref.watch(wave7WorkspaceProvider);
    final item = w7.activityItems.firstWhere(
      (i) => i.intentRef == intentRef,
      orElse: () => w7.activityItems.first,
    );
    return ObmScreenFrame(
      title: item.title,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmVerificationReceipt(
            title: item.title,
            statusLabel: item.observableReceiptState,
            tone: ObmStatusTone.information,
            facts: [
              ObmPlanFact(label: 'Intent ref', value: item.intentRef),
              ObmPlanFact(label: 'Category', value: item.category),
              ObmPlanFact(
                label: 'Observable receipt',
                value: item.observableReceiptState,
              ),
              const ObmPlanFact(
                label: 'Irreversible background actions',
                value: 'Forbidden fail-closed',
              ),
            ],
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            label: item.actionCompleted
                ? 'Reversible action completed'
                : 'Acknowledge (one-time nonce)',
            onPressed: item.actionCompleted
                ? null
                : () => ref
                      .read(wave7WorkspaceProvider.notifier)
                      .executeReversibleActivityAction(item.intentRef),
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-NTF-003` — Notification channels & quiet hours policy.
class NotificationSettingsScreen extends ConsumerWidget {
  const NotificationSettingsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: strings.wave7NotificationsSettingsTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          SwitchListTile(
            value: w7.quietHoursEnabled,
            title: const Text('Quiet hours (suppress non-critical banners)'),
            subtitle: const Text('Durable Activity inbox still records intents'),
            onChanged: (v) => ref
                .read(wave7WorkspaceProvider.notifier)
                .toggleQuietHours(v),
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-SYS-001` — Bilingual `en`/`vi` interface switcher.
class LanguageSettingsScreen extends ConsumerWidget {
  const LanguageSettingsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    return ObmScreenFrame(
      title: strings.languageSettingsTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmButton(
            label: strings.languageEnglish,
            onPressed: () => ref
                .read(localeControllerProvider.notifier)
                .select(const Locale('en')),
          ),
          SizedBox(height: context.spacing.sm),
          ObmButton(
            label: strings.languageVietnamese,
            variant: ObmButtonVariant.outline,
            onPressed: () => ref
                .read(localeControllerProvider.notifier)
                .select(const Locale('vi')),
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-SYS-002` — Accessibility, 200% text scaling, high contrast & Reduce Motion.
class AccessibilitySettingsScreen extends StatelessWidget {
  const AccessibilitySettingsScreen({super.key});

  @override
  Widget build(BuildContext context) => ObmScreenFrame(
    title: 'Accessibility contract',
    child: ObmVerificationReceipt(
      title: 'WCAG AA & OS accessibility',
      statusLabel: 'ACTIVE',
      tone: ObmStatusTone.ready,
      facts: [
        ObmPlanFact(
          label: 'Disable animations',
          value: '${MediaQuery.disableAnimationsOf(context)}',
        ),
        ObmPlanFact(
          label: 'High contrast',
          value: '${MediaQuery.highContrastOf(context)}',
        ),
        const ObmPlanFact(label: 'Minimum touch target', value: '48x48 dp'),
      ],
    ),
  );
}

/// `MOB-SCR-SYS-003` — OS permissions & zero-INTERNET Bootstrap boundary.
class PermissionsSettingsScreen extends StatelessWidget {
  const PermissionsSettingsScreen({super.key});

  @override
  Widget build(BuildContext context) => const ObmScreenFrame(
    title: 'Permissions boundary',
    child: ObmVerificationReceipt(
      title: 'Zero ambient permissions',
      statusLabel: 'MINIMAL',
      tone: ObmStatusTone.ready,
      facts: [
        ObmPlanFact(
          label: 'android.permission.INTERNET (Bootstrap)',
          value: 'ABSENT',
        ),
        ObmPlanFact(
          label: 'Media & Registry files',
          value: 'System ACTION_OPEN_DOCUMENT picker only',
        ),
        ObmPlanFact(
          label: 'Cloud / device backup extraction',
          value: 'Excluded via data_extraction_rules.xml & iOS flag',
        ),
      ],
    ),
  );
}

/// `MOB-SCR-SYS-004` — Single `ActivationArbiter` & zero-idle-polling energy policy.
class BackgroundPolicyScreen extends StatelessWidget {
  const BackgroundPolicyScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    return ObmScreenFrame(
      title: strings.wave7BackgroundPolicyTitle,
      child: const ObmVerificationReceipt(
        title: 'ActivationArbiter energy ledger',
        statusLabel: 'ZERO IDLE POLLING',
        tone: ObmStatusTone.ready,
        facts: [
          ObmPlanFact(label: 'Authority', value: 'Single Rust ActivationArbiter'),
          ObmPlanFact(label: 'Idle background timers', value: '0'),
          ObmPlanFact(
            label: 'Stale process generation callbacks',
            value: 'Fenced & rejected',
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-SYS-005` — Privacy-safe redacted diagnostics export.
class DiagnosticsScreen extends ConsumerWidget {
  const DiagnosticsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: strings.wave7DiagnosticsTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const ObmScopeBanner(
            title: 'Redacted by construction',
            body:
                'Contains bounded counters and schema versions only. Never includes filesystem paths, content:// URIs, private keys or plaintext drafts.',
            tone: ObmStatusTone.pausedPrivate,
            icon: ObmSymbol.shield,
          ),
          SizedBox(height: context.spacing.lg),
          Card(
            child: Padding(
              padding: EdgeInsets.all(context.spacing.md),
              child: SelectableText(
                w7.redactedDiagnosticsJson,
                style: context.dataStyle.value,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-SYS-006` — Honest `MOB-GATE-NETWORKED-BETA` status & schema migration matrix.
class AdvancedGatesScreen extends ConsumerWidget {
  const AdvancedGatesScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    return ObmScreenFrame(
      title: strings.wave7AdvancedGatesTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmGateUnavailableNotice(
            gateId: 'MOB-GATE-NETWORKED-BETA',
            summary: strings.wave7NetworkedBetaNotice,
            blockedCapabilities: const [
              'MOB-SCR-NET-001..008 (P2P node & relay networking)',
              'MOB-SCR-MAT-001..005 (Active network need/match)',
              'MOB-SCR-FID-002..010 (Blind external fidelity network)',
              'MOB-SCR-PUB-001..003 (Public knowledge publication)',
              'Wallet / M7 & Active M6 KQL',
            ],
          ),
          SizedBox(height: context.spacing.lg),
          const ObmVerificationReceipt(
            title: 'MigrationCoordinator & passive OBP engine',
            statusLabel: '_schema_meta v2',
            tone: ObmStatusTone.ready,
            facts: [
              ObmPlanFact(
                label: 'Passive OBP reunion engine',
                value: 'Quarantined · std::net-free · flag=false',
              ),
              ObmPlanFact(
                label: 'Additive redb schema migration',
                value: 'Single-txn _schema_meta v2',
              ),
              ObmPlanFact(
                label: 'Rollback safety modes',
                value: 'NMinusOneReadWrite / ReverseBridge / PreWriteOnly',
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-SYS-007` — Autonomous mobile node build & contract summary.
class AboutNodeScreen extends StatelessWidget {
  const AboutNodeScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    return ObmScreenFrame(
      title: strings.wave7AboutTitle,
      child: const ObmVerificationReceipt(
        title: 'OneBrain Autonomous Mobile Node',
        statusLabel: 'ARCH-10 SHARED CORE',
        tone: ObmStatusTone.ready,
        facts: [
          ObmPlanFact(label: 'Architecture', value: 'Independent mobile node'),
          ObmPlanFact(label: 'Shared core', value: 'onebrain-core + ku-core'),
          ObmPlanFact(label: 'Bundled Registry / LLM payloads', value: '0 bytes'),
        ],
      ),
    );
  }
}
