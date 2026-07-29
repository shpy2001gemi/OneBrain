import 'package:flutter/material.dart';

import 'generated/mobile_design_tokens.g.dart';
import 'onebrain_theme_extensions.dart';

abstract final class OneBrainTheme {
  static ThemeData get light => _build(Brightness.light);

  static ThemeData get dark => _build(Brightness.dark);

  static ThemeData get highContrastLight =>
      _build(Brightness.light, highContrast: true);

  static ThemeData get highContrastDark =>
      _build(Brightness.dark, highContrast: true);

  static ThemeData _build(Brightness brightness, {bool highContrast = false}) {
    final isDark = brightness == Brightness.dark;
    final base = isDark
        ? ObmDesignTokens.darkColors
        : ObmDesignTokens.lightColors;
    final overrides = highContrast
        ? (isDark
              ? ObmDesignTokens.highContrastDarkColors
              : ObmDesignTokens.highContrastLightColors)
        : const <String, Color>{};
    final colors = <String, Color>{...base, ...overrides};
    Color color(String key) => colors[key]!;
    double metric(String group, String key) =>
        ObmDesignTokens.componentMetrics[group]![key]!;

    final colorScheme = ColorScheme(
      brightness: brightness,
      primary: color('primary'),
      onPrimary: color('onPrimary'),
      primaryContainer: color('primaryContainer'),
      onPrimaryContainer: color('onPrimaryContainer'),
      secondary: color('secondary'),
      onSecondary: color('onSecondary'),
      secondaryContainer: color('secondaryContainer'),
      onSecondaryContainer: color('onSecondaryContainer'),
      tertiary: color('tertiary'),
      onTertiary: color('onTertiary'),
      tertiaryContainer: color('tertiaryContainer'),
      onTertiaryContainer: color('onTertiaryContainer'),
      error: color('error'),
      onError: color('onError'),
      errorContainer: color('errorContainer'),
      onErrorContainer: color('onErrorContainer'),
      surface: color('surface'),
      onSurface: color('onSurface'),
      surfaceContainer: color('surfaceSoft'),
      outline: color('borderStrong'),
      outlineVariant: color('border'),
      scrim: color('scrim'),
    );
    final textTheme = _textTheme(
      primary: color('textPrimary'),
      secondary: color('textSecondary'),
    );
    final buttonShape = RoundedRectangleBorder(
      borderRadius: BorderRadius.circular(metric('button', 'radius')),
    );
    final buttonPadding = EdgeInsets.symmetric(
      horizontal: metric('button', 'horizontalPadding'),
    );
    final buttonMinimumSize = Size.fromHeight(metric('button', 'height'));
    final buttonStyle = ButtonStyle(
      minimumSize: WidgetStatePropertyAll(buttonMinimumSize),
      padding: WidgetStatePropertyAll(buttonPadding),
      shape: WidgetStatePropertyAll(buttonShape),
      textStyle: WidgetStatePropertyAll(textTheme.labelLarge),
    );

    return ThemeData(
      useMaterial3: true,
      brightness: brightness,
      colorScheme: colorScheme,
      scaffoldBackgroundColor: color('background'),
      textTheme: textTheme,
      appBarTheme: AppBarTheme(
        centerTitle: false,
        elevation: 0,
        scrolledUnderElevation: 0,
        backgroundColor: color('background'),
        foregroundColor: color('textPrimary'),
        surfaceTintColor: color('background'),
        titleTextStyle: textTheme.titleLarge,
        toolbarHeight: metric('topAppBar', 'rootHeight'),
      ),
      cardTheme: CardThemeData(
        color: color('surface'),
        elevation: 0,
        margin: EdgeInsets.zero,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(metric('card', 'radius')),
          side: BorderSide(
            color: color('border'),
            width: metric('card', 'borderWidth'),
          ),
        ),
      ),
      filledButtonTheme: FilledButtonThemeData(style: buttonStyle),
      outlinedButtonTheme: OutlinedButtonThemeData(style: buttonStyle),
      textButtonTheme: TextButtonThemeData(style: buttonStyle),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: color('surface'),
        contentPadding: EdgeInsets.symmetric(
          horizontal: metric('input', 'horizontalPadding'),
          vertical: metric('input', 'verticalPadding'),
        ),
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(metric('input', 'radius')),
          borderSide: BorderSide(color: color('border')),
        ),
      ),
      extensions: <ThemeExtension<dynamic>>[
        _spacing(),
        _statusColors(isDark),
        _motion(),
        _gradients(),
        OneBrainDataStyle(value: _tokenTextStyle('data', color('textPrimary'))),
        OneBrainLayout(
          compactBreakpoint:
              ObmDesignTokens.layoutBreakpoints['compactMaxExclusive']!,
          expandedBreakpoint: ObmDesignTokens.layoutBreakpoints['expandedMin']!,
          onboardingMaxWidth: ObmDesignTokens.layoutMaxWidth['onboarding']!,
          readingMaxWidth: ObmDesignTokens.layoutMaxWidth['reading']!,
        ),
      ],
    );
  }

  static TextTheme _textTheme({
    required Color primary,
    required Color secondary,
  }) => TextTheme(
    displayLarge: _tokenTextStyle('display', primary),
    headlineLarge: _tokenTextStyle('headlineLarge', primary),
    headlineMedium: _tokenTextStyle('headlineMedium', primary),
    titleLarge: _tokenTextStyle('titleLarge', primary),
    titleMedium: _tokenTextStyle('titleMedium', primary),
    bodyLarge: _tokenTextStyle('bodyLarge', primary),
    bodyMedium: _tokenTextStyle('bodyMedium', secondary),
    labelLarge: _tokenTextStyle('labelLarge', primary),
    labelMedium: _tokenTextStyle('labelMedium', secondary),
  );

  static TextStyle _tokenTextStyle(String key, Color color) {
    final token = ObmDesignTokens.typography[key]!;
    final familyToken = ObmDesignTokens.typographyFamilies[key]!;
    final family = ObmDesignTokens.fontFamilies[familyToken]!.first;
    return TextStyle(
      color: color,
      fontFamily: family,
      fontSize: token['size'],
      height: token['lineHeight']! / token['size']!,
      fontWeight: _fontWeight(token['weight']!.round()),
      letterSpacing: token['letterSpacing'],
      fontFeatures: key == 'data'
          ? const <FontFeature>[FontFeature.tabularFigures()]
          : null,
    );
  }

  static FontWeight _fontWeight(int weight) => switch (weight) {
    400 => FontWeight.w400,
    600 => FontWeight.w600,
    700 => FontWeight.w700,
    800 => FontWeight.w800,
    _ => FontWeight.w400,
  };

  static OneBrainSpacing _spacing() {
    final values = ObmDesignTokens.spacing;
    return OneBrainSpacing(
      xxs: values['xxs']!,
      xs: values['xs']!,
      sm: values['sm']!,
      md: values['md']!,
      lg: values['lg']!,
      xl: values['xl']!,
      twoXl: values['2xl']!,
      threeXl: values['3xl']!,
      fourXl: values['4xl']!,
      fiveXl: values['5xl']!,
      sixXl: values['6xl']!,
    );
  }

  static OneBrainMotion _motion() {
    final values = ObmDesignTokens.motionDuration;
    Duration duration(String key) =>
        Duration(milliseconds: values[key]!.round());
    return OneBrainMotion(
      instant: duration('instant'),
      press: duration('press'),
      micro: duration('micro'),
      standard: duration('standard'),
      emphasized: duration('emphasized'),
      long: duration('long'),
    );
  }

  static OneBrainGradients _gradients() => OneBrainGradients(
    ideaPath: LinearGradient(colors: ObmDesignTokens.gradients['ideaPath']!),
    sunSpark: LinearGradient(colors: ObmDesignTokens.gradients['sunSpark']!),
  );

  static OneBrainStatusColors _statusColors(bool dark) {
    final containers = dark
        ? ObmDesignTokens.darkStatusContainer
        : ObmDesignTokens.lightStatusContainer;
    final contents = dark
        ? ObmDesignTokens.darkStatusContent
        : ObmDesignTokens.lightStatusContent;
    ObmStatusPalette palette(String key) =>
        ObmStatusPalette(container: containers[key]!, content: contents[key]!);
    return OneBrainStatusColors(
      ready: palette('ready'),
      information: palette('information'),
      waiting: palette('waiting'),
      pausedPrivate: palette('pausedPrivate'),
      degraded: palette('degraded'),
      failed: palette('failed'),
      offlineUnavailable: palette('offlineUnavailable'),
    );
  }
}
