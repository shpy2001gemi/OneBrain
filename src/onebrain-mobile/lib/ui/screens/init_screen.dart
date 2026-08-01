import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../l10n/app_localizations.dart';
import '../../platform/mobile_host_gateway.dart';
import '../shared/obm_action_card.dart';
import '../shared/obm_button.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_registry_plan.dart';
import '../shared/obm_scope_banner.dart';
import '../shared/obm_screen_frame.dart';

class InitScreen extends ConsumerStatefulWidget {
  const InitScreen({super.key});

  @override
  ConsumerState<InitScreen> createState() => _InitScreenState();
}

class _InitScreenState extends ConsumerState<InitScreen> {
  MobileRegistryInitPlan? _plan;
  MobileRegistryNetworkPolicy _networkPolicy =
      MobileRegistryNetworkPolicy.unmetered;
  bool _oneTimeNetworkOverride = false;
  bool _busy = false;
  String? _error;

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final availability = ref.watch(registryInitAvailabilityProvider);
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
          if (_plan case final plan?)
            _buildExactReview(context, strings, plan)
          else
            _buildEntry(context, strings, availability),
          if (_error case final error?) ...[
            SizedBox(height: context.spacing.lg),
            ObmActionCard(
              title: strings.initErrorTitle,
              body: error,
              icon: ObmSymbol.cancel,
              tone: ObmStatusTone.failed,
            ),
          ],
        ],
      ),
    );
  }

  Widget _buildEntry(
    BuildContext context,
    AppLocalizations strings,
    AsyncValue<MobileRegistryInitAvailability> availability,
  ) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      ObmActionCard(
        title: strings.initBoundaryTitle,
        body: strings.initBoundaryBody,
        icon: ObmSymbol.database,
        tone: ObmStatusTone.information,
      ),
      SizedBox(height: context.spacing.lg),
      availability.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (_, _) => _buildUnavailable(strings),
        data: (facts) => Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (facts.trustMode ==
                MobileRegistryTrustMode.developmentFixture) ...[
              ObmActionCard(
                title: strings.initDevelopmentFixtureTitle,
                body: strings.initDevelopmentFixtureBody,
                icon: ObmSymbol.buildCircle,
                tone: ObmStatusTone.degraded,
                statusLabel: facts.reasonCode,
              ),
              SizedBox(height: context.spacing.lg),
            ],
            ObmButton(
              key: const Key('init_begin_action'),
              label: strings.initUnavailableAction,
              onPressed: facts.available && !_busy
                  ? () => _begin(facts.channelId)
                  : null,
              busy: _busy,
              disabledReason: facts.available
                  ? null
                  : strings.initUnavailableReason,
            ),
            if (!facts.available) ...[
              SizedBox(height: context.spacing.sm),
              Text(
                strings.initUnavailableReason,
                style: Theme.of(context).textTheme.bodyMedium,
              ),
            ],
          ],
        ),
      ),
      SizedBox(height: context.spacing.sm),
      ObmButton(
        label: strings.limitedModeAction,
        variant: ObmButtonVariant.outline,
        onPressed: _busy ? null : _enterLimitedMode,
      ),
    ],
  );

  Widget _buildUnavailable(AppLocalizations strings) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      ObmButton(
        label: strings.initUnavailableAction,
        onPressed: null,
        disabledReason: strings.initUnavailableReason,
      ),
      SizedBox(height: context.spacing.sm),
      Text(strings.initUnavailableReason),
    ],
  );

  Widget _buildExactReview(
    BuildContext context,
    AppLocalizations strings,
    MobileRegistryInitPlan plan,
  ) {
    final hasCapacity = plan.measuredFreeBytes >= plan.initialRequiredFreeBytes;
    final confirmationRecorded = plan.stateCode == 8 || plan.stateCode == 21;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ObmActionCard(
          title: strings.initDevelopmentFixtureTitle,
          body: strings.initDevelopmentFixtureBody,
          icon: ObmSymbol.buildCircle,
          tone: ObmStatusTone.degraded,
        ),
        SizedBox(height: context.spacing.lg),
        ObmExactPlanPanel(
          key: const Key('init_exact_plan'),
          title: strings.initPlanTitle,
          subtitle: strings.initPlanSubtitle,
          statusLabel: hasCapacity
              ? strings.initCapacityReady
              : strings.initCapacityInsufficient,
          tone: hasCapacity ? ObmStatusTone.ready : ObmStatusTone.degraded,
          facts: [
            ObmPlanFact(label: strings.initChannelLabel, value: plan.channelId),
            ObmPlanFact(
              label: strings.initReleaseLabel,
              value: _shortDigest(plan.releaseId),
            ),
            ObmPlanFact(
              label: strings.initManifestLabel,
              value: _shortDigest(plan.manifestDigest),
            ),
            ObmPlanFact(
              label: strings.initHeadGenerationLabel,
              value: '${plan.headGeneration}',
            ),
            ObmPlanFact(
              label: strings.initReleaseSequenceLabel,
              value: '${plan.releaseSequence}',
            ),
            ObmPlanFact(
              label: strings.initPublisherFloorLabel,
              value: _formatBytes(plan.publisherMinAdditionalFreeBytes),
            ),
            ObmPlanFact(
              label: strings.initArtifactBytesLabel,
              value: _formatBytes(plan.artifactTotalBytes),
            ),
            ObmPlanFact(
              label: strings.initTargetAllocationLabel,
              value: _formatBytes(plan.targetTotalAllocBytes),
            ),
            ObmPlanFact(
              label: strings.initTransferPeakLabel,
              value: _formatBytes(plan.transferInitialBytes),
            ),
            ObmPlanFact(
              label: strings.initVerificationWorkspaceLabel,
              value: _formatBytes(plan.verificationWorkspaceBytes),
            ),
            ObmPlanFact(
              label: strings.initCatalogGrowthLabel,
              value: _formatBytes(plan.catalogGrowthBytes),
            ),
            ObmPlanFact(
              label: strings.initSafetyReserveLabel,
              value: _formatBytes(plan.safetyReserveBytes),
            ),
          ],
        ),
        SizedBox(height: context.spacing.lg),
        ObmResourceFacts(
          title: strings.initResourceFactsTitle,
          availableLabel: strings.initAvailableBytesLabel,
          availableValue: _formatBytes(plan.measuredFreeBytes),
          requiredLabel: strings.initRequiredBytesLabel,
          requiredValue: _formatBytes(plan.initialRequiredFreeBytes),
          reserveLabel: strings.initSafetyReserveLabel,
          reserveValue: _formatBytes(plan.safetyReserveBytes),
          capacityLabel: strings.initVolumeCapacityLabel,
          capacityValue: _formatBytes(plan.destinationTotalUsableBytes),
          hasCapacity: hasCapacity,
        ),
        SizedBox(height: context.spacing.lg),
        if (confirmationRecorded)
          ObmActionCard(
            title: plan.stateCode == 8
                ? strings.initAdmittedTitle
                : strings.initWaitingStorageTitle,
            body: plan.stateCode == 8
                ? strings.initAdmittedBody
                : strings.initWaitingStorageBody,
            icon: plan.stateCode == 8
                ? ObmSymbol.checkCircle
                : ObmSymbol.storage,
            tone: plan.stateCode == 8
                ? ObmStatusTone.ready
                : ObmStatusTone.waiting,
            statusLabel: strings.initTransportGated,
          )
        else ...[
          Text(
            strings.initNetworkPolicyTitle,
            style: Theme.of(context).textTheme.titleMedium,
          ),
          SizedBox(height: context.spacing.sm),
          Wrap(
            spacing: context.spacing.sm,
            runSpacing: context.spacing.sm,
            children:
                [
                      (
                        MobileRegistryNetworkPolicy.wifiOnly,
                        strings.initWifiOnlyPolicy,
                      ),
                      (
                        MobileRegistryNetworkPolicy.unmetered,
                        strings.initUnmeteredPolicy,
                      ),
                      (
                        MobileRegistryNetworkPolicy.anyNetwork,
                        strings.initAnyNetworkPolicy,
                      ),
                    ]
                    .map(
                      (option) => ChoiceChip(
                        label: Text(option.$2),
                        selected: _networkPolicy == option.$1,
                        onSelected: _busy
                            ? null
                            : (_) {
                                setState(() {
                                  _networkPolicy = option.$1;
                                  if (_networkPolicy !=
                                      MobileRegistryNetworkPolicy.anyNetwork) {
                                    _oneTimeNetworkOverride = false;
                                  }
                                });
                              },
                      ),
                    )
                    .toList(growable: false),
          ),
          if (_networkPolicy == MobileRegistryNetworkPolicy.anyNetwork)
            CheckboxListTile(
              contentPadding: EdgeInsets.zero,
              value: _oneTimeNetworkOverride,
              title: Text(strings.initOneTimeOverrideLabel),
              onChanged: _busy
                  ? null
                  : (value) => setState(
                      () => _oneTimeNetworkOverride = value ?? false,
                    ),
            ),
          SizedBox(height: context.spacing.lg),
          ObmSplitReviewActions(
            deferLabel: strings.initDeferAction,
            confirmLabel: strings.initConfirmAction,
            onDefer: _defer,
            onConfirm: _confirm,
            busy: _busy,
          ),
        ],
      ],
    );
  }

  Future<void> _begin(String channelId) async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final plan = await ref
          .read(mobileHostGatewayProvider)
          .beginRegistryInit(channelId);
      if (mounted) {
        setState(() => _plan = plan);
      }
    } on Object {
      if (mounted) {
        setState(() => _error = AppLocalizations.of(context).initPlanError);
      }
    } finally {
      if (mounted) {
        setState(() => _busy = false);
      }
    }
  }

  Future<void> _defer() async {
    final plan = _plan;
    if (plan == null) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final gateway = ref.read(mobileHostGatewayProvider);
      await gateway.deferRegistryInit(
        operationId: plan.operationId,
        manifestDigest: plan.manifestDigest,
      );
      await gateway.setOnboardingCursor(MobileOnboardingCursor.limitedHome);
      if (mounted) context.go('/home');
    } on Object {
      if (mounted) {
        setState(() => _error = AppLocalizations.of(context).initDeferError);
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _confirm() async {
    final plan = _plan;
    if (plan == null) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final confirmed = await ref
          .read(mobileHostGatewayProvider)
          .confirmRegistryInit(
            operationId: plan.operationId,
            manifestDigest: plan.manifestDigest,
            networkPolicy: _networkPolicy,
            oneTimeNetworkOverride: _oneTimeNetworkOverride,
          );
      if (mounted) setState(() => _plan = confirmed);
    } on Object {
      if (mounted) {
        setState(() => _error = AppLocalizations.of(context).initConfirmError);
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _enterLimitedMode() async {
    try {
      await ref
          .read(mobileHostGatewayProvider)
          .setOnboardingCursor(MobileOnboardingCursor.limitedHome);
      if (mounted) context.go('/home');
    } on Object {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              AppLocalizations.of(context).onboardingProgressSaveError,
            ),
          ),
        );
      }
    }
  }

  String _shortDigest(String digest) => digest.length <= 16
      ? digest
      : '${digest.substring(0, 8)}…${digest.substring(digest.length - 8)}';

  String _formatBytes(int bytes) {
    const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
    var value = bytes.toDouble();
    var unit = 0;
    while (value >= 1024 && unit < units.length - 1) {
      value /= 1024;
      unit += 1;
    }
    final digits = unit == 0 || value >= 100 ? 0 : (value >= 10 ? 1 : 2);
    return '${value.toStringAsFixed(digits)} ${units[unit]}';
  }
}
