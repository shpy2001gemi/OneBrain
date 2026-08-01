import 'package:flutter/material.dart';

import '../../design/onebrain_theme_extensions.dart';
import 'obm_button.dart';
import 'obm_icon.dart';
import 'obm_status_badge.dart';

@immutable
class ObmPlanFact {
  const ObmPlanFact({required this.label, required this.value});

  final String label;
  final String value;
}

/// `OBM-CMP-OPS-002` — exact signed target and capacity plan.
class ObmExactPlanPanel extends StatelessWidget {
  const ObmExactPlanPanel({
    required this.title,
    required this.subtitle,
    required this.facts,
    required this.statusLabel,
    required this.tone,
    super.key,
  });

  final String title;
  final String subtitle;
  final List<ObmPlanFact> facts;
  final String statusLabel;
  final ObmStatusTone tone;

  @override
  Widget build(BuildContext context) => Card(
    child: Padding(
      padding: EdgeInsets.all(context.spacing.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const ObmIcon(ObmSymbol.description),
              SizedBox(width: context.spacing.md),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(title, style: Theme.of(context).textTheme.titleLarge),
                    SizedBox(height: context.spacing.xs),
                    Text(
                      subtitle,
                      style: Theme.of(context).textTheme.bodyMedium,
                    ),
                  ],
                ),
              ),
            ],
          ),
          SizedBox(height: context.spacing.lg),
          ...facts.map(
            (fact) => Padding(
              padding: EdgeInsets.only(bottom: context.spacing.sm),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(
                    child: Text(
                      fact.label,
                      style: Theme.of(context).textTheme.bodyMedium,
                    ),
                  ),
                  SizedBox(width: context.spacing.md),
                  Flexible(
                    child: Text(
                      fact.value,
                      textAlign: TextAlign.end,
                      style: context.dataStyle.value,
                    ),
                  ),
                ],
              ),
            ),
          ),
          SizedBox(height: context.spacing.sm),
          Align(
            alignment: AlignmentDirectional.centerStart,
            child: ObmStatusBadge(label: statusLabel, tone: tone),
          ),
        ],
      ),
    ),
  );
}

/// `OBM-CMP-OPS-003` — live native resource facts.
class ObmResourceFacts extends StatelessWidget {
  const ObmResourceFacts({
    required this.title,
    required this.availableLabel,
    required this.availableValue,
    required this.requiredLabel,
    required this.requiredValue,
    required this.reserveLabel,
    required this.reserveValue,
    required this.capacityLabel,
    required this.capacityValue,
    required this.hasCapacity,
    super.key,
  });

  final String title;
  final String availableLabel;
  final String availableValue;
  final String requiredLabel;
  final String requiredValue;
  final String reserveLabel;
  final String reserveValue;
  final String capacityLabel;
  final String capacityValue;
  final bool hasCapacity;

  @override
  Widget build(BuildContext context) {
    final tone = hasCapacity ? ObmStatusTone.ready : ObmStatusTone.degraded;
    final palette = context.statusColors.resolve(tone);
    return Card(
      child: Padding(
        padding: EdgeInsets.all(context.spacing.lg),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              children: [
                ObmIcon(ObmSymbol.storage, color: palette.content),
                SizedBox(width: context.spacing.md),
                Expanded(
                  child: Text(
                    title,
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                ),
              ],
            ),
            SizedBox(height: context.spacing.lg),
            _ResourceRow(label: availableLabel, value: availableValue),
            _ResourceRow(label: requiredLabel, value: requiredValue),
            _ResourceRow(label: reserveLabel, value: reserveValue),
            _ResourceRow(label: capacityLabel, value: capacityValue),
          ],
        ),
      ),
    );
  }
}

class _ResourceRow extends StatelessWidget {
  const _ResourceRow({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) => Padding(
    padding: EdgeInsets.only(bottom: context.spacing.sm),
    child: Row(
      children: [
        Expanded(child: Text(label)),
        SizedBox(width: context.spacing.md),
        Text(value, style: context.dataStyle.value),
      ],
    ),
  );
}

/// `OBM-CMP-ACT-003` — explicit Defer and Confirm authority transition.
class ObmSplitReviewActions extends StatelessWidget {
  const ObmSplitReviewActions({
    required this.deferLabel,
    required this.confirmLabel,
    required this.onDefer,
    required this.onConfirm,
    this.busy = false,
    this.confirmDisabledReason,
    super.key,
  });

  final String deferLabel;
  final String confirmLabel;
  final VoidCallback? onDefer;
  final VoidCallback? onConfirm;
  final bool busy;
  final String? confirmDisabledReason;

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, constraints) {
      final defer = ObmButton(
        label: deferLabel,
        variant: ObmButtonVariant.outline,
        onPressed: busy ? null : onDefer,
      );
      final confirm = ObmButton(
        label: confirmLabel,
        onPressed: busy ? null : onConfirm,
        busy: busy,
        disabledReason: confirmDisabledReason,
      );
      if (constraints.maxWidth < context.layout.compactBreakpoint) {
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            defer,
            SizedBox(height: context.spacing.sm),
            confirm,
          ],
        );
      }
      return Row(
        children: [
          Expanded(child: defer),
          SizedBox(width: context.spacing.md),
          Expanded(child: confirm),
        ],
      );
    },
  );
}
