import 'package:flutter/material.dart';

import '../../design/onebrain_theme_extensions.dart';
import 'obm_icon.dart';
import 'obm_status_badge.dart';

class ObmScopeBanner extends StatelessWidget {
  const ObmScopeBanner({
    required this.title,
    required this.body,
    required this.tone,
    required this.icon,
    this.statusLabel,
    super.key,
  });

  final String title;
  final String body;
  final ObmStatusTone tone;
  final ObmSymbol icon;
  final String? statusLabel;

  @override
  Widget build(BuildContext context) {
    final palette = context.statusColors.resolve(tone);
    return Semantics(
      container: true,
      child: Card(
        color: palette.container,
        child: Padding(
          padding: EdgeInsets.all(context.spacing.lg),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              ObmIcon(icon, color: palette.content),
              SizedBox(width: context.spacing.md),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      title,
                      style: Theme.of(
                        context,
                      ).textTheme.titleMedium?.copyWith(color: palette.content),
                    ),
                    SizedBox(height: context.spacing.xs),
                    Text(
                      body,
                      style: Theme.of(
                        context,
                      ).textTheme.bodyMedium?.copyWith(color: palette.content),
                    ),
                    if (statusLabel != null) ...[
                      SizedBox(height: context.spacing.sm),
                      ObmStatusBadge(label: statusLabel!, tone: tone),
                    ],
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
