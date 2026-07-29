import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../design/onebrain_theme.dart';
import '../l10n/app_localizations.dart';
import 'app_router.dart';
import 'locale_controller.dart';

class OneBrainApp extends ConsumerWidget {
  const OneBrainApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final router = ref.watch(appRouterProvider);
    final locale = ref.watch(localeControllerProvider);
    return MaterialApp.router(
      onGenerateTitle: (context) => AppLocalizations.of(context).appTitle,
      debugShowCheckedModeBanner: false,
      locale: locale,
      supportedLocales: AppLocalizations.supportedLocales,
      localizationsDelegates: const [
        AppLocalizations.delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ],
      theme: OneBrainTheme.light,
      darkTheme: OneBrainTheme.dark,
      highContrastTheme: OneBrainTheme.highContrastLight,
      highContrastDarkTheme: OneBrainTheme.highContrastDark,
      themeMode: ThemeMode.system,
      routerConfig: router,
    );
  }
}
