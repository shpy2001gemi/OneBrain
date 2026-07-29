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

  /// No description provided for @rustBridgeTitle.
  ///
  /// In en, this message translates to:
  /// **'Rust bridge'**
  String get rustBridgeTitle;

  /// No description provided for @rustBridgeLoading.
  ///
  /// In en, this message translates to:
  /// **'Checking the native-to-Rust boundary…'**
  String get rustBridgeLoading;

  /// No description provided for @rustBridgeReady.
  ///
  /// In en, this message translates to:
  /// **'Rust bridge {coreVersion} · ABI {abiVersion}'**
  String rustBridgeReady(String coreVersion, int abiVersion);

  /// No description provided for @rustBridgeUnavailable.
  ///
  /// In en, this message translates to:
  /// **'The Rust bridge is not linked in this build. No runtime readiness is claimed.'**
  String get rustBridgeUnavailable;

  /// No description provided for @rustBridgeVerified.
  ///
  /// In en, this message translates to:
  /// **'Typed round trip verified'**
  String get rustBridgeVerified;

  /// No description provided for @rustBridgeNotVerified.
  ///
  /// In en, this message translates to:
  /// **'Rust round trip unavailable'**
  String get rustBridgeNotVerified;

  /// No description provided for @mobileRuntimeTitle.
  ///
  /// In en, this message translates to:
  /// **'Mobile runtime profile'**
  String get mobileRuntimeTitle;

  /// No description provided for @mobileRuntimeLoading.
  ///
  /// In en, this message translates to:
  /// **'Opening the local BootstrapOnly runtime…'**
  String get mobileRuntimeLoading;

  /// No description provided for @mobileRuntimeUnavailable.
  ///
  /// In en, this message translates to:
  /// **'The local runtime could not open. No offline readiness is claimed.'**
  String get mobileRuntimeUnavailable;

  /// No description provided for @mobileRuntimeReady.
  ///
  /// In en, this message translates to:
  /// **'Profile {profileVersion} · generation {generation} · {phase} with {grantCount} active grant(s). Registry state: {registryState}. Device-bound identity and encrypted vault are active.'**
  String mobileRuntimeReady(
    String profileVersion,
    int generation,
    String phase,
    int grantCount,
    String registryState,
  );

  /// No description provided for @mobileRuntimeRecovered.
  ///
  /// In en, this message translates to:
  /// **'Recovered generation {generation} after the previous process ended without a quiesce callback. Stale callbacks remain fenced.'**
  String mobileRuntimeRecovered(int generation);

  /// No description provided for @mobileRuntimeVerified.
  ///
  /// In en, this message translates to:
  /// **'Protected identity, encrypted vault and local runtime verified'**
  String get mobileRuntimeVerified;

  /// No description provided for @mobileRuntimeNotVerified.
  ///
  /// In en, this message translates to:
  /// **'Runtime profile verification incomplete'**
  String get mobileRuntimeNotVerified;

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

  /// No description provided for @onboardingStep.
  ///
  /// In en, this message translates to:
  /// **'Step {current} of {total}'**
  String onboardingStep(int current, int total);

  /// No description provided for @onboardingProgressSaveError.
  ///
  /// In en, this message translates to:
  /// **'Onboarding progress could not be saved. Please try again.'**
  String get onboardingProgressSaveError;

  /// No description provided for @nextAction.
  ///
  /// In en, this message translates to:
  /// **'Next'**
  String get nextAction;

  /// No description provided for @preflightTitle.
  ///
  /// In en, this message translates to:
  /// **'Check the foundations'**
  String get preflightTitle;

  /// No description provided for @preflightBody.
  ///
  /// In en, this message translates to:
  /// **'This check separates required local foundations from optional capabilities. Final Registry storage and network admission happen only after a signed Init plan exists.'**
  String get preflightBody;

  /// No description provided for @preflightRuntimeTitle.
  ///
  /// In en, this message translates to:
  /// **'Protected runtime'**
  String get preflightRuntimeTitle;

  /// No description provided for @preflightRuntimeBody.
  ///
  /// In en, this message translates to:
  /// **'The device-bound identity, encrypted vault and one Rust writer are available.'**
  String get preflightRuntimeBody;

  /// No description provided for @preflightStorageTitle.
  ///
  /// In en, this message translates to:
  /// **'Required data is separate'**
  String get preflightStorageTitle;

  /// No description provided for @preflightStorageBody.
  ///
  /// In en, this message translates to:
  /// **'The app contains no Concept Registry release. The initial dataset is downloaded after launch and may use more than 2 GB including working space.'**
  String get preflightStorageBody;

  /// No description provided for @preflightOptionalTitle.
  ///
  /// In en, this message translates to:
  /// **'Optional lanes stay off'**
  String get preflightOptionalTitle;

  /// No description provided for @preflightOptionalBody.
  ///
  /// In en, this message translates to:
  /// **'Local or cloud AI, notifications and node networking are not required to save a private raw draft.'**
  String get preflightOptionalBody;

  /// No description provided for @identityTitle.
  ///
  /// In en, this message translates to:
  /// **'This installation is its own node'**
  String get identityTitle;

  /// No description provided for @identityBody.
  ///
  /// In en, this message translates to:
  /// **'OneBrain created independent Node, feed and Actor authority domains for this phone. It does not replicate or extend a desktop node.'**
  String get identityBody;

  /// No description provided for @identityReadyTitle.
  ///
  /// In en, this message translates to:
  /// **'Independent authority'**
  String get identityReadyTitle;

  /// No description provided for @identityReadyBody.
  ///
  /// In en, this message translates to:
  /// **'Private signing material stays behind the native and Rust boundary. Only typed public facts may reach the UI.'**
  String get identityReadyBody;

  /// No description provided for @securityTitle.
  ///
  /// In en, this message translates to:
  /// **'Private by default'**
  String get securityTitle;

  /// No description provided for @securityBody.
  ///
  /// In en, this message translates to:
  /// **'Raw capture starts as PrivateLocal. Backgrounding locks the private session; public and network transitions require separate confirmation.'**
  String get securityBody;

  /// No description provided for @securityVaultTitle.
  ///
  /// In en, this message translates to:
  /// **'Encrypted local storage'**
  String get securityVaultTitle;

  /// No description provided for @securityVaultBody.
  ///
  /// In en, this message translates to:
  /// **'Private drafts and validated private objects use device-bound encrypted stores excluded from generic OS backup.'**
  String get securityVaultBody;

  /// No description provided for @initHandoffTitle.
  ///
  /// In en, this message translates to:
  /// **'Add required Concept data after launch'**
  String get initHandoffTitle;

  /// No description provided for @initHandoffBody.
  ///
  /// In en, this message translates to:
  /// **'Concept lookup, validation, KU encode, Library search and local KQL remain unavailable until one exact signed Registry release is verified and activated.'**
  String get initHandoffBody;

  /// No description provided for @initHandoffLimitedTitle.
  ///
  /// In en, this message translates to:
  /// **'Limited mode remains useful'**
  String get initHandoffLimitedTitle;

  /// No description provided for @initHandoffLimitedBody.
  ///
  /// In en, this message translates to:
  /// **'You can capture and save encrypted raw text drafts now. Init, Operations, storage and diagnostics remain available.'**
  String get initHandoffLimitedBody;

  /// No description provided for @openInitAction.
  ///
  /// In en, this message translates to:
  /// **'Open required-data Init'**
  String get openInitAction;

  /// No description provided for @limitedModeAction.
  ///
  /// In en, this message translates to:
  /// **'Use Limited mode for now'**
  String get limitedModeAction;

  /// No description provided for @initTitle.
  ///
  /// In en, this message translates to:
  /// **'Required Concept data'**
  String get initTitle;

  /// No description provided for @initBody.
  ///
  /// In en, this message translates to:
  /// **'No Registry request has been made. MOB-05 will resolve the signed target and show exact bytes, capacity, network and energy facts before any large transfer.'**
  String get initBody;

  /// No description provided for @initBoundaryTitle.
  ///
  /// In en, this message translates to:
  /// **'Post-launch download'**
  String get initBoundaryTitle;

  /// No description provided for @initBoundaryBody.
  ///
  /// In en, this message translates to:
  /// **'concepts.obr and its indexes are never bundled in the APK or IPA. This screen does not simulate their presence.'**
  String get initBoundaryBody;

  /// No description provided for @initUnavailableAction.
  ///
  /// In en, this message translates to:
  /// **'Begin Init'**
  String get initUnavailableAction;

  /// No description provided for @initUnavailableReason.
  ///
  /// In en, this message translates to:
  /// **'Signed Registry planning and transfer are not active in this build.'**
  String get initUnavailableReason;

  /// No description provided for @homeTitle.
  ///
  /// In en, this message translates to:
  /// **'Home'**
  String get homeTitle;

  /// No description provided for @homeGreeting.
  ///
  /// In en, this message translates to:
  /// **'A bright place for private ideas'**
  String get homeGreeting;

  /// No description provided for @limitedTitle.
  ///
  /// In en, this message translates to:
  /// **'Limited mode'**
  String get limitedTitle;

  /// No description provided for @limitedBody.
  ///
  /// In en, this message translates to:
  /// **'Your node is protected, but required Concept data is not active. Raw drafts work; Concept-dependent features stay honestly unavailable.'**
  String get limitedBody;

  /// No description provided for @requiredInitTitle.
  ///
  /// In en, this message translates to:
  /// **'Finish required data'**
  String get requiredInitTitle;

  /// No description provided for @requiredInitBody.
  ///
  /// In en, this message translates to:
  /// **'Open Init to review the post-launch data boundary. No transfer starts from this card.'**
  String get requiredInitBody;

  /// No description provided for @quickCaptureTitle.
  ///
  /// In en, this message translates to:
  /// **'Capture a thought'**
  String get quickCaptureTitle;

  /// No description provided for @quickCaptureBody.
  ///
  /// In en, this message translates to:
  /// **'Save bounded text directly into the encrypted PrivateLocal draft store. No LLM or network is used.'**
  String get quickCaptureBody;

  /// No description provided for @captureAction.
  ///
  /// In en, this message translates to:
  /// **'Capture text'**
  String get captureAction;

  /// No description provided for @draftCountTitle.
  ///
  /// In en, this message translates to:
  /// **'Encrypted drafts'**
  String get draftCountTitle;

  /// No description provided for @draftCountBody.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, =0{No private raw drafts saved yet.} =1{1 private raw draft saved on this device.} other{{count} private raw drafts saved on this device.}}'**
  String draftCountBody(int count);

  /// No description provided for @operationsTitle.
  ///
  /// In en, this message translates to:
  /// **'Operations'**
  String get operationsTitle;

  /// No description provided for @operationsBody.
  ///
  /// In en, this message translates to:
  /// **'No Registry, model, import, backup, sync or seed operation is active.'**
  String get operationsBody;

  /// No description provided for @navHome.
  ///
  /// In en, this message translates to:
  /// **'Home'**
  String get navHome;

  /// No description provided for @navLibrary.
  ///
  /// In en, this message translates to:
  /// **'Library'**
  String get navLibrary;

  /// No description provided for @navCapture.
  ///
  /// In en, this message translates to:
  /// **'Capture'**
  String get navCapture;

  /// No description provided for @navAssistant.
  ///
  /// In en, this message translates to:
  /// **'Assistant'**
  String get navAssistant;

  /// No description provided for @navSettings.
  ///
  /// In en, this message translates to:
  /// **'Settings'**
  String get navSettings;

  /// No description provided for @libraryTitle.
  ///
  /// In en, this message translates to:
  /// **'Library'**
  String get libraryTitle;

  /// No description provided for @libraryBody.
  ///
  /// In en, this message translates to:
  /// **'Local shelves keep origin, acquisition, retention and semantic state separate. This clean Limited node has no active Concept release.'**
  String get libraryBody;

  /// No description provided for @myKnowledgeTitle.
  ///
  /// In en, this message translates to:
  /// **'My / local KU'**
  String get myKnowledgeTitle;

  /// No description provided for @myKnowledgeBody.
  ///
  /// In en, this message translates to:
  /// **'Private KU browse and detail become available after Registry activation and deterministic validation.'**
  String get myKnowledgeBody;

  /// No description provided for @receivedKnowledgeTitle.
  ///
  /// In en, this message translates to:
  /// **'Received KU'**
  String get receivedKnowledgeTitle;

  /// No description provided for @receivedKnowledgeBody.
  ///
  /// In en, this message translates to:
  /// **'Received shelves require the Networked Mobile Beta gate; they are not simulated locally.'**
  String get receivedKnowledgeBody;

  /// No description provided for @mediaLibraryTitle.
  ///
  /// In en, this message translates to:
  /// **'My media'**
  String get mediaLibraryTitle;

  /// No description provided for @mediaLibraryBody.
  ///
  /// In en, this message translates to:
  /// **'Owned originals and derived media will use verified encrypted storage. Media ingestion is not active in this slice.'**
  String get mediaLibraryBody;

  /// No description provided for @conceptsTitle.
  ///
  /// In en, this message translates to:
  /// **'Concepts, search and KQL'**
  String get conceptsTitle;

  /// No description provided for @conceptsBody.
  ///
  /// In en, this message translates to:
  /// **'These routes require one healthy active Concept Registry release.'**
  String get conceptsBody;

  /// No description provided for @registryRequiredReason.
  ///
  /// In en, this message translates to:
  /// **'Required Concept Registry data is not active.'**
  String get registryRequiredReason;

  /// No description provided for @networkBetaReason.
  ///
  /// In en, this message translates to:
  /// **'Node networking is disabled until the Networked Mobile Beta gate.'**
  String get networkBetaReason;

  /// No description provided for @captureTitle.
  ///
  /// In en, this message translates to:
  /// **'Capture'**
  String get captureTitle;

  /// No description provided for @captureBody.
  ///
  /// In en, this message translates to:
  /// **'Every source begins as PrivateLocal. Derived text or candidates never overwrite the owned original.'**
  String get captureBody;

  /// No description provided for @textCaptureTitle.
  ///
  /// In en, this message translates to:
  /// **'Text or clipboard'**
  String get textCaptureTitle;

  /// No description provided for @textCaptureBody.
  ///
  /// In en, this message translates to:
  /// **'Compose bounded text and save it directly into the encrypted raw-draft store.'**
  String get textCaptureBody;

  /// No description provided for @shareCaptureTitle.
  ///
  /// In en, this message translates to:
  /// **'Share into OneBrain'**
  String get shareCaptureTitle;

  /// No description provided for @shareCaptureBody.
  ///
  /// In en, this message translates to:
  /// **'Text shared from another app lands in an encrypted private spool. Review its type and size before importing it as a draft.'**
  String get shareCaptureBody;

  /// No description provided for @shareSpoolTitle.
  ///
  /// In en, this message translates to:
  /// **'Shared into OneBrain'**
  String get shareSpoolTitle;

  /// No description provided for @shareSpoolBody.
  ///
  /// In en, this message translates to:
  /// **'Incoming content stays encrypted and private. Opening this screen does not import, encode, publish or send anything.'**
  String get shareSpoolBody;

  /// No description provided for @shareSpoolEmptyTitle.
  ///
  /// In en, this message translates to:
  /// **'No pending shared content'**
  String get shareSpoolEmptyTitle;

  /// No description provided for @shareSpoolEmptyBody.
  ///
  /// In en, this message translates to:
  /// **'Use the system Share action in another app and choose OneBrain. Plain text is supported in this foundation slice.'**
  String get shareSpoolEmptyBody;

  /// No description provided for @shareSpoolItemTitle.
  ///
  /// In en, this message translates to:
  /// **'Private shared text'**
  String get shareSpoolItemTitle;

  /// No description provided for @shareSpoolItemBody.
  ///
  /// In en, this message translates to:
  /// **'{mimeType} · {bytes} bytes'**
  String shareSpoolItemBody(String mimeType, int bytes);

  /// No description provided for @shareSpoolImportAction.
  ///
  /// In en, this message translates to:
  /// **'Import as private draft'**
  String get shareSpoolImportAction;

  /// No description provided for @shareSpoolImported.
  ///
  /// In en, this message translates to:
  /// **'Shared text was imported into encrypted draft {draftRef}.'**
  String shareSpoolImported(String draftRef);

  /// No description provided for @shareSpoolLoadError.
  ///
  /// In en, this message translates to:
  /// **'Pending shared content could not be inspected.'**
  String get shareSpoolLoadError;

  /// No description provided for @shareSpoolImportError.
  ///
  /// In en, this message translates to:
  /// **'Shared text could not be imported. It remains safely pending.'**
  String get shareSpoolImportError;

  /// No description provided for @fileCaptureTitle.
  ///
  /// In en, this message translates to:
  /// **'Photo, video, document or audio'**
  String get fileCaptureTitle;

  /// No description provided for @fileCaptureBody.
  ///
  /// In en, this message translates to:
  /// **'Choose through the system picker. Native streams the source directly into bounded encrypted Rust staging; no path or source bytes enter Flutter.'**
  String get fileCaptureBody;

  /// No description provided for @mediaImportTitle.
  ///
  /// In en, this message translates to:
  /// **'Import private media'**
  String get mediaImportTitle;

  /// No description provided for @mediaImportBody.
  ///
  /// In en, this message translates to:
  /// **'Choose one source with the system picker. OneBrain verifies its bytes, encrypts every chunk and returns only an opaque local reference.'**
  String get mediaImportBody;

  /// No description provided for @mediaImportBoundaryTitle.
  ///
  /// In en, this message translates to:
  /// **'Staging is not publication'**
  String get mediaImportBoundaryTitle;

  /// No description provided for @mediaImportBoundaryBody.
  ///
  /// In en, this message translates to:
  /// **'A verified stage remains PrivateLocal. It is not yet an OwnedOriginal, KU attachment, shared object or seedable media pack.'**
  String get mediaImportBoundaryBody;

  /// No description provided for @mediaPickImageTitle.
  ///
  /// In en, this message translates to:
  /// **'Photo or image'**
  String get mediaPickImageTitle;

  /// No description provided for @mediaPickImageBody.
  ///
  /// In en, this message translates to:
  /// **'Select an image. Its actual type is detected from bytes rather than its filename.'**
  String get mediaPickImageBody;

  /// No description provided for @mediaPickVideoTitle.
  ///
  /// In en, this message translates to:
  /// **'Video'**
  String get mediaPickVideoTitle;

  /// No description provided for @mediaPickVideoBody.
  ///
  /// In en, this message translates to:
  /// **'Select one video for foreground, encrypted streaming.'**
  String get mediaPickVideoBody;

  /// No description provided for @mediaPickAudioTitle.
  ///
  /// In en, this message translates to:
  /// **'Audio'**
  String get mediaPickAudioTitle;

  /// No description provided for @mediaPickAudioBody.
  ///
  /// In en, this message translates to:
  /// **'Select one audio source through the device picker.'**
  String get mediaPickAudioBody;

  /// No description provided for @mediaPickDocumentTitle.
  ///
  /// In en, this message translates to:
  /// **'PDF document'**
  String get mediaPickDocumentTitle;

  /// No description provided for @mediaPickDocumentBody.
  ///
  /// In en, this message translates to:
  /// **'This foundation slice accepts verified PDF bytes and rejects archives or disguised files.'**
  String get mediaPickDocumentBody;

  /// No description provided for @mediaPickAction.
  ///
  /// In en, this message translates to:
  /// **'Choose with system picker'**
  String get mediaPickAction;

  /// No description provided for @mediaPickBusy.
  ///
  /// In en, this message translates to:
  /// **'Encrypting and verifying on this device…'**
  String get mediaPickBusy;

  /// No description provided for @mediaStageReadyTitle.
  ///
  /// In en, this message translates to:
  /// **'Encrypted stage verified'**
  String get mediaStageReadyTitle;

  /// No description provided for @mediaStageReadyBody.
  ///
  /// In en, this message translates to:
  /// **'{mimeType} · {bytes} bytes · BLAKE3 {digestShort}. Opaque source: {sourceRef}.'**
  String mediaStageReadyBody(
    String mimeType,
    int bytes,
    String digestShort,
    String sourceRef,
  );

  /// No description provided for @mediaStageError.
  ///
  /// In en, this message translates to:
  /// **'The selected source was cancelled, unreadable, unsupported or did not match its claimed type. No unverified stage was kept.'**
  String get mediaStageError;

  /// No description provided for @textComposerTitle.
  ///
  /// In en, this message translates to:
  /// **'Private text draft'**
  String get textComposerTitle;

  /// No description provided for @textComposerBody.
  ///
  /// In en, this message translates to:
  /// **'This source is saved only on this device. Saving does not encode a KU, publish, share or invoke AI.'**
  String get textComposerBody;

  /// No description provided for @contentLanguageLabel.
  ///
  /// In en, this message translates to:
  /// **'Content language'**
  String get contentLanguageLabel;

  /// No description provided for @draftTextLabel.
  ///
  /// In en, this message translates to:
  /// **'Your text'**
  String get draftTextLabel;

  /// No description provided for @draftTextHint.
  ///
  /// In en, this message translates to:
  /// **'Write or paste a thought…'**
  String get draftTextHint;

  /// No description provided for @savePrivateDraftAction.
  ///
  /// In en, this message translates to:
  /// **'Save private draft'**
  String get savePrivateDraftAction;

  /// No description provided for @draftSavedTitle.
  ///
  /// In en, this message translates to:
  /// **'Saved on this device'**
  String get draftSavedTitle;

  /// No description provided for @draftSavedBody.
  ///
  /// In en, this message translates to:
  /// **'{bytes} encrypted source bytes saved. The private store now contains {count} draft(s).'**
  String draftSavedBody(int bytes, int count);

  /// No description provided for @draftSaveError.
  ///
  /// In en, this message translates to:
  /// **'The private draft could not be saved. Your text remains in the editor.'**
  String get draftSaveError;

  /// No description provided for @draftBlankError.
  ///
  /// In en, this message translates to:
  /// **'Enter some text before saving.'**
  String get draftBlankError;

  /// No description provided for @assistantTitle.
  ///
  /// In en, this message translates to:
  /// **'Assistant'**
  String get assistantTitle;

  /// No description provided for @assistantBody.
  ///
  /// In en, this message translates to:
  /// **'The deterministic no-LLM baseline is preserved. Local, system and cloud LLM routes are separate optional packages and are currently off.'**
  String get assistantBody;

  /// No description provided for @settingsTitle.
  ///
  /// In en, this message translates to:
  /// **'Settings'**
  String get settingsTitle;

  /// No description provided for @settingsBody.
  ///
  /// In en, this message translates to:
  /// **'Inspect protected runtime, required data, storage and optional capability boundaries.'**
  String get settingsBody;

  /// No description provided for @runtimeSettingsTitle.
  ///
  /// In en, this message translates to:
  /// **'Runtime and diagnostics'**
  String get runtimeSettingsTitle;

  /// No description provided for @runtimeSettingsBody.
  ///
  /// In en, this message translates to:
  /// **'BootstrapOnly profile, protected identity, one writer and redacted security history.'**
  String get runtimeSettingsBody;

  /// No description provided for @registrySettingsTitle.
  ///
  /// In en, this message translates to:
  /// **'Concept Registry'**
  String get registrySettingsTitle;

  /// No description provided for @registrySettingsBody.
  ///
  /// In en, this message translates to:
  /// **'No release is active and no Registry request has been issued.'**
  String get registrySettingsBody;

  /// No description provided for @storageSettingsTitle.
  ///
  /// In en, this message translates to:
  /// **'Storage'**
  String get storageSettingsTitle;

  /// No description provided for @storageSettingsBody.
  ///
  /// In en, this message translates to:
  /// **'Protected drafts, Registry, model, media, staging and reclaimable bytes stay separate.'**
  String get storageSettingsBody;

  /// No description provided for @backupSettingsTitle.
  ///
  /// In en, this message translates to:
  /// **'Encrypted backup and export'**
  String get backupSettingsTitle;

  /// No description provided for @backupSettingsBody.
  ///
  /// In en, this message translates to:
  /// **'The versioned authenticated archive engine is present; user-selected destination wiring remains gated.'**
  String get backupSettingsBody;

  /// No description provided for @languageSettingsTitle.
  ///
  /// In en, this message translates to:
  /// **'Language and accessibility'**
  String get languageSettingsTitle;

  /// No description provided for @languageSettingsBody.
  ///
  /// In en, this message translates to:
  /// **'English and Vietnamese UI, system text scaling, contrast and Reduce Motion use the shared design contract.'**
  String get languageSettingsBody;

  /// No description provided for @unavailableTitle.
  ///
  /// In en, this message translates to:
  /// **'Feature not available yet'**
  String get unavailableTitle;

  /// No description provided for @backHomeAction.
  ///
  /// In en, this message translates to:
  /// **'Back to Home'**
  String get backHomeAction;
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
