import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../l10n/app_localizations.dart';
import '../../platform/mobile_host_gateway.dart';
import '../shared/obm_action_card.dart';
import '../shared/obm_app_shell.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_scope_banner.dart';
import '../shared/obm_shell_destinations.dart';

class HomeScreen extends ConsumerWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final runtime = ref.watch(mobileRuntimeSnapshotProvider);
    final draftCount = runtime.asData?.value.encryptedRawDraftCount ?? 0;
    return ObmAppShell(
      title: strings.homeTitle,
      selectedIndex: 0,
      destinations: obmShellDestinations(strings),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            strings.homeGreeting,
            style: Theme.of(context).textTheme.headlineLarge,
          ),
          SizedBox(height: context.spacing.lg),
          ObmScopeBanner(
            title: strings.limitedTitle,
            body: strings.limitedBody,
            tone: ObmStatusTone.waiting,
            icon: ObmSymbol.schedule,
            statusLabel: strings.statusWaiting,
          ),
          SizedBox(height: context.spacing.lg),
          ObmActionCard(
            title: strings.quickCaptureTitle,
            body: strings.quickCaptureBody,
            icon: ObmSymbol.editNote,
            tone: ObmStatusTone.pausedPrivate,
            statusLabel: strings.statusPrivate,
            actionLabel: strings.captureAction,
            onPressed: () => context.go('/capture/text'),
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.requiredInitTitle,
            body: strings.requiredInitBody,
            icon: ObmSymbol.cloudDownload,
            tone: ObmStatusTone.waiting,
            statusLabel: strings.statusWaiting,
            actionLabel: strings.openInitAction,
            onPressed: () => context.push('/init'),
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.draftCountTitle,
            body: strings.draftCountBody(draftCount),
            icon: ObmSymbol.description,
            tone: ObmStatusTone.pausedPrivate,
            statusLabel: strings.statusPrivate,
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.operationsTitle,
            body: strings.operationsBody,
            icon: ObmSymbol.operations,
            tone: ObmStatusTone.information,
          ),
        ],
      ),
    );
  }
}
