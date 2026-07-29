import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../l10n/app_localizations.dart';
import '../shared/obm_action_card.dart';
import '../shared/obm_app_shell.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_scope_banner.dart';
import '../shared/obm_shell_destinations.dart';

class CaptureScreen extends StatelessWidget {
  const CaptureScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    return ObmAppShell(
      title: strings.captureTitle,
      selectedIndex: 2,
      destinations: obmShellDestinations(strings),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            strings.captureTitle,
            style: Theme.of(context).textTheme.headlineLarge,
          ),
          SizedBox(height: context.spacing.md),
          Text(
            strings.captureBody,
            style: Theme.of(context).textTheme.bodyLarge,
          ),
          SizedBox(height: context.spacing.lg),
          ObmScopeBanner(
            title: strings.statusPrivate,
            body: strings.textComposerBody,
            tone: ObmStatusTone.pausedPrivate,
            icon: ObmSymbol.lock,
          ),
          SizedBox(height: context.spacing.lg),
          ObmActionCard(
            title: strings.textCaptureTitle,
            body: strings.textCaptureBody,
            icon: ObmSymbol.editNote,
            tone: ObmStatusTone.pausedPrivate,
            statusLabel: strings.statusPrivate,
            actionLabel: strings.captureAction,
            onPressed: () => context.push('/capture/text'),
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.shareCaptureTitle,
            body: strings.shareCaptureBody,
            icon: ObmSymbol.shield,
            tone: ObmStatusTone.pausedPrivate,
            statusLabel: strings.statusPrivate,
            actionLabel: strings.shareSpoolTitle,
            onPressed: () => context.push('/capture/spools'),
          ),
          SizedBox(height: context.spacing.md),
          ObmActionCard(
            title: strings.fileCaptureTitle,
            body: strings.fileCaptureBody,
            icon: ObmSymbol.folder,
            tone: ObmStatusTone.offlineUnavailable,
            actionLabel: strings.navCapture,
            onPressed: null,
            disabledReason: strings.notImplementedBody,
          ),
        ],
      ),
    );
  }
}
