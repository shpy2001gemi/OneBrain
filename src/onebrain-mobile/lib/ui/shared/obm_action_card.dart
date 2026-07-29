import 'package:flutter/material.dart';

import '../../design/onebrain_theme_extensions.dart';
import 'obm_button.dart';
import 'obm_icon.dart';
import 'obm_status_badge.dart';

class ObmActionCard extends StatelessWidget {
  const ObmActionCard({
    required this.title,
    required this.body,
    required this.icon,
    this.tone = ObmStatusTone.information,
    this.statusLabel,
    this.actionLabel,
    this.onPressed,
    this.disabledReason,
    super.key,
  });

  final String title;
  final String body;
  final ObmSymbol icon;
  final ObmStatusTone tone;
  final String? statusLabel;
  final String? actionLabel;
  final VoidCallback? onPressed;
  final String? disabledReason;

  @override
  Widget build(BuildContext context) {
    final palette = context.statusColors.resolve(tone);
    return Card(
      child: Padding(
        padding: EdgeInsets.all(context.spacing.lg),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
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
                      Text(body, style: Theme.of(context).textTheme.bodyMedium),
                    ],
                  ),
                ),
              ],
            ),
            if (statusLabel != null) ...[
              SizedBox(height: context.spacing.md),
              Align(
                alignment: AlignmentDirectional.centerStart,
                child: ObmStatusBadge(label: statusLabel!, tone: tone),
              ),
            ],
            if (actionLabel != null) ...[
              SizedBox(height: context.spacing.md),
              ObmButton(
                label: actionLabel!,
                variant: ObmButtonVariant.tonal,
                onPressed: onPressed,
                disabledReason: disabledReason,
              ),
              if (onPressed == null && disabledReason != null) ...[
                SizedBox(height: context.spacing.sm),
                Text(
                  disabledReason!,
                  style: Theme.of(context).textTheme.bodyMedium,
                ),
              ],
            ],
          ],
        ),
      ),
    );
  }
}
