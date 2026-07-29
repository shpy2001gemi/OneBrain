import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../l10n/app_localizations.dart';
import '../../platform/mobile_host_gateway.dart';

class EntryScreen extends ConsumerStatefulWidget {
  const EntryScreen({super.key});

  @override
  ConsumerState<EntryScreen> createState() => _EntryScreenState();
}

class _EntryScreenState extends ConsumerState<EntryScreen> {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      var location = MobileOnboardingCursor.welcome.location;
      try {
        await ref.read(bootstrapHostSnapshotProvider.future);
        final runtime = await ref.read(mobileRuntimeSnapshotProvider.future);
        location = runtime.onboardingCursor.location;
      } on Object {
        // The typed host state remains visible as unavailable on the next
        // screen. Entry resolution does not invent product readiness.
      }
      if (mounted) {
        context.go(location);
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    return Scaffold(
      body: SafeArea(
        child: Center(
          child: Semantics(
            liveRegion: true,
            label: strings.entryResolving,
            child: Padding(
              padding: EdgeInsets.all(context.spacing.twoXl),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  const CircularProgressIndicator(),
                  SizedBox(height: context.spacing.lg),
                  Text(
                    strings.entryResolving,
                    style: Theme.of(context).textTheme.bodyLarge,
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
