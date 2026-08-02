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
  String get rustBridgeTitle => 'Rust bridge';

  @override
  String get rustBridgeLoading => 'Checking the native-to-Rust boundary…';

  @override
  String rustBridgeReady(String coreVersion, int abiVersion) {
    return 'Rust bridge $coreVersion · ABI $abiVersion';
  }

  @override
  String get rustBridgeUnavailable =>
      'The Rust bridge is not linked in this build. No runtime readiness is claimed.';

  @override
  String get rustBridgeVerified => 'Typed round trip verified';

  @override
  String get rustBridgeNotVerified => 'Rust round trip unavailable';

  @override
  String get mobileRuntimeTitle => 'Mobile runtime profile';

  @override
  String get mobileRuntimeLoading => 'Opening the local BootstrapOnly runtime…';

  @override
  String get mobileRuntimeUnavailable =>
      'The local runtime could not open. No offline readiness is claimed.';

  @override
  String mobileRuntimeReady(
    String profileVersion,
    int generation,
    String phase,
    int grantCount,
    String registryState,
  ) {
    return 'Profile $profileVersion · generation $generation · $phase with $grantCount active grant(s). Registry state: $registryState. Device-bound identity and encrypted vault are active.';
  }

  @override
  String mobileRuntimeRecovered(int generation) {
    return 'Recovered generation $generation after the previous process ended without a quiesce callback. Stale callbacks remain fenced.';
  }

  @override
  String get mobileRuntimeVerified =>
      'Protected identity, encrypted vault and local runtime verified';

  @override
  String get mobileRuntimeNotVerified =>
      'Runtime profile verification incomplete';

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

  @override
  String onboardingStep(int current, int total) {
    return 'Step $current of $total';
  }

  @override
  String get onboardingProgressSaveError =>
      'Onboarding progress could not be saved. Please try again.';

  @override
  String get nextAction => 'Next';

  @override
  String get preflightTitle => 'Check the foundations';

  @override
  String get preflightBody =>
      'This check separates required local foundations from optional capabilities. Final Registry storage and network admission happen only after a signed Init plan exists.';

  @override
  String get preflightRuntimeTitle => 'Protected runtime';

  @override
  String get preflightRuntimeBody =>
      'The device-bound identity, encrypted vault and one Rust writer are available.';

  @override
  String get preflightStorageTitle => 'Required data is separate';

  @override
  String get preflightStorageBody =>
      'The app contains no Concept Registry release. The initial dataset is downloaded after launch and may use more than 2 GB including working space.';

  @override
  String get preflightOptionalTitle => 'Optional lanes stay off';

  @override
  String get preflightOptionalBody =>
      'Local or cloud AI, notifications and node networking are not required to save a private raw draft.';

  @override
  String get identityTitle => 'This installation is its own node';

  @override
  String get identityBody =>
      'OneBrain created independent Node, feed and Actor authority domains for this phone. It does not replicate or extend a desktop node.';

  @override
  String get identityReadyTitle => 'Independent authority';

  @override
  String get identityReadyBody =>
      'Private signing material stays behind the native and Rust boundary. Only typed public facts may reach the UI.';

  @override
  String get securityTitle => 'Private by default';

  @override
  String get securityBody =>
      'Raw capture starts as PrivateLocal. Backgrounding locks the private session; public and network transitions require separate confirmation.';

  @override
  String get securityVaultTitle => 'Encrypted local storage';

  @override
  String get securityVaultBody =>
      'Private drafts and validated private objects use device-bound encrypted stores excluded from generic OS backup.';

  @override
  String get initHandoffTitle => 'Add required Concept data after launch';

  @override
  String get initHandoffBody =>
      'Concept lookup, validation, KU encode, Library search and local KQL remain unavailable until one exact signed Registry release is verified and activated.';

  @override
  String get initHandoffLimitedTitle => 'Limited mode remains useful';

  @override
  String get initHandoffLimitedBody =>
      'You can capture and save encrypted raw text drafts now. Init, Operations, storage and diagnostics remain available.';

  @override
  String get openInitAction => 'Open required-data Init';

  @override
  String get limitedModeAction => 'Use Limited mode for now';

  @override
  String get initTitle => 'Required Concept data';

  @override
  String get initBody =>
      'Resolve the signed target and review exact storage and network facts before any large transfer.';

  @override
  String get initBoundaryTitle => 'Post-launch download';

  @override
  String get initBoundaryBody =>
      'concepts.obr and its indexes are never bundled in the APK or IPA. This screen does not simulate their presence.';

  @override
  String get initUnavailableAction => 'Begin Init';

  @override
  String get initUnavailableReason =>
      'No approved production Registry trust profile is available in this build.';

  @override
  String get initDevelopmentFixtureTitle => 'Development fixture';

  @override
  String get initDevelopmentFixtureBody =>
      'This emulator-only signed fixture tests admission. It is not production Concept data and cannot start a transfer.';

  @override
  String get initPlanTitle => 'Exact Init plan';

  @override
  String get initPlanSubtitle =>
      'The target and publisher floor are signed. Storage facts come from the destination filesystem.';

  @override
  String get initChannelLabel => 'Channel';

  @override
  String get initReleaseLabel => 'Release';

  @override
  String get initManifestLabel => 'Manifest';

  @override
  String get initHeadGenerationLabel => 'Head generation';

  @override
  String get initReleaseSequenceLabel => 'Release sequence';

  @override
  String get initPublisherFloorLabel => 'Publisher floor (P)';

  @override
  String get initArtifactBytesLabel => 'Signed artifacts';

  @override
  String get initTargetAllocationLabel => 'Target allocation (N)';

  @override
  String get initTransferPeakLabel => 'Transfer peak (T)';

  @override
  String get initVerificationWorkspaceLabel => 'Verification workspace (W)';

  @override
  String get initCatalogGrowthLabel => 'Catalog growth (G)';

  @override
  String get initSafetyReserveLabel => 'Safety reserve (R)';

  @override
  String get initResourceFactsTitle => 'Destination storage facts';

  @override
  String get initAvailableBytesLabel => 'Available now';

  @override
  String get initRequiredBytesLabel => 'Required before start';

  @override
  String get initVolumeCapacityLabel => 'Volume capacity';

  @override
  String get initCapacityReady => 'Capacity check passed';

  @override
  String get initCapacityInsufficient => 'More free space is required';

  @override
  String get initNetworkPolicyTitle => 'Network policy';

  @override
  String get initWifiOnlyPolicy => 'Wi-Fi only';

  @override
  String get initUnmeteredPolicy => 'Unmetered';

  @override
  String get initAnyNetworkPolicy => 'Any network';

  @override
  String get initOneTimeOverrideLabel =>
      'Allow this Init once on the selected network policy';

  @override
  String get initDeferAction => 'Defer and use Limited mode';

  @override
  String get initConfirmAction => 'Confirm exact plan';

  @override
  String get initPlanError =>
      'The signed Init plan could not be prepared. No transfer was started.';

  @override
  String get initErrorTitle => 'Init needs attention';

  @override
  String get initDeferError => 'Limited-mode receipt could not be saved.';

  @override
  String get initConfirmError =>
      'Confirmation failed during the live capacity recheck. No transfer was started.';

  @override
  String get initAdmittedTitle => 'Capacity admitted';

  @override
  String get initAdmittedBody =>
      'Rust bound the exact manifest, trust profile, storage plan and network policy. Transfer remains disabled in MOB-05A.';

  @override
  String get initWaitingStorageTitle => 'Waiting for storage';

  @override
  String get initWaitingStorageBody =>
      'The exact plan is durable, but current free space is below the required amount. No bytes were scheduled.';

  @override
  String get initTransportGated =>
      'Transfer is not enabled in this build slice.';

  @override
  String get homeTitle => 'Home';

  @override
  String get homeGreeting => 'A bright place for private ideas';

  @override
  String get limitedTitle => 'Limited mode';

  @override
  String get limitedBody =>
      'Your node is protected, but required Concept data is not active. Raw drafts work; Concept-dependent features stay honestly unavailable.';

  @override
  String get requiredInitTitle => 'Finish required data';

  @override
  String get requiredInitBody =>
      'Open Init to review the post-launch data boundary. No transfer starts from this card.';

  @override
  String get quickCaptureTitle => 'Capture a thought';

  @override
  String get quickCaptureBody =>
      'Save bounded text directly into the encrypted PrivateLocal draft store. No LLM or network is used.';

  @override
  String get captureAction => 'Capture text';

  @override
  String get draftCountTitle => 'Encrypted drafts';

  @override
  String draftCountBody(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count private raw drafts saved on this device.',
      one: '1 private raw draft saved on this device.',
      zero: 'No private raw drafts saved yet.',
    );
    return '$_temp0';
  }

  @override
  String get operationsTitle => 'Operations';

  @override
  String get operationsBody =>
      'No Registry, model, import, backup, sync or seed operation is active.';

  @override
  String get navHome => 'Home';

  @override
  String get navLibrary => 'Library';

  @override
  String get navCapture => 'Capture';

  @override
  String get navAssistant => 'Assistant';

  @override
  String get navSettings => 'Settings';

  @override
  String get libraryTitle => 'Library';

  @override
  String get libraryBody =>
      'Local shelves keep origin, acquisition, retention and semantic state separate. This clean Limited node has no active Concept release.';

  @override
  String get myKnowledgeTitle => 'My / local KU';

  @override
  String get myKnowledgeBody =>
      'Private KU browse and detail become available after Registry activation and deterministic validation.';

  @override
  String get receivedKnowledgeTitle => 'Received KU';

  @override
  String get receivedKnowledgeBody =>
      'Received shelves require the Networked Mobile Beta gate; they are not simulated locally.';

  @override
  String get mediaLibraryTitle => 'My media';

  @override
  String get mediaLibraryBody =>
      'Browse encrypted originals owned by this node. Private ownership does not make media shared or seedable.';

  @override
  String get myMediaShelfBody =>
      'This bounded shelf is read from the Rust-owned encrypted catalog. Each entry shows verified local bytes and its exact retention class.';

  @override
  String get myMediaPrivateTitle => 'Owned here, not published';

  @override
  String get myMediaPrivateBody =>
      'OwnedOriginal keeps a durable hold against local garbage collection. It is still PrivateLocal and has no share representation, access grant or seed eligibility.';

  @override
  String get myMediaLoadingTitle => 'Opening encrypted media catalog';

  @override
  String get myMediaLoadingBody =>
      'The native host is querying bounded metadata; no media bytes or storage paths enter Flutter.';

  @override
  String get myMediaLoadError =>
      'The encrypted media catalog could not be inspected. No ownership or availability claim was inferred.';

  @override
  String get myMediaEmptyTitle => 'No owned media yet';

  @override
  String get myMediaEmptyBody =>
      'Import a photo, PDF, audio file or video through the system picker. OneBrain will verify and activate it before adding this shelf reference.';

  @override
  String myMediaItemTitle(String mediaClass) {
    return 'Owned $mediaClass';
  }

  @override
  String myMediaVerifiedBytes(int verifiedBytes, int contentBytes) {
    return '$verifiedBytes of $contentBytes bytes verified locally';
  }

  @override
  String get storageClassOwnedOriginal => 'OwnedOriginal';

  @override
  String get mediaOwnedHoldProtected => 'Protected by owned hold';

  @override
  String get mediaOwnedHoldMissing => 'Owned hold missing';

  @override
  String get mediaClassImage => 'image';

  @override
  String get mediaClassVideo => 'video';

  @override
  String get mediaClassAudio => 'audio';

  @override
  String get mediaClassDocument => 'document';

  @override
  String get retryAction => 'Try again';

  @override
  String get conceptsTitle => 'Concepts, search and KQL';

  @override
  String get conceptsBody =>
      'These routes require one healthy active Concept Registry release.';

  @override
  String get registryRequiredReason =>
      'Required Concept Registry data is not active.';

  @override
  String get networkBetaReason =>
      'Node networking is disabled until the Networked Mobile Beta gate.';

  @override
  String get captureTitle => 'Capture';

  @override
  String get captureBody =>
      'Every source begins as PrivateLocal. Derived text or candidates never overwrite the owned original.';

  @override
  String get textCaptureTitle => 'Text or clipboard';

  @override
  String get textCaptureBody =>
      'Compose bounded text and save it directly into the encrypted raw-draft store.';

  @override
  String get shareCaptureTitle => 'Share into OneBrain';

  @override
  String get shareCaptureBody =>
      'Text shared from another app lands in an encrypted private spool. Review its type and size before importing it as a draft.';

  @override
  String get shareSpoolTitle => 'Shared into OneBrain';

  @override
  String get shareSpoolBody =>
      'Incoming content stays encrypted and private. Opening this screen does not import, encode, publish or send anything.';

  @override
  String get shareSpoolEmptyTitle => 'No pending shared content';

  @override
  String get shareSpoolEmptyBody =>
      'Use the system Share action in another app and choose OneBrain. Plain text is supported in this foundation slice.';

  @override
  String get shareSpoolItemTitle => 'Private shared text';

  @override
  String shareSpoolItemBody(String mimeType, int bytes) {
    return '$mimeType · $bytes bytes';
  }

  @override
  String get shareSpoolImportAction => 'Import as private draft';

  @override
  String shareSpoolImported(String draftRef) {
    return 'Shared text was imported into encrypted draft $draftRef.';
  }

  @override
  String get shareSpoolLoadError =>
      'Pending shared content could not be inspected.';

  @override
  String get shareSpoolImportError =>
      'Shared text could not be imported. It remains safely pending.';

  @override
  String get fileCaptureTitle => 'Photo, video, document or audio';

  @override
  String get fileCaptureBody =>
      'Choose through the system picker. Native streams the source directly into bounded encrypted Rust staging; no path or source bytes enter Flutter.';

  @override
  String get mediaImportTitle => 'Import private media';

  @override
  String get mediaImportBody =>
      'Choose one source with the system picker. OneBrain streams, verifies and encrypts it, activates immutable local bytes, then commits an OwnedOriginal reference.';

  @override
  String get mediaImportBoundaryTitle => 'Owned is not shared';

  @override
  String get mediaImportBoundaryBody =>
      'The committed original remains PrivateLocal. Import does not attach it to a KU, derive a share representation, publish it or make it seedable.';

  @override
  String get mediaPickImageTitle => 'Photo or image';

  @override
  String get mediaPickImageBody =>
      'Select an image. Its actual type is detected from bytes rather than its filename.';

  @override
  String get mediaPickVideoTitle => 'Video';

  @override
  String get mediaPickVideoBody =>
      'Select one video for foreground, encrypted streaming.';

  @override
  String get mediaPickAudioTitle => 'Audio';

  @override
  String get mediaPickAudioBody =>
      'Select one audio source through the device picker.';

  @override
  String get mediaPickDocumentTitle => 'PDF document';

  @override
  String get mediaPickDocumentBody =>
      'This foundation slice accepts verified PDF bytes and rejects archives or disguised files.';

  @override
  String get mediaPickAction => 'Choose with system picker';

  @override
  String get mediaPickBusy =>
      'Encrypting, verifying and activating on this device…';

  @override
  String get mediaStageReadyTitle => 'Owned original protected';

  @override
  String mediaStageReadyBody(
    String mimeType,
    int bytes,
    String storageClass,
    String mediaRef,
  ) {
    return '$mimeType · $bytes verified bytes · $storageClass. Opaque media reference: $mediaRef.';
  }

  @override
  String get mediaStageError =>
      'The selected source was cancelled, unreadable, unsupported or did not match its claimed type. No unverified catalog reference was kept.';

  @override
  String get textComposerTitle => 'Private text draft';

  @override
  String get textComposerBody =>
      'This source is saved only on this device. Saving does not encode a KU, publish, share or invoke AI.';

  @override
  String get contentLanguageLabel => 'Content language';

  @override
  String get draftTextLabel => 'Your text';

  @override
  String get draftTextHint => 'Write or paste a thought…';

  @override
  String get savePrivateDraftAction => 'Save private draft';

  @override
  String get draftSavedTitle => 'Saved on this device';

  @override
  String draftSavedBody(int bytes, int count) {
    return '$bytes encrypted source bytes saved. The private store now contains $count draft(s).';
  }

  @override
  String get draftSaveError =>
      'The private draft could not be saved. Your text remains in the editor.';

  @override
  String get draftBlankError => 'Enter some text before saving.';

  @override
  String get assistantTitle => 'Assistant';

  @override
  String get assistantBody =>
      'The deterministic no-LLM baseline is preserved. Local, system and cloud LLM routes are separate optional packages and are currently off.';

  @override
  String get settingsTitle => 'Settings';

  @override
  String get settingsBody =>
      'Inspect protected runtime, required data, storage and optional capability boundaries.';

  @override
  String get runtimeSettingsTitle => 'Runtime and diagnostics';

  @override
  String get runtimeSettingsBody =>
      'BootstrapOnly profile, protected identity, one writer and redacted security history.';

  @override
  String get registrySettingsTitle => 'Concept Registry';

  @override
  String get registrySettingsBody =>
      'No release is active and no Registry request has been issued.';

  @override
  String get storageSettingsTitle => 'Storage';

  @override
  String get storageSettingsBody =>
      'Protected drafts, Registry, model, media, staging and reclaimable bytes stay separate.';

  @override
  String get backupSettingsTitle => 'Encrypted backup and export';

  @override
  String get backupSettingsBody =>
      'The versioned authenticated archive engine is present; user-selected destination wiring remains gated.';

  @override
  String get languageSettingsTitle => 'Language and accessibility';

  @override
  String get languageSettingsBody =>
      'English and Vietnamese UI, system text scaling, contrast and Reduce Motion use the shared design contract.';

  @override
  String get unavailableTitle => 'Feature not available yet';

  @override
  String get backHomeAction => 'Back to Home';

  @override
  String get initLocalImportTitle =>
      'Import the signed Registry from this device';

  @override
  String get initLocalImportBody =>
      'Choose each role-scoped artifact with the Android system picker. Native streams directly to Rust, which resumes and verifies every signed chunk. File names, Flutter and the picker never grant content authority.';

  @override
  String get initLocalImportResumeNote =>
      'Keep OneBrain in the foreground while a file is streaming. If Android stops the app, choose the same artifact again and verification resumes from durable bytes.';

  @override
  String get initLocalImportSourceStatus => 'Local Import · no server required';

  @override
  String get initLocalImportConceptsTitle => 'Concept data (concepts.obr)';

  @override
  String get initLocalImportLabelsTitle => 'Labels index';

  @override
  String get initLocalImportCcidsTitle => 'CCID index';

  @override
  String get initLocalImportRoleBody =>
      'Select the exact artifact for this signed role. Extra, missing or altered bytes are rejected.';

  @override
  String get initLocalImportAction => 'Choose and verify file';

  @override
  String get initLocalImportRoleComplete => 'Role verified';

  @override
  String get initLocalImportRolePending => 'Not imported';

  @override
  String initLocalImportProgress(
    int verifiedChunks,
    int totalChunks,
    int verifiedBytes,
    int expectedBytes,
  ) {
    return '$verifiedChunks/$totalChunks chunks · $verifiedBytes/$expectedBytes bytes verified';
  }

  @override
  String get initLocalImportAllBytesTitle => 'All Registry bytes verified';

  @override
  String get initLocalImportAllBytesBody =>
      'The three signed artifacts are complete in durable landing storage. Activation remains gated by the next MOB-05C whole-artifact, KQL smoke and atomic pointer checks.';

  @override
  String get initLocalImportError =>
      'The selected artifact was cancelled, unreadable, too short, too long or did not match the signed chunk ledger. Verified durable bytes were kept for resume.';
}
