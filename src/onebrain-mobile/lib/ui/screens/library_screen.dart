import 'package:flutter/material.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../l10n/app_localizations.dart';
import '../shared/obm_action_card.dart';
import '../shared/obm_app_shell.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_scope_banner.dart';
import '../shared/obm_shell_destinations.dart';

class LibraryScreen extends StatelessWidget {
  const LibraryScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    return ObmAppShell(
      title: strings.libraryTitle,
      selectedIndex: 1,
      destinations: obmShellDestinations(strings),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            strings.libraryTitle,
            style: Theme.of(context).textTheme.headlineLarge,
          ),
          SizedBox(height: context.spacing.md),
          Text(
            strings.libraryBody,
            style: Theme.of(context).textTheme.bodyLarge,
          ),
          SizedBox(height: context.spacing.lg),
          ObmScopeBanner(
            title: strings.limitedTitle,
            body: strings.registryRequiredReason,
            tone: ObmStatusTone.waiting,
            icon: ObmSymbol.database,
          ),
          SizedBox(height: context.spacing.lg),
          ObmActionCard(
            title: strings.myKnowledgeTitle,
            body: strings.myKnowledgeBody,
            icon: ObmSymbol.library,
            tone: ObmStatusTone.offlineUnavailable,
            actionLabel: strings.navLibrary,
            onPressed: null,
            disabledReason: strings.registryRequiredReason,
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.receivedKnowledgeTitle,
            body: strings.receivedKnowledgeBody,
            icon: ObmSymbol.cloudOff,
            tone: ObmStatusTone.offlineUnavailable,
            actionLabel: strings.navLibrary,
            onPressed: null,
            disabledReason: strings.networkBetaReason,
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.mediaLibraryTitle,
            body: strings.mediaLibraryBody,
            icon: ObmSymbol.folder,
            tone: ObmStatusTone.offlineUnavailable,
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.conceptsTitle,
            body: strings.conceptsBody,
            icon: ObmSymbol.search,
            tone: ObmStatusTone.offlineUnavailable,
            actionLabel: strings.navLibrary,
            onPressed: null,
            disabledReason: strings.registryRequiredReason,
          ),
        ],
      ),
    );
  }
}
