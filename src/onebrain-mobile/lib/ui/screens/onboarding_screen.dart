import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../l10n/app_localizations.dart';
import '../../platform/mobile_host_gateway.dart';
import '../shared/obm_action_card.dart';
import '../shared/obm_button.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_onboarding_frame.dart';

enum OnboardingStep { preflight, identity, security, initHandoff }

class OnboardingScreen extends ConsumerWidget {
  const OnboardingScreen({required this.step, super.key});

  final OnboardingStep step;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final content = _content(strings);
    return ObmOnboardingFrame(
      appTitle: strings.appTitle,
      stepLabel: strings.onboardingStep(step.index + 2, 5),
      title: content.title,
      body: content.body,
      primaryLabel: content.primaryLabel,
      onPrimary: () => _saveAndGo(
        context,
        ref,
        content.nextCursor,
        content.nextLocation,
        strings,
      ),
      onBack: () => _saveAndGo(
        context,
        ref,
        content.backCursor,
        content.backLocation,
        strings,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final card in content.cards) ...[
            ObmActionCard(
              title: card.title,
              body: card.body,
              icon: card.icon,
              tone: card.tone,
              statusLabel: card.statusLabel,
            ),
            SizedBox(height: context.spacing.md),
          ],
          if (step == OnboardingStep.initHandoff)
            ObmButton(
              label: strings.limitedModeAction,
              variant: ObmButtonVariant.outline,
              onPressed: () => _saveAndGo(
                context,
                ref,
                MobileOnboardingCursor.limitedHome,
                '/home',
                strings,
              ),
            ),
        ],
      ),
    );
  }

  _OnboardingContent _content(AppLocalizations strings) => switch (step) {
    OnboardingStep.preflight => _OnboardingContent(
      title: strings.preflightTitle,
      body: strings.preflightBody,
      primaryLabel: strings.nextAction,
      nextLocation: '/onboarding/identity',
      nextCursor: MobileOnboardingCursor.identity,
      backLocation: '/onboarding/welcome',
      backCursor: MobileOnboardingCursor.welcome,
      cards: [
        _OnboardingCard(
          title: strings.preflightRuntimeTitle,
          body: strings.preflightRuntimeBody,
          icon: ObmSymbol.shield,
          tone: ObmStatusTone.ready,
          statusLabel: strings.statusReady,
        ),
        _OnboardingCard(
          title: strings.preflightStorageTitle,
          body: strings.preflightStorageBody,
          icon: ObmSymbol.cloudDownload,
          tone: ObmStatusTone.waiting,
          statusLabel: strings.statusWaiting,
        ),
        _OnboardingCard(
          title: strings.preflightOptionalTitle,
          body: strings.preflightOptionalBody,
          icon: ObmSymbol.wifiOff,
          tone: ObmStatusTone.offlineUnavailable,
        ),
      ],
    ),
    OnboardingStep.identity => _OnboardingContent(
      title: strings.identityTitle,
      body: strings.identityBody,
      primaryLabel: strings.nextAction,
      nextLocation: '/onboarding/security',
      nextCursor: MobileOnboardingCursor.security,
      backLocation: '/onboarding/preflight',
      backCursor: MobileOnboardingCursor.preflight,
      cards: [
        _OnboardingCard(
          title: strings.identityReadyTitle,
          body: strings.identityReadyBody,
          icon: ObmSymbol.hub,
          tone: ObmStatusTone.ready,
          statusLabel: strings.statusPrivate,
        ),
      ],
    ),
    OnboardingStep.security => _OnboardingContent(
      title: strings.securityTitle,
      body: strings.securityBody,
      primaryLabel: strings.nextAction,
      nextLocation: '/onboarding/init-handoff',
      nextCursor: MobileOnboardingCursor.initHandoff,
      backLocation: '/onboarding/identity',
      backCursor: MobileOnboardingCursor.identity,
      cards: [
        _OnboardingCard(
          title: strings.securityVaultTitle,
          body: strings.securityVaultBody,
          icon: ObmSymbol.lock,
          tone: ObmStatusTone.pausedPrivate,
          statusLabel: strings.statusPrivate,
        ),
      ],
    ),
    OnboardingStep.initHandoff => _OnboardingContent(
      title: strings.initHandoffTitle,
      body: strings.initHandoffBody,
      primaryLabel: strings.openInitAction,
      nextLocation: '/init',
      nextCursor: MobileOnboardingCursor.initHandoff,
      backLocation: '/onboarding/security',
      backCursor: MobileOnboardingCursor.security,
      cards: [
        _OnboardingCard(
          title: strings.initHandoffLimitedTitle,
          body: strings.initHandoffLimitedBody,
          icon: ObmSymbol.editNote,
          tone: ObmStatusTone.information,
          statusLabel: strings.statusPrivate,
        ),
      ],
    ),
  };

  Future<void> _saveAndGo(
    BuildContext context,
    WidgetRef ref,
    MobileOnboardingCursor cursor,
    String location,
    AppLocalizations strings,
  ) async {
    try {
      await ref.read(mobileHostGatewayProvider).setOnboardingCursor(cursor);
      if (context.mounted) {
        context.go(location);
      }
    } on Object {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(strings.onboardingProgressSaveError)),
        );
      }
    }
  }
}

class _OnboardingContent {
  const _OnboardingContent({
    required this.title,
    required this.body,
    required this.primaryLabel,
    required this.nextLocation,
    required this.nextCursor,
    required this.backLocation,
    required this.backCursor,
    required this.cards,
  });

  final String title;
  final String body;
  final String primaryLabel;
  final String nextLocation;
  final MobileOnboardingCursor nextCursor;
  final String backLocation;
  final MobileOnboardingCursor backCursor;
  final List<_OnboardingCard> cards;
}

class _OnboardingCard {
  const _OnboardingCard({
    required this.title,
    required this.body,
    required this.icon,
    required this.tone,
    this.statusLabel,
  });

  final String title;
  final String body;
  final ObmSymbol icon;
  final ObmStatusTone tone;
  final String? statusLabel;
}
