import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../l10n/app_localizations.dart';
import '../../platform/mobile_host_gateway.dart';
import '../shared/obm_action_card.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_node_fact_card.dart';
import '../shared/obm_scope_banner.dart';
import '../shared/obm_screen_frame.dart';

class MediaImportScreen extends ConsumerStatefulWidget {
  const MediaImportScreen({super.key});

  @override
  ConsumerState<MediaImportScreen> createState() => _MediaImportScreenState();
}

class _MediaImportScreenState extends ConsumerState<MediaImportScreen> {
  MobileMediaClass? _busyClass;
  MobileOwnedMediaSummary? _receipt;
  bool _failed = false;

  Future<void> _pick(MobileMediaClass mediaClass) async {
    if (_busyClass != null) {
      return;
    }
    setState(() {
      _busyClass = mediaClass;
      _failed = false;
    });
    try {
      final receipt = await ref
          .read(mobileHostGatewayProvider)
          .pickAndImportOwnedMedia(mediaClass);
      if (!mounted) {
        return;
      }
      setState(() => _receipt = receipt);
      ref.invalidate(mobileRuntimeSnapshotProvider);
      ref.invalidate(ownedMediaProvider);
    } on Object {
      if (!mounted) {
        return;
      }
      setState(() => _failed = true);
    } finally {
      if (mounted) {
        setState(() => _busyClass = null);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final isBusy = _busyClass != null;
    return ObmScreenFrame(
      title: strings.mediaImportTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            strings.mediaImportTitle,
            style: Theme.of(context).textTheme.headlineLarge,
          ),
          SizedBox(height: context.spacing.md),
          Text(
            strings.mediaImportBody,
            style: Theme.of(context).textTheme.bodyLarge,
          ),
          SizedBox(height: context.spacing.lg),
          ObmScopeBanner(
            title: strings.mediaImportBoundaryTitle,
            body: strings.mediaImportBoundaryBody,
            tone: ObmStatusTone.pausedPrivate,
            icon: ObmSymbol.lock,
          ),
          if (isBusy) ...[
            SizedBox(height: context.spacing.md),
            Semantics(
              liveRegion: true,
              child: ObmNodeFactCard(
                title: strings.mediaPickBusy,
                body: strings.mediaImportBody,
                icon: ObmSymbol.schedule,
                tone: ObmStatusTone.information,
              ),
            ),
          ],
          if (_receipt case final receipt?) ...[
            SizedBox(height: context.spacing.md),
            Semantics(
              liveRegion: true,
              child: ObmNodeFactCard(
                title: strings.mediaStageReadyTitle,
                body: strings.mediaStageReadyBody(
                  receipt.mimeType,
                  receipt.contentBytes,
                  receipt.storageClass,
                  receipt.mediaRef,
                ),
                icon: ObmSymbol.checkCircle,
                tone: ObmStatusTone.ready,
                statusLabel: strings.statusPrivate,
              ),
            ),
          ],
          if (_failed) ...[
            SizedBox(height: context.spacing.md),
            Semantics(
              liveRegion: true,
              child: ObmNodeFactCard(
                title: strings.unavailableTitle,
                body: strings.mediaStageError,
                icon: ObmSymbol.info,
                tone: ObmStatusTone.failed,
              ),
            ),
          ],
          SizedBox(height: context.spacing.lg),
          _pickerCard(
            title: strings.mediaPickImageTitle,
            body: strings.mediaPickImageBody,
            icon: ObmSymbol.capture,
            mediaClass: MobileMediaClass.image,
            actionLabel: strings.mediaPickAction,
          ),
          SizedBox(height: context.spacing.md),
          _pickerCard(
            title: strings.mediaPickVideoTitle,
            body: strings.mediaPickVideoBody,
            icon: ObmSymbol.folder,
            mediaClass: MobileMediaClass.video,
            actionLabel: strings.mediaPickAction,
          ),
          SizedBox(height: context.spacing.md),
          _pickerCard(
            title: strings.mediaPickAudioTitle,
            body: strings.mediaPickAudioBody,
            icon: ObmSymbol.assistant,
            mediaClass: MobileMediaClass.audio,
            actionLabel: strings.mediaPickAction,
          ),
          SizedBox(height: context.spacing.md),
          _pickerCard(
            title: strings.mediaPickDocumentTitle,
            body: strings.mediaPickDocumentBody,
            icon: ObmSymbol.description,
            mediaClass: MobileMediaClass.document,
            actionLabel: strings.mediaPickAction,
          ),
        ],
      ),
    );
  }

  Widget _pickerCard({
    required String title,
    required String body,
    required ObmSymbol icon,
    required MobileMediaClass mediaClass,
    required String actionLabel,
  }) => ObmActionCard(
    title: title,
    body: body,
    icon: icon,
    tone: ObmStatusTone.pausedPrivate,
    statusLabel: _busyClass == mediaClass ? actionLabel : null,
    actionLabel: actionLabel,
    onPressed: _busyClass == null ? () => _pick(mediaClass) : null,
  );
}
