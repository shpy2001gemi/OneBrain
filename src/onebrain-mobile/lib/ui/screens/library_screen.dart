import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../domain/wave7_workspace_controller.dart';
import '../../l10n/app_localizations.dart';
import '../shared/obm_action_card.dart';
import '../shared/obm_app_shell.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_scope_banner.dart';
import '../shared/obm_shell_destinations.dart';

class LibraryScreen extends ConsumerWidget {
  const LibraryScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    final ready = w7.isReadyOffline;
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
            title: ready ? strings.wave7ReadyOfflineTitle : strings.limitedTitle,
            body: ready
                ? strings.wave7ReadyOfflineBody
                : strings.registryRequiredReason,
            tone: ready ? ObmStatusTone.ready : ObmStatusTone.waiting,
            icon: ObmSymbol.database,
          ),
          SizedBox(height: context.spacing.lg),
          ObmActionCard(
            title: strings.myKnowledgeTitle,
            body: strings.myKnowledgeBody,
            icon: ObmSymbol.library,
            tone: ready
                ? ObmStatusTone.pausedPrivate
                : ObmStatusTone.offlineUnavailable,
            actionLabel: ready ? strings.wave7MyKuTitle : strings.navLibrary,
            onPressed: ready ? () => context.push('/library/my-ku') : null,
            disabledReason: ready ? null : strings.registryRequiredReason,
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
            tone: ObmStatusTone.ready,
            actionLabel: strings.mediaLibraryTitle,
            onPressed: () => context.push('/library/my-media'),
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.conceptsTitle,
            body: strings.conceptsBody,
            icon: ObmSymbol.search,
            tone: ready ? ObmStatusTone.ready : ObmStatusTone.offlineUnavailable,
            actionLabel: ready ? strings.wave7SearchTitle : strings.navLibrary,
            onPressed: ready ? () => context.push('/library/search') : null,
            disabledReason: ready ? null : strings.registryRequiredReason,
          ),
          if (ready) ...[
            SizedBox(height: context.spacing.md),
            ObmActionCard(
              title: strings.wave7KqlTitle,
              body: strings.wave7KqlHint,
              icon: ObmSymbol.database,
              tone: ObmStatusTone.information,
              actionLabel: strings.wave7KqlTitle,
              onPressed: () => context.push('/library/kql'),
            ),
            SizedBox(height: context.spacing.md),
            ObmActionCard(
              title: strings.wave7ConceptsBrowserTitle,
              body: '${w7.concepts.length} demand-paged concept(s)',
              icon: ObmSymbol.hub,
              tone: ObmStatusTone.ready,
              actionLabel: strings.wave7ConceptsBrowserTitle,
              onPressed: () => context.push('/library/concepts'),
            ),
          ],
        ],
      ),
    );
  }
}
