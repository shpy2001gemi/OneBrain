import 'package:flutter/material.dart';

enum ObmStatusTone {
  ready,
  information,
  waiting,
  pausedPrivate,
  degraded,
  failed,
  offlineUnavailable,
}

@immutable
class ObmStatusPalette {
  const ObmStatusPalette({required this.container, required this.content});

  final Color container;
  final Color content;

  static ObmStatusPalette lerp(
    ObmStatusPalette a,
    ObmStatusPalette b,
    double t,
  ) => ObmStatusPalette(
    container: Color.lerp(a.container, b.container, t)!,
    content: Color.lerp(a.content, b.content, t)!,
  );
}

@immutable
class OneBrainStatusColors extends ThemeExtension<OneBrainStatusColors> {
  const OneBrainStatusColors({
    required this.ready,
    required this.information,
    required this.waiting,
    required this.pausedPrivate,
    required this.degraded,
    required this.failed,
    required this.offlineUnavailable,
  });

  final ObmStatusPalette ready;
  final ObmStatusPalette information;
  final ObmStatusPalette waiting;
  final ObmStatusPalette pausedPrivate;
  final ObmStatusPalette degraded;
  final ObmStatusPalette failed;
  final ObmStatusPalette offlineUnavailable;

  ObmStatusPalette resolve(ObmStatusTone tone) => switch (tone) {
    ObmStatusTone.ready => ready,
    ObmStatusTone.information => information,
    ObmStatusTone.waiting => waiting,
    ObmStatusTone.pausedPrivate => pausedPrivate,
    ObmStatusTone.degraded => degraded,
    ObmStatusTone.failed => failed,
    ObmStatusTone.offlineUnavailable => offlineUnavailable,
  };

  @override
  OneBrainStatusColors copyWith({
    ObmStatusPalette? ready,
    ObmStatusPalette? information,
    ObmStatusPalette? waiting,
    ObmStatusPalette? pausedPrivate,
    ObmStatusPalette? degraded,
    ObmStatusPalette? failed,
    ObmStatusPalette? offlineUnavailable,
  }) => OneBrainStatusColors(
    ready: ready ?? this.ready,
    information: information ?? this.information,
    waiting: waiting ?? this.waiting,
    pausedPrivate: pausedPrivate ?? this.pausedPrivate,
    degraded: degraded ?? this.degraded,
    failed: failed ?? this.failed,
    offlineUnavailable: offlineUnavailable ?? this.offlineUnavailable,
  );

  @override
  OneBrainStatusColors lerp(covariant OneBrainStatusColors? other, double t) {
    if (other == null) {
      return this;
    }
    return OneBrainStatusColors(
      ready: ObmStatusPalette.lerp(ready, other.ready, t),
      information: ObmStatusPalette.lerp(information, other.information, t),
      waiting: ObmStatusPalette.lerp(waiting, other.waiting, t),
      pausedPrivate: ObmStatusPalette.lerp(
        pausedPrivate,
        other.pausedPrivate,
        t,
      ),
      degraded: ObmStatusPalette.lerp(degraded, other.degraded, t),
      failed: ObmStatusPalette.lerp(failed, other.failed, t),
      offlineUnavailable: ObmStatusPalette.lerp(
        offlineUnavailable,
        other.offlineUnavailable,
        t,
      ),
    );
  }
}

@immutable
class OneBrainSpacing extends ThemeExtension<OneBrainSpacing> {
  const OneBrainSpacing({
    required this.xxs,
    required this.xs,
    required this.sm,
    required this.md,
    required this.lg,
    required this.xl,
    required this.twoXl,
    required this.threeXl,
    required this.fourXl,
    required this.fiveXl,
    required this.sixXl,
  });

  final double xxs;
  final double xs;
  final double sm;
  final double md;
  final double lg;
  final double xl;
  final double twoXl;
  final double threeXl;
  final double fourXl;
  final double fiveXl;
  final double sixXl;

  @override
  OneBrainSpacing copyWith({
    double? xxs,
    double? xs,
    double? sm,
    double? md,
    double? lg,
    double? xl,
    double? twoXl,
    double? threeXl,
    double? fourXl,
    double? fiveXl,
    double? sixXl,
  }) => OneBrainSpacing(
    xxs: xxs ?? this.xxs,
    xs: xs ?? this.xs,
    sm: sm ?? this.sm,
    md: md ?? this.md,
    lg: lg ?? this.lg,
    xl: xl ?? this.xl,
    twoXl: twoXl ?? this.twoXl,
    threeXl: threeXl ?? this.threeXl,
    fourXl: fourXl ?? this.fourXl,
    fiveXl: fiveXl ?? this.fiveXl,
    sixXl: sixXl ?? this.sixXl,
  );

  @override
  OneBrainSpacing lerp(covariant OneBrainSpacing? other, double t) {
    if (other == null) {
      return this;
    }
    double value(double a, double b) => a + (b - a) * t;
    return OneBrainSpacing(
      xxs: value(xxs, other.xxs),
      xs: value(xs, other.xs),
      sm: value(sm, other.sm),
      md: value(md, other.md),
      lg: value(lg, other.lg),
      xl: value(xl, other.xl),
      twoXl: value(twoXl, other.twoXl),
      threeXl: value(threeXl, other.threeXl),
      fourXl: value(fourXl, other.fourXl),
      fiveXl: value(fiveXl, other.fiveXl),
      sixXl: value(sixXl, other.sixXl),
    );
  }
}

@immutable
class OneBrainMotion extends ThemeExtension<OneBrainMotion> {
  const OneBrainMotion({
    required this.instant,
    required this.press,
    required this.micro,
    required this.standard,
    required this.emphasized,
    required this.long,
  });

  final Duration instant;
  final Duration press;
  final Duration micro;
  final Duration standard;
  final Duration emphasized;
  final Duration long;

  static const reduced = OneBrainMotion(
    instant: Duration.zero,
    press: Duration.zero,
    micro: Duration.zero,
    standard: Duration.zero,
    emphasized: Duration.zero,
    long: Duration.zero,
  );

  @override
  OneBrainMotion copyWith({
    Duration? instant,
    Duration? press,
    Duration? micro,
    Duration? standard,
    Duration? emphasized,
    Duration? long,
  }) => OneBrainMotion(
    instant: instant ?? this.instant,
    press: press ?? this.press,
    micro: micro ?? this.micro,
    standard: standard ?? this.standard,
    emphasized: emphasized ?? this.emphasized,
    long: long ?? this.long,
  );

  @override
  OneBrainMotion lerp(covariant OneBrainMotion? other, double t) {
    if (other == null) {
      return this;
    }
    Duration value(Duration a, Duration b) => Duration(
      microseconds:
          a.inMicroseconds +
          ((b.inMicroseconds - a.inMicroseconds) * t).round(),
    );
    return OneBrainMotion(
      instant: value(instant, other.instant),
      press: value(press, other.press),
      micro: value(micro, other.micro),
      standard: value(standard, other.standard),
      emphasized: value(emphasized, other.emphasized),
      long: value(long, other.long),
    );
  }
}

@immutable
class OneBrainGradients extends ThemeExtension<OneBrainGradients> {
  const OneBrainGradients({required this.ideaPath, required this.sunSpark});

  final LinearGradient ideaPath;
  final LinearGradient sunSpark;

  @override
  OneBrainGradients copyWith({
    LinearGradient? ideaPath,
    LinearGradient? sunSpark,
  }) => OneBrainGradients(
    ideaPath: ideaPath ?? this.ideaPath,
    sunSpark: sunSpark ?? this.sunSpark,
  );

  @override
  OneBrainGradients lerp(covariant OneBrainGradients? other, double t) {
    if (other == null) {
      return this;
    }
    return OneBrainGradients(
      ideaPath: LinearGradient.lerp(ideaPath, other.ideaPath, t)!,
      sunSpark: LinearGradient.lerp(sunSpark, other.sunSpark, t)!,
    );
  }
}

@immutable
class OneBrainDataStyle extends ThemeExtension<OneBrainDataStyle> {
  const OneBrainDataStyle({required this.value});

  final TextStyle value;

  @override
  OneBrainDataStyle copyWith({TextStyle? value}) =>
      OneBrainDataStyle(value: value ?? this.value);

  @override
  OneBrainDataStyle lerp(covariant OneBrainDataStyle? other, double t) =>
      other == null
      ? this
      : OneBrainDataStyle(value: TextStyle.lerp(value, other.value, t)!);
}

@immutable
class OneBrainLayout extends ThemeExtension<OneBrainLayout> {
  const OneBrainLayout({
    required this.compactBreakpoint,
    required this.expandedBreakpoint,
    required this.onboardingMaxWidth,
    required this.readingMaxWidth,
  });

  final double compactBreakpoint;
  final double expandedBreakpoint;
  final double onboardingMaxWidth;
  final double readingMaxWidth;

  @override
  OneBrainLayout copyWith({
    double? compactBreakpoint,
    double? expandedBreakpoint,
    double? onboardingMaxWidth,
    double? readingMaxWidth,
  }) => OneBrainLayout(
    compactBreakpoint: compactBreakpoint ?? this.compactBreakpoint,
    expandedBreakpoint: expandedBreakpoint ?? this.expandedBreakpoint,
    onboardingMaxWidth: onboardingMaxWidth ?? this.onboardingMaxWidth,
    readingMaxWidth: readingMaxWidth ?? this.readingMaxWidth,
  );

  @override
  OneBrainLayout lerp(covariant OneBrainLayout? other, double t) {
    if (other == null) {
      return this;
    }
    double value(double a, double b) => a + (b - a) * t;
    return OneBrainLayout(
      compactBreakpoint: value(compactBreakpoint, other.compactBreakpoint),
      expandedBreakpoint: value(expandedBreakpoint, other.expandedBreakpoint),
      onboardingMaxWidth: value(onboardingMaxWidth, other.onboardingMaxWidth),
      readingMaxWidth: value(readingMaxWidth, other.readingMaxWidth),
    );
  }
}

extension OneBrainThemeContext on BuildContext {
  OneBrainSpacing get spacing => Theme.of(this).extension<OneBrainSpacing>()!;

  OneBrainStatusColors get statusColors =>
      Theme.of(this).extension<OneBrainStatusColors>()!;

  OneBrainGradients get gradients =>
      Theme.of(this).extension<OneBrainGradients>()!;

  OneBrainMotion get motion => MediaQuery.disableAnimationsOf(this)
      ? OneBrainMotion.reduced
      : Theme.of(this).extension<OneBrainMotion>()!;

  OneBrainDataStyle get dataStyle =>
      Theme.of(this).extension<OneBrainDataStyle>()!;

  OneBrainLayout get layout => Theme.of(this).extension<OneBrainLayout>()!;
}
