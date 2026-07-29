import 'package:flutter/material.dart';

import '../../design/generated/mobile_design_tokens.g.dart';

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
  Widget build(BuildContext context) =>
      AppBar(title: Text(title), leading: leading, actions: actions);
}
