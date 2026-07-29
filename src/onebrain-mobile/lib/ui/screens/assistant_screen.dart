import 'package:flutter/material.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../l10n/app_localizations.dart';
import '../shared/obm_app_shell.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_scope_banner.dart';
import '../shared/obm_shell_destinations.dart';

class AssistantScreen extends StatelessWidget {
  const AssistantScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    return ObmAppShell(
      title: strings.assistantTitle,
      selectedIndex: 3,
      destinations: obmShellDestinations(strings),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            strings.assistantTitle,
            style: Theme.of(context).textTheme.headlineLarge,
          ),
          SizedBox(height: context.spacing.md),
          ObmScopeBanner(
            title: strings.unavailableTitle,
            body: strings.assistantBody,
            tone: ObmStatusTone.offlineUnavailable,
            icon: ObmSymbol.assistant,
          ),
        ],
      ),
    );
  }
}
