import 'package:flutter/material.dart';

import '../../design/onebrain_theme_extensions.dart';

class ObmStatusBadge extends StatelessWidget {
  const ObmStatusBadge({required this.label, required this.tone, super.key});

  final String label;
  final ObmStatusTone tone;

  @override
  Widget build(BuildContext context) {
    final palette = context.statusColors.resolve(tone);
    return Semantics(
      label: label,
      container: true,
      child: DecoratedBox(
        decoration: ShapeDecoration(
          color: palette.container,
          shape: const StadiumBorder(),
        ),
        child: Padding(
          padding: EdgeInsets.symmetric(
            horizontal: context.spacing.md,
            vertical: context.spacing.sm,
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(_icon, color: palette.content),
              SizedBox(width: context.spacing.sm),
              Flexible(
                child: Text(
                  label,
                  style: Theme.of(
                    context,
                  ).textTheme.labelMedium?.copyWith(color: palette.content),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  IconData get _icon => switch (tone) {
    ObmStatusTone.ready => Icons.check_circle_outline,
    ObmStatusTone.information => Icons.info_outline,
    ObmStatusTone.waiting => Icons.schedule,
    ObmStatusTone.pausedPrivate => Icons.shield_outlined,
    ObmStatusTone.degraded => Icons.build_circle_outlined,
    ObmStatusTone.failed => Icons.cancel_outlined,
    ObmStatusTone.offlineUnavailable => Icons.cloud_off_outlined,
  };
}
