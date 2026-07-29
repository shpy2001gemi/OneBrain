import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:intl/intl.dart' as intl;

import 'app_localizations_en.dart';
import 'app_localizations_vi.dart';

// ignore_for_file: type=lint

/// Callers can lookup localized strings with an instance of AppLocalizations
/// returned by `AppLocalizations.of(context)`.
///
/// Applications need to include `AppLocalizations.delegate()` in their app's
/// `localizationDelegates` list, and the locales they support in the app's
/// `supportedLocales` list. For example:
///
/// ```dart
/// import 'l10n/app_localizations.dart';
///
/// return MaterialApp(
///   localizationsDelegates: AppLocalizations.localizationsDelegates,
///   supportedLocales: AppLocalizations.supportedLocales,
///   home: MyApplicationHome(),
/// );
/// ```
///
/// ## Update pubspec.yaml
///
/// Please make sure to update your pubspec.yaml to include the following
/// packages:
///
/// ```yaml
/// dependencies:
///   # Internationalization support.
///   flutter_localizations:
///     sdk: flutter
///   intl: any # Use the pinned version from flutter_localizations
///
///   # Rest of dependencies
/// ```
///
/// ## iOS Applications
///
/// iOS applications define key application metadata, including supported
/// locales, in an Info.plist file that is built into the application bundle.
/// To configure the locales supported by your app, you’ll need to edit this
/// file.
///
/// First, open your project’s ios/Runner.xcworkspace Xcode workspace file.
/// Then, in the Project Navigator, open the Info.plist file under the Runner
/// project’s Runner folder.
///
/// Next, select the Information Property List item, select Add Item from the
/// Editor menu, then select Localizations from the pop-up menu.
///
/// Select and expand the newly-created Localizations item then, for each
/// locale your application supports, add a new item and select the locale
/// you wish to add from the pop-up menu in the Value field. This list should
/// be consistent with the languages listed in the AppLocalizations.supportedLocales
/// property.
abstract class AppLocalizations {
  AppLocalizations(String locale)
    : localeName = intl.Intl.canonicalizedLocale(locale.toString());

  final String localeName;

  static AppLocalizations of(BuildContext context) {
    return Localizations.of<AppLocalizations>(context, AppLocalizations)!;
  }

  static const LocalizationsDelegate<AppLocalizations> delegate =
      _AppLocalizationsDelegate();

  /// A list of this localizations delegate along with the default localizations
  /// delegates.
  ///
  /// Returns a list of localizations delegates containing this delegate along with
  /// GlobalMaterialLocalizations.delegate, GlobalCupertinoLocalizations.delegate,
  /// and GlobalWidgetsLocalizations.delegate.
  ///
  /// Additional delegates can be added by appending to this list in
  /// MaterialApp. This list does not have to be used at all if a custom list
  /// of delegates is preferred or required.
  static const List<LocalizationsDelegate<dynamic>> localizationsDelegates =
      <LocalizationsDelegate<dynamic>>[
        delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ];

  /// A list of this localizations delegate's supported locales.
  static const List<Locale> supportedLocales = <Locale>[
    Locale('en'),
    Locale('vi'),
  ];

  /// No description provided for @appTitle.
  ///
  /// In en, this message translates to:
  /// **'OneBrain'**
  String get appTitle;

  /// No description provided for @bootstrapEyebrow.
  ///
  /// In en, this message translates to:
  /// **'AUTONOMOUS MOBILE NODE'**
  String get bootstrapEyebrow;

  /// No description provided for @entryResolving.
  ///
  /// In en, this message translates to:
  /// **'Opening this node…'**
  String get entryResolving;

  /// No description provided for @welcomeTitle.
  ///
  /// In en, this message translates to:
  /// **'Grow ideas on your own node'**
  String get welcomeTitle;

  /// No description provided for @welcomeBody.
  ///
  /// In en, this message translates to:
  /// **'This phone is an independent OneBrain node. It is not a desktop companion and can keep private work local.'**
  String get welcomeBody;

  /// No description provided for @languageTitle.
  ///
  /// In en, this message translates to:
  /// **'Choose your interface language'**
  String get languageTitle;

  /// No description provided for @languageEnglish.
  ///
  /// In en, this message translates to:
  /// **'English'**
  String get languageEnglish;

  /// No description provided for @languageVietnamese.
  ///
  /// In en, this message translates to:
  /// **'Tiếng Việt'**
  String get languageVietnamese;

  /// No description provided for @nodeFactTitle.
  ///
  /// In en, this message translates to:
  /// **'Node boundary'**
  String get nodeFactTitle;

  /// No description provided for @nodeFactBody.
  ///
  /// In en, this message translates to:
  /// **'Flutter presents typed state. Native and Rust layers own platform work, durable data, policy, signing, and tools.'**
  String get nodeFactBody;

  /// No description provided for @registryFactTitle.
  ///
  /// In en, this message translates to:
  /// **'Required knowledge data'**
  String get registryFactTitle;

  /// No description provided for @registryFactBody.
  ///
  /// In en, this message translates to:
  /// **'Concept Registry data is not bundled in the app. A later Init flow downloads it only after an explicit plan and confirmation.'**
  String get registryFactBody;

  /// No description provided for @requestFactTitle.
  ///
  /// In en, this message translates to:
  /// **'Network before Init'**
  String get requestFactTitle;

  /// No description provided for @requestFactBody.
  ///
  /// In en, this message translates to:
  /// **'No Registry artifact request is made from this bootstrap screen.'**
  String get requestFactBody;

  /// No description provided for @nativeHostTitle.
  ///
  /// In en, this message translates to:
  /// **'Native host'**
  String get nativeHostTitle;

  /// No description provided for @nativeHostLoading.
  ///
  /// In en, this message translates to:
  /// **'Inspecting the device host…'**
  String get nativeHostLoading;

  /// No description provided for @nativeHostReady.
  ///
  /// In en, this message translates to:
  /// **'{platform} host ready · API {apiVersion}'**
  String nativeHostReady(String platform, String apiVersion);

  /// No description provided for @nativeHostUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Native host is unavailable in this environment. The bootstrap UI remains usable.'**
  String get nativeHostUnavailable;

  /// No description provided for @continueAction.
  ///
  /// In en, this message translates to:
  /// **'Continue to device preflight'**
  String get continueAction;

  /// No description provided for @galleryAction.
  ///
  /// In en, this message translates to:
  /// **'View shared components'**
  String get galleryAction;

  /// No description provided for @galleryTitle.
  ///
  /// In en, this message translates to:
  /// **'Shared component gallery'**
  String get galleryTitle;

  /// No description provided for @galleryBody.
  ///
  /// In en, this message translates to:
  /// **'These controls wrap Material 3 primitives and reuse OneBrain semantic tokens.'**
  String get galleryBody;

  /// No description provided for @primaryButton.
  ///
  /// In en, this message translates to:
  /// **'Primary action'**
  String get primaryButton;

  /// No description provided for @tonalButton.
  ///
  /// In en, this message translates to:
  /// **'Tonal action'**
  String get tonalButton;

  /// No description provided for @outlineButton.
  ///
  /// In en, this message translates to:
  /// **'Outline action'**
  String get outlineButton;

  /// No description provided for @statusReady.
  ///
  /// In en, this message translates to:
  /// **'Bootstrap UI ready'**
  String get statusReady;

  /// No description provided for @statusWaiting.
  ///
  /// In en, this message translates to:
  /// **'Registry Init not started'**
  String get statusWaiting;

  /// No description provided for @statusPrivate.
  ///
  /// In en, this message translates to:
  /// **'Private local scope'**
  String get statusPrivate;

  /// No description provided for @backAction.
  ///
  /// In en, this message translates to:
  /// **'Back'**
  String get backAction;

  /// No description provided for @notImplementedTitle.
  ///
  /// In en, this message translates to:
  /// **'Next foundation step'**
  String get notImplementedTitle;

  /// No description provided for @notImplementedBody.
  ///
  /// In en, this message translates to:
  /// **'Device storage preflight and explicit Init planning are intentionally not simulated in this slice.'**
  String get notImplementedBody;
}

class _AppLocalizationsDelegate
    extends LocalizationsDelegate<AppLocalizations> {
  const _AppLocalizationsDelegate();

  @override
  Future<AppLocalizations> load(Locale locale) {
    return SynchronousFuture<AppLocalizations>(lookupAppLocalizations(locale));
  }

  @override
  bool isSupported(Locale locale) =>
      <String>['en', 'vi'].contains(locale.languageCode);

  @override
  bool shouldReload(_AppLocalizationsDelegate old) => false;
}

AppLocalizations lookupAppLocalizations(Locale locale) {
  // Lookup logic when only language code is specified.
  switch (locale.languageCode) {
    case 'en':
      return AppLocalizationsEn();
    case 'vi':
      return AppLocalizationsVi();
  }

  throw FlutterError(
    'AppLocalizations.delegate failed to load unsupported locale "$locale". This is likely '
    'an issue with the localizations generation tool. Please file an issue '
    'on GitHub with a reproducible sample app and the gen-l10n configuration '
    'that was used.',
  );
}
