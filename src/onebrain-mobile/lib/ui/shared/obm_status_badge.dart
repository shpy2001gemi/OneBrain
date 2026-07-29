import 'package:flutter/material.dart';

import '../../design/onebrain_theme_extensions.dart';
import 'obm_icon.dart';

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
              ObmIcon(_icon, size: ObmIconSize.inline, color: palette.content),
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

  ObmSymbol get _icon => switch (tone) {
    ObmStatusTone.ready => ObmSymbol.checkCircle,
    ObmStatusTone.information => ObmSymbol.info,
    ObmStatusTone.waiting => ObmSymbol.schedule,
    ObmStatusTone.pausedPrivate => ObmSymbol.shield,
    ObmStatusTone.degraded => ObmSymbol.buildCircle,
    ObmStatusTone.failed => ObmSymbol.cancel,
    ObmStatusTone.offlineUnavailable => ObmSymbol.cloudOff,
  };
}
