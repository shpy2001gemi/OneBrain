import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../l10n/app_localizations.dart';
import '../shared/obm_action_card.dart';
import '../shared/obm_app_shell.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_shell_destinations.dart';

class SettingsScreen extends StatelessWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
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
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.registrySettingsTitle,
            body: strings.registrySettingsBody,
            icon: ObmSymbol.database,
            tone: ObmStatusTone.waiting,
            statusLabel: strings.statusWaiting,
            actionLabel: strings.openInitAction,
            onPressed: () => context.push('/init'),
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.storageSettingsTitle,
            body: strings.storageSettingsBody,
            icon: ObmSymbol.storage,
            tone: ObmStatusTone.information,
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.backupSettingsTitle,
            body: strings.backupSettingsBody,
            icon: ObmSymbol.backup,
            tone: ObmStatusTone.offlineUnavailable,
            actionLabel: strings.backupSettingsTitle,
            onPressed: null,
            disabledReason: strings.notImplementedBody,
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.languageSettingsTitle,
            body: strings.languageSettingsBody,
            icon: ObmSymbol.translate,
            tone: ObmStatusTone.information,
          ),
        ],
      ),
    );
  }
}
