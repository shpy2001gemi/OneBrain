import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../l10n/app_localizations.dart';
import '../../platform/mobile_host_gateway.dart';
import '../shared/obm_action_card.dart';
import '../shared/obm_button.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_scope_banner.dart';
import '../shared/obm_screen_frame.dart';

class InitScreen extends ConsumerWidget {
  const InitScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    return ObmScreenFrame(
      title: strings.initTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmScopeBanner(
            title: strings.limitedTitle,
            body: strings.limitedBody,
            tone: ObmStatusTone.waiting,
            icon: ObmSymbol.schedule,
            statusLabel: strings.statusWaiting,
          ),
          SizedBox(height: context.spacing.twoXl),
          Text(
            strings.initTitle,
            style: Theme.of(context).textTheme.headlineLarge,
          ),
          SizedBox(height: context.spacing.md),
          Text(strings.initBody, style: Theme.of(context).textTheme.bodyLarge),
          SizedBox(height: context.spacing.twoXl),
          ObmActionCard(
            title: strings.initBoundaryTitle,
            body: strings.initBoundaryBody,
            icon: ObmSymbol.database,
            tone: ObmStatusTone.information,
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            label: strings.initUnavailableAction,
            onPressed: null,
            disabledReason: strings.initUnavailableReason,
          ),
          SizedBox(height: context.spacing.sm),
          Text(
            strings.initUnavailableReason,
            style: Theme.of(context).textTheme.bodyMedium,
          ),
          SizedBox(height: context.spacing.sm),
          ObmButton(
            label: strings.limitedModeAction,
            variant: ObmButtonVariant.outline,
            onPressed: () async {
              try {
                await ref
                    .read(mobileHostGatewayProvider)
                    .setOnboardingCursor(MobileOnboardingCursor.limitedHome);
                if (context.mounted) {
                  context.go('/home');
                }
              } on Object {
                if (context.mounted) {
                  ScaffoldMessenger.of(context).showSnackBar(
                    SnackBar(
                      content: Text(strings.onboardingProgressSaveError),
                    ),
                  );
                }
              }
            },
          ),
        ],
      ),
    );
  }
}
