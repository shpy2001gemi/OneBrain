import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../domain/wave7_workspace_controller.dart';
import '../../l10n/app_localizations.dart';
import '../shared/obm_action_card.dart';
import '../shared/obm_app_shell.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_shell_destinations.dart';

class SettingsScreen extends ConsumerWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmAppShell(
      title: strings.settingsTitle,
      selectedIndex: 4,
      destinations: obmShellDestinations(strings),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            strings.settingsTitle,
            style: Theme.of(context).textTheme.headlineLarge,
          ),
          SizedBox(height: context.spacing.md),
          Text(
            strings.settingsBody,
            style: Theme.of(context).textTheme.bodyLarge,
          ),
          SizedBox(height: context.spacing.lg),
          ObmActionCard(
            title: strings.runtimeSettingsTitle,
            body: strings.runtimeSettingsBody,
            icon: ObmSymbol.memory,
            tone: ObmStatusTone.ready,
            statusLabel: strings.statusReady,
            actionLabel: strings.wave7DiagnosticsTitle,
            onPressed: () => context.push('/settings/diagnostics'),
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.registrySettingsTitle,
            body: w7.isReadyOffline
                ? strings.wave7RegistrySettingsActiveBody
                : strings.registrySettingsBody,
            icon: ObmSymbol.database,
            tone: w7.isReadyOffline
                ? ObmStatusTone.ready
                : ObmStatusTone.waiting,
            statusLabel: w7.registryReadiness,
            actionLabel: strings.wave7OpenRegistrySettingsAction,
            onPressed: () => context.push('/settings/registry'),
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.storageSettingsTitle,
            body: strings.storageSettingsBody,
            icon: ObmSymbol.storage,
            tone: ObmStatusTone.information,
            actionLabel: strings.storageSettingsTitle,
            onPressed: () => context.push('/settings/storage'),
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.backupSettingsTitle,
            body: strings.backupSettingsBody,
            icon: ObmSymbol.backup,
            tone: ObmStatusTone.pausedPrivate,
            actionLabel: strings.backupSettingsTitle,
            onPressed: () => context.push('/settings/backup'),
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.wave7ModelsTitle,
            body: 'Active provider: ${w7.activeLlmProvider}',
            icon: ObmSymbol.assistant,
            tone: ObmStatusTone.information,
            actionLabel: strings.wave7ModelsTitle,
            onPressed: () => context.push('/settings/ai'),
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.wave7ActivityInboxTitle,
            body: '${w7.activityItems.length} durable receipt(s)',
            icon: ObmSymbol.operations,
            tone: ObmStatusTone.information,
            actionLabel: strings.wave7ActivityInboxTitle,
            onPressed: () => context.push('/activity'),
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.wave7AdvancedGatesTitle,
            body: strings.wave7NetworkedBetaNotice,
            icon: ObmSymbol.shield,
            tone: ObmStatusTone.offlineUnavailable,
            actionLabel: strings.wave7AdvancedGatesTitle,
            onPressed: () => context.push('/settings/advanced'),
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.languageSettingsTitle,
            body: strings.languageSettingsBody,
            icon: ObmSymbol.translate,
            tone: ObmStatusTone.information,
            actionLabel: strings.languageSettingsTitle,
            onPressed: () => context.push('/settings/language'),
          ),
        ],
      ),
    );
  }
}
