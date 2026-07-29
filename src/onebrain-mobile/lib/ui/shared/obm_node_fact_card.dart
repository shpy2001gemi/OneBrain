import 'package:flutter/material.dart';

import '../../design/onebrain_theme_extensions.dart';
import 'obm_icon.dart';
import 'obm_status_badge.dart';

class ObmNodeFactCard extends StatelessWidget {
  const ObmNodeFactCard({
    required this.title,
    required this.body,
    required this.icon,
    required this.tone,
    this.statusLabel,
    super.key,
  });

  final String title;
  final String body;
  final ObmSymbol icon;
  final ObmStatusTone tone;
  final String? statusLabel;

  @override
  Widget build(BuildContext context) {
    final palette = context.statusColors.resolve(tone);
    final iconBadge = DecoratedBox(
      decoration: ShapeDecoration(
        color: palette.container,
        shape: const CircleBorder(),
      ),
      child: Padding(
        padding: EdgeInsets.all(context.spacing.md),
        child: ObmIcon(icon, color: palette.content),
      ),
    );
    final titleText = Text(
      title,
      style: Theme.of(context).textTheme.titleMedium,
    );
    final usesStackedHeader = MediaQuery.textScalerOf(context).scale(1) > 1.5;
    return Card(
      child: Padding(
        padding: EdgeInsets.all(context.spacing.lg),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (usesStackedHeader)
              Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  iconBadge,
                  SizedBox(height: context.spacing.md),
                  titleText,
                ],
              )
            else
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  iconBadge,
                  SizedBox(width: context.spacing.md),
                  Expanded(child: titleText),
                ],
              ),
            SizedBox(height: context.spacing.md),
            Text(body, style: Theme.of(context).textTheme.bodyMedium),
            if (statusLabel != null) ...[
              SizedBox(height: context.spacing.md),
              ObmStatusBadge(label: statusLabel!, tone: tone),
            ],
          ],
        ),
      ),
    );
  }
}
