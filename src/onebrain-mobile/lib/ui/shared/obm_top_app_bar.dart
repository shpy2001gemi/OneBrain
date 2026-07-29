import 'package:flutter/material.dart';

import '../../design/generated/mobile_design_tokens.g.dart';
import 'obm_icon.dart';

class ObmTopAppBar extends StatelessWidget implements PreferredSizeWidget {
  const ObmTopAppBar({
    required this.title,
    this.leading,
    this.actions,
    super.key,
  });

  final String title;
  final Widget? leading;
  final List<Widget>? actions;

  @override
  Size get preferredSize => Size.fromHeight(
    ObmDesignTokens.componentMetrics['topAppBar']!['rootHeight']!,
  );

  @override
  Widget build(BuildContext context) {
    final canPop = ModalRoute.of(context)?.canPop ?? false;
    final resolvedLeading =
        leading ??
        (canPop
            ? IconButton(
                tooltip: MaterialLocalizations.of(context).backButtonTooltip,
                onPressed: () => Navigator.maybePop(context),
                icon: const ObmIcon(ObmSymbol.arrowBack),
              )
            : null);
    return AppBar(
      title: Text(title),
      leading: resolvedLeading,
      automaticallyImplyLeading: false,
      actions: actions,
    );
  }
}
