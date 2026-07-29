import 'package:flutter/material.dart';

import '../../design/onebrain_theme_extensions.dart';

enum ObmButtonVariant { primary, tonal, outline, text, destructive }

class ObmButton extends StatelessWidget {
  const ObmButton({
    required this.label,
    required this.onPressed,
    this.variant = ObmButtonVariant.primary,
    this.leadingIcon,
    this.busy = false,
    this.disabledReason,
    super.key,
  });

  final String label;
  final VoidCallback? onPressed;
  final ObmButtonVariant variant;
  final IconData? leadingIcon;
  final bool busy;
  final String? disabledReason;

  @override
  Widget build(BuildContext context) {
    final action = busy ? null : onPressed;
    final content = Row(
      mainAxisSize: MainAxisSize.min,
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        if (busy)
          SizedBox.square(
            dimension: context.spacing.twoXl,
            child: const CircularProgressIndicator(),
          )
        else if (leadingIcon != null)
          Icon(leadingIcon),
        if (busy || leadingIcon != null) SizedBox(width: context.spacing.sm),
        Flexible(child: Text(label)),
      ],
    );
    final button = switch (variant) {
      ObmButtonVariant.primary => FilledButton(
        onPressed: action,
        child: content,
      ),
      ObmButtonVariant.tonal => FilledButton.tonal(
        onPressed: action,
        child: content,
      ),
      ObmButtonVariant.outline => OutlinedButton(
        onPressed: action,
        child: content,
      ),
      ObmButtonVariant.text => TextButton(onPressed: action, child: content),
      ObmButtonVariant.destructive => FilledButton(
        style: FilledButton.styleFrom(
          backgroundColor: Theme.of(context).colorScheme.error,
          foregroundColor: Theme.of(context).colorScheme.onError,
        ),
        onPressed: action,
        child: content,
      ),
    };
    return Semantics(
      hint: action == null ? disabledReason : null,
      child: button,
    );
  }
}
