import 'package:flutter/material.dart';

import '../../design/onebrain_theme_extensions.dart';
import 'obm_icon.dart';
import 'obm_status_badge.dart';

/// OBM-CMP-DAT-003 with OBM-CMP-MED-003 storage-class treatment.
class ObmMediaCard extends StatelessWidget {
  const ObmMediaCard({
    required this.title,
    required this.mimeType,
    required this.mediaRef,
    required this.verifiedBytesLabel,
    required this.storageClassLabel,
    required this.holdLabel,
    required this.icon,
    super.key,
  });

  final String title;
  final String mimeType;
  final String mediaRef;
  final String verifiedBytesLabel;
  final String storageClassLabel;
  final String holdLabel;
  final ObmSymbol icon;

  @override
  Widget build(BuildContext context) {
    final palette = context.statusColors.resolve(ObmStatusTone.ready);
    return Card(
      child: Padding(
        padding: EdgeInsets.all(context.spacing.lg),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                DecoratedBox(
                  decoration: ShapeDecoration(
                    color: palette.container,
                    shape: const CircleBorder(),
                  ),
                  child: Padding(
                    padding: EdgeInsets.all(context.spacing.md),
                    child: ObmIcon(icon, color: palette.content),
                  ),
                ),
                SizedBox(width: context.spacing.md),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        title,
                        style: Theme.of(context).textTheme.titleMedium,
                      ),
                      SizedBox(height: context.spacing.xs),
                      Text(
                        mimeType,
                        style: Theme.of(context).textTheme.bodyMedium,
                      ),
                    ],
                  ),
                ),
              ],
            ),
            SizedBox(height: context.spacing.md),
            Wrap(
              spacing: context.spacing.sm,
              runSpacing: context.spacing.sm,
              children: [
                ObmStatusBadge(
                  label: storageClassLabel,
                  tone: ObmStatusTone.ready,
                ),
                ObmStatusBadge(
                  label: holdLabel,
                  tone: ObmStatusTone.pausedPrivate,
                ),
              ],
            ),
            SizedBox(height: context.spacing.md),
            Text(
              verifiedBytesLabel,
              style: Theme.of(context).textTheme.bodyMedium,
            ),
            SizedBox(height: context.spacing.sm),
            SelectableText(
              mediaRef,
              style: Theme.of(
                context,
              ).textTheme.bodySmall?.copyWith(fontFamily: 'Roboto Mono'),
            ),
          ],
        ),
      ),
    );
  }
}
