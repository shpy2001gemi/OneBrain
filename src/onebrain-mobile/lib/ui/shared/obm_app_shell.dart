import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import '../../design/onebrain_theme_extensions.dart';
import 'obm_icon.dart';
import 'obm_top_app_bar.dart';

class ObmAppShell extends StatelessWidget {
  const ObmAppShell({
    required this.title,
    required this.selectedIndex,
    required this.destinations,
    required this.child,
    super.key,
  });

  final String title;
  final int selectedIndex;
  final List<ObmShellDestination> destinations;
  final Widget child;

  void _select(BuildContext context, int index) {
    if (index != selectedIndex) {
      context.go(destinations[index].location);
    }
  }

  @override
  Widget build(BuildContext context) {
    final expanded =
        MediaQuery.sizeOf(context).width >= context.layout.expandedBreakpoint;
    final content = SafeArea(
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
    );
    return Scaffold(
      appBar: ObmTopAppBar(title: title),
      body: expanded
          ? Row(
              children: [
                NavigationRail(
                  selectedIndex: selectedIndex,
                  onDestinationSelected: (index) => _select(context, index),
                  labelType: NavigationRailLabelType.all,
                  destinations: [
                    for (final destination in destinations)
                      NavigationRailDestination(
                        icon: ObmIcon(destination.icon),
                        label: Text(destination.label),
                      ),
                  ],
                ),
                const VerticalDivider(),
                Expanded(child: content),
              ],
            )
          : content,
      bottomNavigationBar: expanded
          ? null
          : NavigationBar(
              selectedIndex: selectedIndex,
              onDestinationSelected: (index) => _select(context, index),
              destinations: [
                for (final destination in destinations)
                  NavigationDestination(
                    icon: ObmIcon(
                      destination.icon,
                      size: ObmIconSize.navigation,
                    ),
                    label: destination.label,
                  ),
              ],
            ),
    );
  }
}

class ObmShellDestination {
  const ObmShellDestination({
    required this.label,
    required this.icon,
    required this.location,
  });

  final String label;
  final ObmSymbol icon;
  final String location;
}
