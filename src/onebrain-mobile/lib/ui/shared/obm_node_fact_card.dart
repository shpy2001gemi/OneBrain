import 'package:flutter/material.dart';

import '../../design/onebrain_theme_extensions.dart';
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
  final IconData icon;
  final ObmStatusTone tone;
  final String? statusLabel;

  @override
  Widget build(BuildContext context) {
    final palette = context.statusColors.resolve(tone);
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
                    child: Icon(icon, color: palette.content),
                  ),
                ),
                SizedBox(width: context.spacing.md),
                Expanded(
                  child: Text(
                    title,
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                ),
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
