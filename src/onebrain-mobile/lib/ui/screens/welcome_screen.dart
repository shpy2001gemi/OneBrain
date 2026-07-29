import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../app/locale_controller.dart';
import '../../design/onebrain_theme_extensions.dart';
import '../../l10n/app_localizations.dart';
import '../../platform/mobile_host_gateway.dart';
import '../shared/obm_button.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_node_fact_card.dart';
import '../shared/obm_screen_frame.dart';

class WelcomeScreen extends ConsumerWidget {
  const WelcomeScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final host = ref.watch(bootstrapHostSnapshotProvider);
    final runtime = ref.watch(mobileRuntimeSnapshotProvider);
    return ObmScreenFrame(
      title: strings.appTitle,
      child: ConstrainedBox(
        constraints: BoxConstraints(
          maxWidth: context.layout.onboardingMaxWidth,
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _WelcomeHero(strings: strings),
            SizedBox(height: context.spacing.twoXl),
            Text(
              strings.languageTitle,
              style: Theme.of(context).textTheme.titleMedium,
            ),
            SizedBox(height: context.spacing.md),
            Wrap(
              spacing: context.spacing.sm,
              runSpacing: context.spacing.sm,
              children: [
                ObmButton(
                  label: strings.languageEnglish,
                  variant: ObmButtonVariant.outline,
                  onPressed: () => ref
                      .read(localeControllerProvider.notifier)
                      .select(const Locale('en')),
                ),
                ObmButton(
                  label: strings.languageVietnamese,
                  variant: ObmButtonVariant.outline,
                  onPressed: () => ref
                      .read(localeControllerProvider.notifier)
                      .select(const Locale('vi')),
                ),
              ],
            ),
            SizedBox(height: context.spacing.twoXl),
            ObmNodeFactCard(
              title: strings.nodeFactTitle,
              body: strings.nodeFactBody,
              icon: ObmSymbol.hub,
              tone: ObmStatusTone.pausedPrivate,
              statusLabel: strings.statusPrivate,
            ),
            SizedBox(height: context.spacing.md),
            ObmNodeFactCard(
              title: strings.registryFactTitle,
              body: strings.registryFactBody,
              icon: ObmSymbol.cloudDownload,
              tone: ObmStatusTone.waiting,
              statusLabel: strings.statusWaiting,
            ),
            SizedBox(height: context.spacing.md),
            ObmNodeFactCard(
              title: strings.requestFactTitle,
              body: strings.requestFactBody,
              icon: ObmSymbol.wifiOff,
              tone: ObmStatusTone.ready,
              statusLabel: strings.statusReady,
            ),
            SizedBox(height: context.spacing.md),
            host.when(
              loading: () => ObmNodeFactCard(
                title: strings.nativeHostTitle,
                body: strings.nativeHostLoading,
                icon: ObmSymbol.phoneAndroid,
                tone: ObmStatusTone.waiting,
              ),
              error: (error, stackTrace) => ObmNodeFactCard(
                title: strings.nativeHostTitle,
                body: strings.nativeHostUnavailable,
                icon: ObmSymbol.phonelinkErase,
                tone: ObmStatusTone.offlineUnavailable,
              ),
              data: (snapshot) => ObmNodeFactCard(
                title: strings.nativeHostTitle,
                body: strings.nativeHostReady(
                  snapshot.platform,
                  snapshot.apiVersion,
                ),
                icon: ObmSymbol.phoneAndroid,
                tone: snapshot.registryRequestIssued
                    ? ObmStatusTone.failed
                    : ObmStatusTone.ready,
              ),
            ),
            SizedBox(height: context.spacing.md),
            host.when(
              loading: () => ObmNodeFactCard(
                title: strings.rustBridgeTitle,
                body: strings.rustBridgeLoading,
                icon: ObmSymbol.memory,
                tone: ObmStatusTone.waiting,
              ),
              error: (error, stackTrace) => ObmNodeFactCard(
                title: strings.rustBridgeTitle,
                body: strings.rustBridgeUnavailable,
                icon: ObmSymbol.memory,
                tone: ObmStatusTone.offlineUnavailable,
              ),
              data: (snapshot) {
                final verified =
                    snapshot.rustCoreLinked &&
                    snapshot.rustRoundTripVerified &&
                    !snapshot.registryRequestIssued;
                return ObmNodeFactCard(
                  title: strings.rustBridgeTitle,
                  body: snapshot.rustCoreLinked
                      ? strings.rustBridgeReady(
                          snapshot.rustCoreVersion,
                          snapshot.rustAbiVersion,
                        )
                      : strings.rustBridgeUnavailable,
                  icon: ObmSymbol.memory,
                  tone: verified
                      ? ObmStatusTone.ready
                      : ObmStatusTone.offlineUnavailable,
                  statusLabel: verified
                      ? strings.rustBridgeVerified
                      : strings.rustBridgeNotVerified,
                );
              },
            ),
            SizedBox(height: context.spacing.md),
            runtime.when(
              loading: () => ObmNodeFactCard(
                title: strings.mobileRuntimeTitle,
                body: strings.mobileRuntimeLoading,
                icon: ObmSymbol.memory,
                tone: ObmStatusTone.waiting,
              ),
              error: (error, stackTrace) => ObmNodeFactCard(
                title: strings.mobileRuntimeTitle,
                body: strings.mobileRuntimeUnavailable,
                icon: ObmSymbol.memory,
                tone: ObmStatusTone.offlineUnavailable,
              ),
              data: (snapshot) {
                final verified =
                    snapshot.bootstrapStoreOpened &&
                    snapshot.registryState == 'BootstrapOnly' &&
                    snapshot.localKqlFixtureVerified &&
                    snapshot.privatePlannerVerified &&
                    snapshot.noLlmProvider &&
                    snapshot.staleCallbackRejected &&
                    snapshot.secureProfileActive &&
                    snapshot.installationBindingVerified &&
                    snapshot.securitySessionUnlocked &&
                    snapshot.privateVaultReady &&
                    snapshot.identityDomainsSeparated &&
                    snapshot.privacyDefaultsFailSafe &&
                    snapshot.redactedHistoryReady;
                return ObmNodeFactCard(
                  title: strings.mobileRuntimeTitle,
                  body: snapshot.recoveredUncleanStart
                      ? strings.mobileRuntimeRecovered(
                          snapshot.processGeneration,
                        )
                      : strings.mobileRuntimeReady(
                          snapshot.profileVersion,
                          snapshot.processGeneration,
                          snapshot.activationPhase,
                          snapshot.activeGrantCount,
                          snapshot.registryState,
                        ),
                  icon: ObmSymbol.memory,
                  tone: verified
                      ? ObmStatusTone.ready
                      : ObmStatusTone.offlineUnavailable,
                  statusLabel: verified
                      ? strings.mobileRuntimeVerified
                      : strings.mobileRuntimeNotVerified,
                );
              },
            ),
            SizedBox(height: context.spacing.twoXl),
            ObmButton(
              label: strings.continueAction,
              onPressed: () async {
                try {
                  await ref
                      .read(mobileHostGatewayProvider)
                      .setOnboardingCursor(MobileOnboardingCursor.preflight);
                  if (context.mounted) {
                    context.go('/onboarding/preflight');
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
            SizedBox(height: context.spacing.sm),
            ObmButton(
              label: strings.galleryAction,
              variant: ObmButtonVariant.text,
              onPressed: () => context.push('/debug/components'),
            ),
          ],
        ),
      ),
    );
  }
}

class _WelcomeHero extends StatelessWidget {
  const _WelcomeHero({required this.strings});

  final AppLocalizations strings;

  @override
  Widget build(BuildContext context) => Card(
    clipBehavior: Clip.antiAlias,
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SizedBox(
          height: context.spacing.sm,
          child: DecoratedBox(
            decoration: BoxDecoration(gradient: context.gradients.ideaPath),
          ),
        ),
        Padding(
          padding: EdgeInsets.all(context.spacing.twoXl),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                strings.bootstrapEyebrow,
                style: Theme.of(context).textTheme.labelMedium?.copyWith(
                  color: Theme.of(context).colorScheme.primary,
                ),
              ),
              SizedBox(height: context.spacing.sm),
              Text(
                strings.welcomeTitle,
                style: Theme.of(context).textTheme.headlineLarge,
              ),
              SizedBox(height: context.spacing.md),
              Text(
                strings.welcomeBody,
                style: Theme.of(context).textTheme.bodyLarge,
              ),
            ],
          ),
        ),
      ],
    ),
  );
}
