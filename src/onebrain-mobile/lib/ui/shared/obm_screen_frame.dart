import 'package:flutter/material.dart';

import '../../design/onebrain_theme_extensions.dart';
import 'obm_top_app_bar.dart';

class ObmScreenFrame extends StatelessWidget {
  const ObmScreenFrame({
    required this.title,
    required this.child,
    this.showAppBar = true,
    this.leading,
    this.actions,
    super.key,
  });

  final String title;
  final Widget child;
  final bool showAppBar;
  final Widget? leading;
  final List<Widget>? actions;

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: showAppBar
        ? ObmTopAppBar(title: title, leading: leading, actions: actions)
        : null,
    body: SafeArea(
      child: Align(
        alignment: Alignment.topCenter,
        child: SingleChildScrollView(
          padding: EdgeInsets.all(context.spacing.lg),
          child: ConstrainedBox(
            constraints: BoxConstraints(
              maxWidth: context.layout.readingMaxWidth,
            ),
            child: child,
          ),
        ),
      ),
    ),
  );
}
