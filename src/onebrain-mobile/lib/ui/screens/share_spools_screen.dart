import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../l10n/app_localizations.dart';
import '../../platform/mobile_host_gateway.dart';
import '../shared/obm_action_card.dart';
import '../shared/obm_app_shell.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_scope_banner.dart';
import '../shared/obm_shell_destinations.dart';

class ShareSpoolsScreen extends ConsumerStatefulWidget {
  const ShareSpoolsScreen({super.key});

  @override
  ConsumerState<ShareSpoolsScreen> createState() => _ShareSpoolsScreenState();
}

class _ShareSpoolsScreenState extends ConsumerState<ShareSpoolsScreen> {
  String? _importingRef;

  Future<void> _import(MobileShareSpoolSummary spool) async {
    setState(() => _importingRef = spool.spoolRef);
    final strings = AppLocalizations.of(context);
    try {
      final receipt = await ref
          .read(mobileHostGatewayProvider)
          .importSharedText(
            spoolRef: spool.spoolRef,
            contentLanguage: Localizations.localeOf(context).languageCode,
          );
      ref.invalidate(pendingShareSpoolsProvider);
      ref.invalidate(mobileRuntimeSnapshotProvider);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(strings.shareSpoolImported(receipt.draftRef))),
        );
      }
    } on Object {
      if (mounted) {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text(strings.shareSpoolImportError)));
      }
    } finally {
      if (mounted) {
        setState(() => _importingRef = null);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final spools = ref.watch(pendingShareSpoolsProvider);
    return ObmAppShell(
      title: strings.shareSpoolTitle,
      selectedIndex: 2,
      destinations: obmShellDestinations(strings),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            strings.shareSpoolTitle,
            style: Theme.of(context).textTheme.headlineLarge,
          ),
          SizedBox(height: context.spacing.md),
          Text(
            strings.shareSpoolBody,
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
          switch (spools) {
            AsyncData(value: final items) when items.isEmpty => ObmActionCard(
              title: strings.shareSpoolEmptyTitle,
              body: strings.shareSpoolEmptyBody,
              icon: ObmSymbol.shield,
            ),
            AsyncData(value: final items) => Column(
              children: [
                for (final spool in items) ...[
                  ObmActionCard(
                    title: strings.shareSpoolItemTitle,
                    body: strings.shareSpoolItemBody(
                      spool.mimeType,
                      spool.contentBytes,
                    ),
                    icon: ObmSymbol.shield,
                    tone: ObmStatusTone.pausedPrivate,
                    statusLabel: strings.statusPrivate,
                    actionLabel: strings.shareSpoolImportAction,
                    onPressed: _importingRef == null
                        ? () => _import(spool)
                        : null,
                    disabledReason: _importingRef == null
                        ? null
                        : strings.entryResolving,
                  ),
                  SizedBox(height: context.spacing.md),
                ],
              ],
            ),
            AsyncError() => ObmActionCard(
              title: strings.unavailableTitle,
              body: strings.shareSpoolLoadError,
              icon: ObmSymbol.shield,
              tone: ObmStatusTone.failed,
              actionLabel: strings.backAction,
              onPressed: () => ref.invalidate(pendingShareSpoolsProvider),
            ),
            _ => const Center(child: CircularProgressIndicator()),
          },
        ],
      ),
    );
  }
}
