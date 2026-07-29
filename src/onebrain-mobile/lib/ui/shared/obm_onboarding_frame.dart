import 'package:flutter/material.dart';

import '../../design/onebrain_theme_extensions.dart';
import 'obm_button.dart';
import 'obm_screen_frame.dart';

class ObmOnboardingFrame extends StatelessWidget {
  const ObmOnboardingFrame({
    required this.appTitle,
    required this.stepLabel,
    required this.title,
    required this.body,
    required this.child,
    required this.primaryLabel,
    required this.onPrimary,
    this.onBack,
    this.primaryDisabledReason,
    super.key,
  });

  final String appTitle;
  final String stepLabel;
  final String title;
  final String body;
  final Widget child;
  final String primaryLabel;
  final VoidCallback? onPrimary;
  final VoidCallback? onBack;
  final String? primaryDisabledReason;

  @override
  Widget build(BuildContext context) => ObmScreenFrame(
    title: appTitle,
    child: ConstrainedBox(
      constraints: BoxConstraints(maxWidth: context.layout.onboardingMaxWidth),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            stepLabel,
            style: Theme.of(context).textTheme.labelMedium?.copyWith(
              color: Theme.of(context).colorScheme.primary,
            ),
          ),
          SizedBox(height: context.spacing.sm),
          Text(title, style: Theme.of(context).textTheme.headlineLarge),
          SizedBox(height: context.spacing.md),
          Text(body, style: Theme.of(context).textTheme.bodyLarge),
          SizedBox(height: context.spacing.twoXl),
          child,
          SizedBox(height: context.spacing.twoXl),
          ObmButton(
            label: primaryLabel,
            onPressed: onPrimary,
            disabledReason: primaryDisabledReason,
          ),
          if (onBack != null) ...[
            SizedBox(height: context.spacing.sm),
            ObmButton(
              label: MaterialLocalizations.of(context).backButtonTooltip,
              variant: ObmButtonVariant.text,
              onPressed: onBack,
            ),
          ],
        ],
      ),
    ),
  );
}
