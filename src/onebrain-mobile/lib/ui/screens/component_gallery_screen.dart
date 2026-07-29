import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../l10n/app_localizations.dart';
import '../shared/obm_button.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_node_fact_card.dart';
import '../shared/obm_screen_frame.dart';
import '../shared/obm_status_badge.dart';

class ComponentGalleryScreen extends StatelessWidget {
  const ComponentGalleryScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    return ObmScreenFrame(
      title: strings.galleryTitle,
      leading: IconButton(
        tooltip: strings.backAction,
        onPressed: context.pop,
        icon: const ObmIcon(ObmSymbol.arrowBack),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            strings.galleryBody,
            style: Theme.of(context).textTheme.bodyLarge,
          ),
          SizedBox(height: context.spacing.twoXl),
          ObmButton(label: strings.primaryButton, onPressed: () {}),
          SizedBox(height: context.spacing.sm),
          ObmButton(
            label: strings.tonalButton,
            variant: ObmButtonVariant.tonal,
            onPressed: () {},
          ),
          SizedBox(height: context.spacing.sm),
          ObmButton(
            label: strings.outlineButton,
            variant: ObmButtonVariant.outline,
            onPressed: () {},
          ),
          SizedBox(height: context.spacing.twoXl),
          Wrap(
            spacing: context.spacing.sm,
            runSpacing: context.spacing.sm,
            children: [
              ObmStatusBadge(
                label: strings.statusReady,
                tone: ObmStatusTone.ready,
              ),
              ObmStatusBadge(
                label: strings.statusWaiting,
                tone: ObmStatusTone.waiting,
              ),
              ObmStatusBadge(
                label: strings.statusPrivate,
                tone: ObmStatusTone.pausedPrivate,
              ),
            ],
          ),
          SizedBox(height: context.spacing.twoXl),
          ObmNodeFactCard(
            title: strings.nodeFactTitle,
            body: strings.nodeFactBody,
            icon: ObmSymbol.hub,
            tone: ObmStatusTone.information,
          ),
        ],
      ),
    );
  }
}
