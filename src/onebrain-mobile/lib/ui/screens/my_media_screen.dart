import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../l10n/app_localizations.dart';
import '../../platform/mobile_host_gateway.dart';
import '../shared/obm_action_card.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_media_card.dart';
import '../shared/obm_node_fact_card.dart';
import '../shared/obm_scope_banner.dart';
import '../shared/obm_screen_frame.dart';

class MyMediaScreen extends ConsumerWidget {
  const MyMediaScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final media = ref.watch(ownedMediaProvider);
    return ObmScreenFrame(
      title: strings.mediaLibraryTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            strings.mediaLibraryTitle,
            style: Theme.of(context).textTheme.headlineLarge,
          ),
          SizedBox(height: context.spacing.md),
          Text(
            strings.myMediaShelfBody,
            style: Theme.of(context).textTheme.bodyLarge,
          ),
          SizedBox(height: context.spacing.lg),
          ObmScopeBanner(
            title: strings.myMediaPrivateTitle,
            body: strings.myMediaPrivateBody,
            tone: ObmStatusTone.pausedPrivate,
            icon: ObmSymbol.lock,
          ),
          SizedBox(height: context.spacing.lg),
          media.when(
            loading: () => ObmNodeFactCard(
              title: strings.myMediaLoadingTitle,
              body: strings.myMediaLoadingBody,
              icon: ObmSymbol.schedule,
              tone: ObmStatusTone.information,
            ),
            error: (error, stackTrace) => ObmActionCard(
              title: strings.unavailableTitle,
              body: strings.myMediaLoadError,
              icon: ObmSymbol.info,
              tone: ObmStatusTone.failed,
              actionLabel: strings.retryAction,
              onPressed: () => ref.invalidate(ownedMediaProvider),
            ),
            data: (entries) => entries.isEmpty
                ? ObmNodeFactCard(
                    title: strings.myMediaEmptyTitle,
                    body: strings.myMediaEmptyBody,
                    icon: ObmSymbol.folder,
                    tone: ObmStatusTone.information,
                  )
                : Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      for (var index = 0; index < entries.length; index++) ...[
                        if (index > 0) SizedBox(height: context.spacing.md),
                        _mediaCard(strings, entries[index]),
                      ],
                    ],
                  ),
          ),
        ],
      ),
    );
  }

  Widget _mediaCard(AppLocalizations strings, MobileOwnedMediaSummary media) =>
      ObmMediaCard(
        title: strings.myMediaItemTitle(_classLabel(strings, media.mediaClass)),
        mimeType: media.mimeType,
        mediaRef: media.mediaRef,
        verifiedBytesLabel: strings.myMediaVerifiedBytes(
          media.verifiedBytes,
          media.contentBytes,
        ),
        storageClassLabel: strings.storageClassOwnedOriginal,
        holdLabel: media.ownedHold
            ? strings.mediaOwnedHoldProtected
            : strings.mediaOwnedHoldMissing,
        icon: _classIcon(media.mediaClass),
      );

  String _classLabel(AppLocalizations strings, MobileMediaClass mediaClass) =>
      switch (mediaClass) {
        MobileMediaClass.image => strings.mediaClassImage,
        MobileMediaClass.video => strings.mediaClassVideo,
        MobileMediaClass.audio => strings.mediaClassAudio,
        MobileMediaClass.document => strings.mediaClassDocument,
      };

  ObmSymbol _classIcon(MobileMediaClass mediaClass) => switch (mediaClass) {
    MobileMediaClass.image => ObmSymbol.capture,
    MobileMediaClass.video => ObmSymbol.folder,
    MobileMediaClass.audio => ObmSymbol.assistant,
    MobileMediaClass.document => ObmSymbol.description,
  };
}
