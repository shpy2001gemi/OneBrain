// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for English (`en`).
class AppLocalizationsEn extends AppLocalizations {
  AppLocalizationsEn([String locale = 'en']) : super(locale);

  @override
  String get appTitle => 'OneBrain';

  @override
  String get bootstrapEyebrow => 'AUTONOMOUS MOBILE NODE';

  @override
  String get entryResolving => 'Opening this node…';

  @override
  String get welcomeTitle => 'Grow ideas on your own node';

  @override
  String get welcomeBody =>
      'This phone is an independent OneBrain node. It is not a desktop companion and can keep private work local.';

  @override
  String get languageTitle => 'Choose your interface language';

  @override
  String get languageEnglish => 'English';

  @override
  String get languageVietnamese => 'Tiếng Việt';

  @override
  String get nodeFactTitle => 'Node boundary';

  @override
  String get nodeFactBody =>
      'Flutter presents typed state. Native and Rust layers own platform work, durable data, policy, signing, and tools.';

  @override
  String get registryFactTitle => 'Required knowledge data';

  @override
  String get registryFactBody =>
      'Concept Registry data is not bundled in the app. A later Init flow downloads it only after an explicit plan and confirmation.';

  @override
  String get requestFactTitle => 'Network before Init';

  @override
  String get requestFactBody =>
      'No Registry artifact request is made from this bootstrap screen.';

  @override
  String get nativeHostTitle => 'Native host';

  @override
  String get nativeHostLoading => 'Inspecting the device host…';

  @override
  String nativeHostReady(String platform, String apiVersion) {
    return '$platform host ready · API $apiVersion';
  }

  @override
  String get nativeHostUnavailable =>
      'Native host is unavailable in this environment. The bootstrap UI remains usable.';

  @override
  String get continueAction => 'Continue to device preflight';

  @override
  String get galleryAction => 'View shared components';

  @override
  String get galleryTitle => 'Shared component gallery';

  @override
  String get galleryBody =>
      'These controls wrap Material 3 primitives and reuse OneBrain semantic tokens.';

  @override
  String get primaryButton => 'Primary action';

  @override
  String get tonalButton => 'Tonal action';

  @override
  String get outlineButton => 'Outline action';

  @override
  String get statusReady => 'Bootstrap UI ready';

  @override
  String get statusWaiting => 'Registry Init not started';

  @override
  String get statusPrivate => 'Private local scope';

  @override
  String get backAction => 'Back';

  @override
  String get notImplementedTitle => 'Next foundation step';

  @override
  String get notImplementedBody =>
      'Device storage preflight and explicit Init planning are intentionally not simulated in this slice.';
}
